//! Content and updates: what to fetch, where it waits, and how it goes live.
//!
//! # Everything hangs off the one signed object
//!
//! `nsr.release.v1` is the only thing that is ever signed, and it pins every
//! `nsr.component.v1` manifest by SHA-256. So a component manifest cannot be
//! substituted without breaking the release signature, and this module makes
//! that structural rather than optional: [`TrustedRelease`] is the only way in,
//! it cannot be constructed without a signature that verified over the exact
//! bytes served, and [`ContentService::plan`] takes one. "Did anybody check the
//! signature" becomes a thing the compiler tracks instead of a thing somebody
//! has to remember.
//!
//! No verifier is implemented here, on purpose. Key custody is unresolved, and
//! a built-in verifier that returned true would be worse than no verifier at
//! all: every call site would look correct and nothing would be checked. The
//! caller supplies a [`SignatureVerifier`], and until one exists no release can
//! be opened, which is the honest state of the project.
//!
//! # The four things that must never happen
//!
//! The shipped 2.1 updater deletes before it installs, and that single ordering
//! choice is the root of most of its damage. Nothing here removes or replaces
//! anything until a verified replacement exists on disk. Concretely:
//!
//! 1. **Nothing is promoted before its hash matches the manifest.** The only
//!    path into `staging/` is a rename that happens after the bytes hashed
//!    correctly, and [`ContentService::promote`] hashes the whole staged set
//!    again before it moves a single live file. The second hash is not
//!    paranoia: the first one proves what arrived over the network, the second
//!    proves what is on disk right now, and between the two the disk may have
//!    lied, the file may have been edited, or the process may have been killed.
//! 2. **A partial download can never be mistaken for a complete one.**
//!    Incomplete bytes live in `downloads/` under a `.part` name and become a
//!    staged file only by way of a hash check. Length is never treated as
//!    evidence of completeness, and neither is the journal.
//! 3. **A failed update leaves the previous version working.** Every live file
//!    is renamed aside into `versions/<component>/prior/` before its
//!    replacement is renamed in, so a failure at any point is reversed by
//!    running the recorded moves backwards. The reversal also puts the new
//!    file back into `staging/`, so the retry does not re-download.
//! 4. **A customizable component is never clobbered.** A user who rebuilt the
//!    offline image with their own soundtrack has a file that will never match
//!    a shipped hash. Those files are excluded from the fetch set and skipped
//!    at promotion, and the receipt says so, so the launcher can tell the user
//!    their music was kept rather than quietly re-downloading 1.5 GB over it.
//!    See [`crate::library`] for how a rebuild is told apart from corruption.
//!
//! # The journal can only save work, never authorize it
//!
//! [`Journal`] records which operation was in flight so an interrupted swap can
//! be resolved without re-fetching anything. It is never consulted to decide
//! that something is verified, and an unreadable journal is discarded rather
//! than repaired. The worst a lost journal can cost is a repeated download; if
//! it were trusted, it could cost a promotion of bytes nobody checked.
//!
//! An interrupted swap always resolves to the *previous* version, even when the
//! new files look complete. A swap that cannot be proven to have finished is
//! not a swap to trust, and the staged bytes are still there to try again with.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::{hash_file, FileVerdict, RebuildState, Verifier};
use crate::manifest::{Component, ManifestError, ManifestFile, Release, Sha256Hex};
use crate::platform::{DataDir, Error, Platform, Result};

/// Schema of the installed-version pointer. Ours to keep readable.
pub const POINTER_SCHEMA: &str = "nsr.pointer.v1";
/// Schema of the in-flight operation journal.
pub const JOURNAL_SCHEMA: &str = "nsr.content-journal.v1";
/// Schema of the record naming what is in the prior directory.
pub const PRIOR_SCHEMA: &str = "nsr.prior.v1";

const POINTER_FILE: &str = "current.json";
const PRIOR_DIR: &str = "prior";
/// The manifest of the version that is live, kept beside the pointer so the
/// NEXT update can tell which files that version had and this one drops.
const MANIFEST_FILE: &str = "manifest.json";
const PRIOR_FILE: &str = "prior.json";
const JOURNAL_FILE: &str = "journal.json";

/// A refusal reported as an error because there is no safe way to continue:
/// tampering, a malformed manifest, or a path that tries to escape.
fn refuse(message: impl Into<String>) -> Error {
    Error::PlainIo(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.into(),
    ))
}

fn from_manifest(e: ManifestError) -> Error {
    refuse(e.to_string())
}

/// Join a manifest-relative path onto a root, refusing anything that escapes.
///
/// [`Component::traversal_offenders`] already rejects these at plan time. This
/// exists so that a plan built by hand, or a manifest that changed underneath a
/// long-running operation, still cannot make the launcher write outside the
/// component's install root. Every write in this module goes through here.
/// Whether a manifest's install root is the one the caller expected.
///
/// Exact equality for every data directory but one. `textures` installs to
/// `library/textures/<game id>`, one directory below [`DataDir::LibraryTextures`],
/// because two game ids sharing a root would overwrite each other mostly
/// invisibly (2,252 of FR4N31's files are byte-identical to GN7E69's). The
/// publisher enforces exactly that shape in `inventory::check_install_root`;
/// this is the same rule from the client's side, so the two halves agree.
/// Before this, the client demanded equality and no textures update could
/// ever be planned.
fn install_root_matches(expected: DataDir, install_root: &str) -> bool {
    let base = expected.relative();
    if install_root == base {
        return expected != DataDir::LibraryTextures;
    }
    if expected != DataDir::LibraryTextures {
        return false;
    }
    match install_root.strip_prefix(base).and_then(|rest| rest.strip_prefix('/')) {
        Some(game_id) => !game_id.is_empty() && !game_id.contains(['/', '\\']),
        None => false,
    }
}

/// The shape a refusal names as expected, so the message says
/// `library/textures/<game id>` rather than a bare `library/textures` that the
/// client would also refuse.
fn expected_shape(expected: DataDir) -> String {
    match expected {
        DataDir::LibraryTextures => format!("{}/<game id>", expected.relative()),
        other => other.relative().to_string(),
    }
}

fn resolve_under(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.starts_with('/') || relative.starts_with('\\') || relative.contains(':') {
        return Err(refuse(format!(
            "refusing {relative:?}: a manifest path must be relative to the \
             component install root, and this one is not. Report the manifest \
             to whoever published it"
        )));
    }
    let mut out = root.to_path_buf();
    for segment in relative.split(['/', '\\']) {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(refuse(format!(
                "refusing {relative:?}: it climbs out of the component install \
                 root, which is how an update writes over saves. Report the \
                 manifest to whoever published it"
            )));
        }
        out.push(segment);
    }
    if out == root {
        return Err(refuse(format!(
            "refusing {relative:?}: it names no file inside the install root"
        )));
    }
    Ok(out)
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

/// Length of a file, with absence reported as zero.
///
/// Used for resume, where "no part file yet" and "a part file of zero bytes"
/// are the same starting position.
fn file_len(path: &Path) -> Result<u64> {
    match std::fs::metadata(path) {
        Ok(m) => Ok(m.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

// ---------------------------------------------------------------------------
// Trust
// ---------------------------------------------------------------------------

/// Checks a detached signature over the exact bytes that were served.
///
/// Re-serializing the JSON before checking is how signature verification
/// silently starts passing on modified content, so the trait is handed the
/// bytes and nothing else.
pub trait SignatureVerifier: std::fmt::Debug {
    /// True when `signature` is a valid signature over `signed_bytes` by a key
    /// this verifier trusts. An `Err` means the check could not be performed,
    /// which is not the same as a failed check and must not be treated as one.
    fn verify(&self, signed_bytes: &[u8], signature: &[u8]) -> Result<bool>;
}

/// A release manifest whose signature verified.
///
/// There is no constructor that skips the check and no `Deserialize`, so a
/// `TrustedRelease` cannot be conjured from a file on disk.
#[derive(Debug)]
pub struct TrustedRelease {
    release: Release,
}

impl TrustedRelease {
    /// Verify a signature over `bytes`, then parse.
    ///
    /// In that order. Parsing first would mean the launcher had already acted on
    /// attacker-controlled structure before checking whether the bytes were
    /// genuine.
    pub fn open(
        verifier: &dyn SignatureVerifier,
        bytes: &[u8],
        signature: &[u8],
    ) -> Result<TrustedRelease> {
        if !verifier.verify(bytes, signature)? {
            return Err(refuse(
                "the release manifest signature did not verify. Nothing was \
                 downloaded and nothing was changed. The file was altered in \
                 transit or truncated: retry, and if it happens again do not \
                 install and report it",
            ));
        }
        let release = Release::parse(bytes).map_err(from_manifest)?;
        Ok(TrustedRelease { release })
    }

    pub fn release(&self) -> &Release {
        &self.release
    }

    /// Refuse a release that is not strictly newer than the one installed.
    ///
    /// Replay protection at the release level. The caller owns the stored
    /// release sequence, because a release covers several components and no
    /// single component's pointer file can speak for it.
    pub fn check_sequence(&self, installed: u64) -> Result<()> {
        self.release
            .check_sequence(installed)
            .map_err(from_manifest)
    }

    /// Validate component manifest bytes against this release's pin.
    ///
    /// Failure here is tampering or corruption rather than a state a user can be
    /// in, so it is an error and not a refusal.
    pub fn accept_component(&self, requested: &str, bytes: &[u8]) -> Result<Component> {
        self.release
            .accept_component(requested, bytes)
            .map_err(from_manifest)
    }
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// Where content operations happen, all of it under one data root.
///
/// One root matters for correctness and not tidiness: promotion is a rename, and
/// a rename is only atomic within a filesystem. Downloads, staging, the live
/// trees and the prior copies all sit under the data root so that every move in
/// this module is a same-volume rename.
#[derive(Debug, Clone)]
pub struct ContentLayout {
    data_root: PathBuf,
}

impl ContentLayout {
    pub fn new(data_root: &Path) -> Self {
        Self {
            data_root: data_root.to_path_buf(),
        }
    }

    pub fn from_platform(platform: &Platform) -> Result<Self> {
        Ok(Self::new(&platform.paths.data_root()?))
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    /// Unverified bytes, resumable, `.part` named.
    pub fn downloads_dir(&self, component: &str, version: &str) -> PathBuf {
        self.data_root
            .join(DataDir::Downloads.relative())
            .join(component)
            .join(version)
    }

    /// Fully downloaded, hash-verified, inert.
    pub fn staging_dir(&self, component: &str, version: &str) -> PathBuf {
        self.data_root
            .join(DataDir::Staging.relative())
            .join(component)
            .join(version)
    }

    fn versions_dir(&self, component: &str) -> PathBuf {
        self.data_root
            .join(DataDir::Versions.relative())
            .join(component)
    }

    /// The record of which version is live.
    pub fn pointer_path(&self, component: &str) -> PathBuf {
        self.versions_dir(component).join(POINTER_FILE)
    }

    /// The single retained prior version's files.
    pub fn prior_dir(&self, component: &str) -> PathBuf {
        self.versions_dir(component).join(PRIOR_DIR)
    }

    /// The manifest of the live version, written at promotion.
    ///
    /// What it is for: the next update compares it with the incoming manifest
    /// to find files the new version no longer ships, and retires them. What
    /// it is NOT for: deciding that anything is verified. It is a local copy
    /// with no signature, and the worst a tampered one can do is move a file
    /// into `prior/`, which rollback puts back.
    pub fn manifest_path(&self, component: &str) -> PathBuf {
        self.versions_dir(component).join(MANIFEST_FILE)
    }

    fn prior_record_path(&self, component: &str) -> PathBuf {
        self.versions_dir(component).join(PRIOR_FILE)
    }

    pub fn journal_path(&self) -> PathBuf {
        self.data_root.join(JOURNAL_FILE)
    }

    /// Where a component's files actually live.
    ///
    /// Resolved through the same path guard as every write in this module,
    /// because `install_root` comes out of a manifest, and a manifest that says
    /// `../../saves` must not be able to point the live tree at somebody's
    /// memory cards.
    pub fn live_root(&self, component: &Component) -> Result<PathBuf> {
        resolve_under(&self.data_root, &component.install_root)
    }
}

// ---------------------------------------------------------------------------
// What is installed
// ---------------------------------------------------------------------------

/// A version and the sequence number that goes with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledVersion {
    pub version: String,
    /// Carried so replay protection survives a restart. Without it the launcher
    /// would have to re-derive "is this newer" from a version string, and
    /// version strings are not ordered by anything a client can trust.
    pub sequence: u64,
}

/// The record of what is live for one component.
///
/// Absent means nothing is installed. There is deliberately no default: a
/// fabricated `0.0.0` would satisfy the type system and then be shown to a user
/// as though the launcher knew something it does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pointer {
    pub schema: String,
    pub component: String,
    pub current: InstalledVersion,
    /// What rollback goes back to. `None` after a first install, and after a
    /// rollback, because exactly one prior version is retained and naming a
    /// version whose files are gone would be a lie the user acts on.
    pub previous: Option<InstalledVersion>,
}

impl Pointer {
    fn new(component: &str, current: InstalledVersion, previous: Option<InstalledVersion>) -> Self {
        Self {
            schema: POINTER_SCHEMA.to_string(),
            component: component.to_string(),
            current,
            previous,
        }
    }

    /// Read the pointer, or `None` when nothing is installed.
    ///
    /// An unreadable pointer is an error, which is the opposite of how
    /// [`Journal`] treats damage, and the difference is deliberate. The journal
    /// can only save work. This file is the only statement of what is installed
    /// and which sequence number it carried: discarding it silently would make
    /// the launcher re-download a component it already has, and would drop the
    /// replay protection that stops a signed-but-old release being reinstalled.
    pub fn load(path: &Path) -> Result<Option<Pointer>> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(Error::Io {
                    path: path.to_path_buf(),
                    source,
                })
            }
        };
        let pointer: Pointer = serde_json::from_slice(&bytes).map_err(|e| {
            refuse(format!(
                "{} does not read as an installed-version record: {e}. Delete \
                 the file to force a full verify, which re-checks the files on \
                 disk instead of guessing what is installed",
                path.display()
            ))
        })?;
        if pointer.schema != POINTER_SCHEMA {
            return Err(refuse(format!(
                "{} has schema {:?}, expected {POINTER_SCHEMA}. This launcher \
                 is older than the install it is looking at. Update the \
                 launcher rather than deleting the file",
                path.display(),
                pointer.schema
            )));
        }
        Ok(Some(pointer))
    }

    /// Write the pointer by write-temp-then-rename.
    ///
    /// The rename is the only thing in an update that has to be atomic, so it is
    /// the last thing that happens. A reader sees either wholly the old record
    /// or wholly the new one, never a half-written file that would be unreadable
    /// and, per [`Pointer::load`], would then block startup.
    pub fn save(&self, platform: &Platform, path: &Path) -> Result<()> {
        ensure_parent(path)?;
        let temp = path.with_file_name(format!("{POINTER_FILE}.new"));
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| refuse(format!("could not encode the version record: {e}")))?;
        {
            let mut file = std::fs::File::create(&temp).map_err(|source| Error::Io {
                path: temp.clone(),
                source,
            })?;
            file.write_all(&json).map_err(|source| Error::Io {
                path: temp.clone(),
                source,
            })?;
            file.sync_all().map_err(|source| Error::Io {
                path: temp.clone(),
                source,
            })?;
        }
        platform.files.atomic_replace(&temp, path)
    }
}

/// Names the version whose files are sitting in the prior directory.
///
/// The directory itself is the authority on *which* files are retained, because
/// a crash can interrupt the move-aside loop and a list written in advance would
/// name files that were never moved. This record carries only what the directory
/// cannot: which version those bytes are, and which tree they came out of.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorRecord {
    pub schema: String,
    pub component: String,
    /// Absent when the outgoing files had no pointer, so their version is
    /// genuinely unknown. Rollback refuses rather than inventing one.
    pub version: Option<InstalledVersion>,
    pub live_root: PathBuf,
}

impl PriorRecord {
    fn load(path: &Path) -> Option<PriorRecord> {
        let bytes = std::fs::read(path).ok()?;
        let record: PriorRecord = serde_json::from_slice(&bytes).ok()?;
        if record.schema != PRIOR_SCHEMA {
            return None;
        }
        Some(record)
    }

    fn save(&self, path: &Path) -> Result<()> {
        ensure_parent(path)?;
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| refuse(format!("could not encode the prior record: {e}")))?;
        std::fs::write(path, json).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

// ---------------------------------------------------------------------------
// Journal
// ---------------------------------------------------------------------------

/// How far an operation got.
///
/// There is no `Active`: once the pointer file is written the operation is not
/// in flight, and the pointer is the record of what is live. A journal row that
/// outlives its operation is a second answer to a question that already has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Planned,
    Downloading,
    Staged,
    /// Live files are being moved. The only state that needs recovery.
    Swapping,
    Failed,
}

/// One in-flight content operation.
///
/// The three roots are recorded so recovery works without re-fetching a
/// manifest. A launcher that has to reach the network before it can put a
/// half-swapped install back together is a launcher that cannot recover on a
/// plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub component: String,
    pub version: String,
    pub state: OperationState,
    pub live_root: PathBuf,
    pub staging: PathBuf,
    pub prior: PathBuf,
}

/// The in-flight operation log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journal {
    pub schema: String,
    pub operations: Vec<Operation>,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            schema: JOURNAL_SCHEMA.to_string(),
            operations: Vec::new(),
        }
    }
}

impl Journal {
    /// Read the journal, discarding anything that does not read cleanly.
    ///
    /// Infallible by design. A journal can only ever save work: the worst a
    /// discarded one costs is a resume that starts over, or an interrupted swap
    /// that the next verify catches instead. If it were trusted, a bit-rotted or
    /// hand-edited row could steer a promotion, so damage is thrown away rather
    /// than parsed as far as it will go.
    pub fn load(path: &Path) -> Journal {
        let Ok(bytes) = std::fs::read(path) else {
            return Journal::default();
        };
        match serde_json::from_slice::<Journal>(&bytes) {
            Ok(j) if j.schema == JOURNAL_SCHEMA => j,
            _ => Journal::default(),
        }
    }

    pub fn save(&self, platform: &Platform, path: &Path) -> Result<()> {
        ensure_parent(path)?;
        let temp = path.with_file_name(format!("{JOURNAL_FILE}.new"));
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| refuse(format!("could not encode the journal: {e}")))?;
        std::fs::write(&temp, json).map_err(|source| Error::Io {
            path: temp.clone(),
            source,
        })?;
        platform.files.atomic_replace(&temp, path)
    }

    /// One row per component. A second operation on the same component replaces
    /// the first, because two in-flight updates of one component is not a state
    /// the design allows.
    pub fn upsert(&mut self, op: Operation) {
        self.operations.retain(|o| o.component != op.component);
        self.operations.push(op);
    }

    pub fn find(&self, component: &str) -> Option<&Operation> {
        self.operations.iter().find(|o| o.component == component)
    }

    pub fn remove(&mut self, component: &str) {
        self.operations.retain(|o| o.component != component);
    }

    /// Operations that were part way through moving live files.
    pub fn interrupted(&self) -> Vec<&Operation> {
        self.operations
            .iter()
            .filter(|o| o.state == OperationState::Swapping)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// Why a file is in the fetch set.
///
/// Carried so the user can be told something true. "Repairing 3 files" is worth
/// less than naming them and saying which were absent and which were damaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchReason {
    /// Not on disk.
    Absent,
    /// On disk, and neither the manifest hash nor a recorded rebuild explains
    /// what is there.
    Damaged,
}

/// One file the plan will fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFetch {
    pub path: String,
    pub bytes: u64,
    pub sha256: Sha256Hex,
    pub reason: FetchReason,
}

/// What an update will do, before it does any of it.
///
/// The fetch set falls out of verifying the local tree against the *new*
/// manifest, which is why an update is differential for free: a file that
/// already hashes to what the new version wants is already the new version of
/// that file, whatever its age.
#[derive(Debug, Clone)]
pub struct UpdatePlan {
    /// The accepted component manifest. Held here so no later call can be handed
    /// a different manifest than the one the plan was built from.
    pub manifest: Component,
    /// What is installed now, if anything.
    pub from: Option<InstalledVersion>,
    pub fetches: Vec<PlannedFetch>,
    /// Already correct on disk. Not fetched, not promoted, not touched.
    pub already_present: Vec<String>,
    /// Customizable files the user has rebuilt. Never fetched and never promoted
    /// over. See [`crate::library::FileVerdict::UserModified`].
    pub preserved: Vec<String>,
    /// Bytes currently occupied by the live copies of the files being replaced,
    /// measured on disk. That is what the prior directory will hold after the
    /// swap, so it is the standing cost of being able to roll back.
    pub retained_bytes: u64,
}

impl UpdatePlan {
    pub fn component(&self) -> &str {
        &self.manifest.component
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    pub fn sequence(&self) -> u64 {
        self.manifest.sequence
    }

    /// What has to come over the network.
    pub fn download_bytes(&self) -> u64 {
        self.fetches.iter().map(|f| f.bytes).sum()
    }

    /// Extra disk needed to complete this update, on top of the live copy.
    ///
    /// Downloaded bytes are renamed into staging rather than copied, so they are
    /// counted once. The retained prior copy is counted because it is real and
    /// stays until the next update replaces it.
    ///
    /// This is a measurement, not a policy. The free-space precheck itself needs
    /// a way to ask how much room a volume has, no platform seam offers one yet,
    /// so the caller does the comparison.
    pub fn peak_extra_bytes(&self) -> u64 {
        self.download_bytes().saturating_add(self.retained_bytes)
    }

    /// Whether this is a repair of the installed version rather than a move to a
    /// new one.
    pub fn is_repair(&self) -> bool {
        self.from
            .as_ref()
            .is_some_and(|f| f.version == self.manifest.version)
    }

    fn manifest_file(&self, path: &str) -> Option<&ManifestFile> {
        self.manifest.files.iter().find(|f| f.path == path)
    }

    fn preserved_set(&self) -> BTreeSet<&str> {
        self.preserved.iter().map(|p| p.as_str()).collect()
    }
}

/// Why an update was not planned. None of these are faults in the launcher, and
/// each one has a different thing to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanRefusal {
    /// The offered component is not newer than what is installed.
    Stale {
        component: String,
        installed: InstalledVersion,
        offered_version: String,
        offered_sequence: u64,
    },
    /// The manifest carries paths that leave the install root.
    UnsafePaths {
        component: String,
        paths: Vec<String>,
    },
    /// The manifest wants to install somewhere other than where the launcher
    /// keeps this component.
    WrongInstallRoot {
        component: String,
        found: String,
        expected: String,
    },
}

impl PlanRefusal {
    /// A sentence the player, or whoever is reading the log, can act on.
    pub fn message(&self) -> String {
        match self {
            PlanRefusal::Stale {
                component,
                installed,
                offered_version,
                offered_sequence,
            } => format!(
                "Not installing {component} {offered_version} (sequence \
                 {offered_sequence}): version {} sequence {} is already \
                 installed. An older release can be genuinely signed and still \
                 be one whose bugs were fixed, so it is refused. Check for a \
                 newer update.",
                installed.version, installed.sequence
            ),
            PlanRefusal::UnsafePaths { component, paths } => format!(
                "Not installing {component}: {} of its file paths point outside \
                 the folder this component installs into, for example {:?}. \
                 Nothing was downloaded. Report this to whoever published the \
                 update.",
                paths.len(),
                paths.first().map(String::as_str).unwrap_or("")
            ),
            PlanRefusal::WrongInstallRoot {
                component,
                found,
                expected,
            } => format!(
                "Not installing {component}: its manifest installs into \
                 {found:?} but the launcher keeps this component in \
                 {expected:?}. Nothing was downloaded. Report this to whoever \
                 published the update."
            ),
        }
    }
}

/// The result of planning.
#[derive(Debug)]
pub enum PlanOutcome {
    /// There is work to do.
    Update(Box<UpdatePlan>),
    /// The installed version is the offered one and every file checks out.
    UpToDate { version: String },
    /// Nothing was planned, and this is why.
    Refused(PlanRefusal),
}

// ---------------------------------------------------------------------------
// Fetching, expressed as a seam
// ---------------------------------------------------------------------------

/// One request for the bytes of one manifest file.
#[derive(Debug)]
pub struct BlobRequest<'a> {
    pub component: &'a str,
    pub version: &'a str,
    pub file: &'a ManifestFile,
    /// Bytes already on disk for this file. An implementation must serve from
    /// this offset, which over HTTP is a `Range` request.
    ///
    /// An implementation that cannot do that must return an error rather than
    /// starting from zero. Silently serving from the beginning appends a second
    /// copy onto a partial file: the hash check does catch it, but it costs the
    /// user the whole download twice and it reports as corruption instead of as
    /// the missing feature it is.
    pub offset: u64,
}

/// Where bytes come from.
///
/// A trait rather than an HTTP client because this module has no network stack
/// and must not grow one. Every rule worth testing here is about what happens to
/// bytes after they arrive, and a test that needs a server is a test nobody runs.
pub trait BlobSource: std::fmt::Debug {
    /// Open a stream of the file's bytes starting at `req.offset`.
    fn open(&self, req: &BlobRequest<'_>) -> Result<Box<dyn Read>>;
}

/// What happened to one file's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchState {
    /// Complete, hashed, and moved into staging. The only state that can be
    /// promoted.
    Staged,
    /// Fewer bytes than the manifest says. The `.part` file is kept and the next
    /// attempt resumes from `have`.
    Incomplete { have: u64, want: u64 },
    /// A complete-length file whose hash is wrong, or a file longer than the
    /// manifest allows. The `.part` file is deleted, because resuming from
    /// poisoned bytes can only ever produce the same wrong hash again.
    ///
    /// `resumed_from` separates two very different causes: a nonzero value with
    /// a repeatable failure means the source is ignoring the offset, and
    /// retrying forever will not fix that.
    Corrupt {
        resumed_from: u64,
        expected: Sha256Hex,
        actual: Sha256Hex,
    },
}

/// The result of one download pass.
#[derive(Debug, Clone, Default)]
pub struct FetchReport {
    pub results: BTreeMap<String, FetchState>,
}

impl FetchReport {
    /// Whether every planned file is staged. Never inferred from counts or
    /// sizes: this only reads states that a hash check produced.
    pub fn is_complete(&self) -> bool {
        self.results.values().all(|s| *s == FetchState::Staged)
    }

    pub fn incomplete(&self) -> Vec<&str> {
        self.results
            .iter()
            .filter(|(_, s)| matches!(s, FetchState::Incomplete { .. }))
            .map(|(p, _)| p.as_str())
            .collect()
    }

    pub fn corrupt(&self) -> Vec<&str> {
        self.results
            .iter()
            .filter(|(_, s)| matches!(s, FetchState::Corrupt { .. }))
            .map(|(p, _)| p.as_str())
            .collect()
    }

    /// Bytes still to fetch before this set can be promoted.
    pub fn remaining_bytes(&self, plan: &UpdatePlan) -> u64 {
        self.results
            .iter()
            .map(|(path, state)| match state {
                FetchState::Staged => 0,
                FetchState::Incomplete { have, want } => want.saturating_sub(*have),
                // A corrupt part was deleted, so the whole file is owed again.
                FetchState::Corrupt { .. } => plan
                    .manifest_file(path)
                    .map(|f| f.bytes)
                    .unwrap_or_default(),
            })
            .sum()
    }
}

/// Whether the staged set may be promoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagedVerdict {
    /// Every planned file is present and hashes to the manifest.
    Complete,
    NotReady {
        missing: Vec<String>,
        wrong: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// Promotion
// ---------------------------------------------------------------------------

/// Why a swap did not start. Nothing was moved in any of these cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotionRefusal {
    /// The staged set is not all there, or does not all hash correctly.
    StagingIncomplete {
        component: String,
        missing: Vec<String>,
        wrong: Vec<String>,
    },
    /// A live file is held open by another process, which on Windows means it
    /// cannot be replaced. This is the quiescence rule: refuse before starting
    /// rather than fail half way through.
    InUse { path: PathBuf },
    /// Staging and the live tree are on different volumes, so the swap would be
    /// a copy and a delete rather than a rename, and could leave the install in
    /// neither state.
    CrossVolume { staging: PathBuf, live: PathBuf },
}

impl PromotionRefusal {
    pub fn message(&self) -> String {
        match self {
            PromotionRefusal::StagingIncomplete {
                component,
                missing,
                wrong,
            } => format!(
                "{component} is not ready to install: {} downloaded files are \
                 missing and {} do not match the manifest. Nothing was changed. \
                 Resume the download and it will pick up where it stopped.",
                missing.len(),
                wrong.len()
            ),
            PromotionRefusal::InUse { path } => format!(
                "{} is open in another program, so it cannot be replaced. \
                 Nothing was changed. Close the game and install again.",
                path.display()
            ),
            PromotionRefusal::CrossVolume { staging, live } => format!(
                "{} and {} are on different drives, so this install cannot be \
                 completed safely. Nothing was changed. Keep the launcher data \
                 folder on one drive.",
                staging.display(),
                live.display()
            ),
        }
    }
}

/// What a completed swap did.
#[derive(Debug, Clone)]
pub struct PromotionReceipt {
    pub component: String,
    pub version: String,
    pub previous: Option<InstalledVersion>,
    pub promoted: Vec<String>,
    /// Customizable files that were deliberately left as the user had them.
    /// Reported so the launcher can say so instead of implying a clean sweep.
    pub preserved: Vec<String>,
    /// Where the replaced files went, when any were replaced.
    pub prior_dir: Option<PathBuf>,
    /// Files the previous version shipped and this one does not. Moved into
    /// the prior directory, never deleted, so rollback brings them back.
    /// Reported so the launcher can say "3 files removed" rather than have
    /// a player wonder where a texture went.
    pub retired: Vec<String>,
}

/// The result of a swap.
#[derive(Debug)]
pub enum Promoted {
    Applied(PromotionReceipt),
    /// Nothing was moved.
    Refused(PromotionRefusal),
    /// Something failed part way and every move was reversed. The previous
    /// version is live again and the staged bytes are back in staging, so a
    /// retry costs nothing over the network.
    RolledBack {
        reason: String,
        restored: Vec<PathBuf>,
        returned_to_staging: Vec<PathBuf>,
    },
}

/// Why a rollback could not be performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackRefusal {
    NotInstalled {
        component: String,
    },
    /// A first install has nothing behind it.
    NoPrevious {
        component: String,
        current_version: String,
    },
    /// The pointer names a previous version but its files are not retained
    /// locally. True by design for the disc images, where keeping a prior copy
    /// costs about 1.4 GB permanently.
    NoLocalPrior {
        component: String,
        previous_version: String,
    },
    /// The prior files are there but nothing reliably records which version they
    /// are, so restoring them would mean writing a version number nobody knows.
    UnknownPriorVersion {
        component: String,
    },
    InUse {
        path: PathBuf,
    },
}

impl RollbackRefusal {
    pub fn message(&self) -> String {
        match self {
            RollbackRefusal::NotInstalled { component } => format!(
                "{component} is not installed, so there is nothing to roll \
                 back. Install it first."
            ),
            RollbackRefusal::NoPrevious {
                component,
                current_version,
            } => format!(
                "{component} {current_version} is the first version installed, \
                 so there is no earlier one to go back to. Run a repair instead \
                 if something is wrong."
            ),
            RollbackRefusal::NoLocalPrior {
                component,
                previous_version,
            } => format!(
                "Going back to {component} {previous_version} means downloading \
                 it again, because the launcher does not keep a local copy of \
                 the previous game content. It is a download, not an undo. \
                 Start a repair to fetch it."
            ),
            RollbackRefusal::UnknownPriorVersion { component } => format!(
                "The retained copy of {component} is not labelled with a \
                 version that matches what is installed, so it cannot be \
                 restored as one. Run a repair, which checks the files on disk \
                 rather than trusting a label."
            ),
            RollbackRefusal::InUse { path } => format!(
                "{} is open in another program, so it cannot be replaced. \
                 Nothing was changed. Close the game and try again.",
                path.display()
            ),
        }
    }
}

/// The result of a rollback.
#[derive(Debug)]
pub enum RollbackOutcome {
    Restored {
        to: InstalledVersion,
        restored: Vec<PathBuf>,
        /// The version that was live is moved into staging rather than deleted,
        /// so going forward again does not mean downloading it twice.
        returned_to_staging: Vec<PathBuf>,
    },
    Refused(RollbackRefusal),
}

/// One interrupted swap that was put back.
#[derive(Debug, Clone)]
pub struct RecoveredSwap {
    pub component: String,
    /// The version that was being installed when the process died.
    pub interrupted_version: String,
    pub restored: Vec<PathBuf>,
    pub returned_to_staging: Vec<PathBuf>,
    /// True when the pointer already named the new version and had to be put
    /// back. If this is true the swap had reached its last step, and the files
    /// were still reversed, because a swap nobody finished is not one to trust.
    pub pointer_rolled_back: bool,
    /// Set when the files were restored but the pointer could not be, because it
    /// named the new version and recorded no previous one. The component needs a
    /// verify before it is played.
    pub unresolved: Option<String>,
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// One completed move, kept so it can be undone.
#[derive(Debug)]
struct DoneMove {
    live: PathBuf,
    /// Absent when the file is new and there was nothing to move aside.
    prior: Option<PathBuf>,
    staged: PathBuf,
}

/// Install and update, over one data root.
#[derive(Debug)]
pub struct ContentService<'a> {
    platform: &'a Platform,
    layout: ContentLayout,
}

impl<'a> ContentService<'a> {
    pub fn new(platform: &'a Platform, data_root: &Path) -> Self {
        Self {
            platform,
            layout: ContentLayout::new(data_root),
        }
    }

    pub fn for_platform(platform: &'a Platform) -> Result<Self> {
        Ok(Self {
            layout: ContentLayout::from_platform(platform)?,
            platform,
        })
    }

    pub fn layout(&self) -> &ContentLayout {
        &self.layout
    }

    /// What is installed for one component, or `None`.
    pub fn installed(&self, component: &str) -> Result<Option<Pointer>> {
        Pointer::load(&self.layout.pointer_path(component))
    }

    /// Work out what an update would do, without doing any of it.
    ///
    /// `expected_root` is the directory the launcher keeps this component in, and
    /// it is checked against the manifest rather than taken from it. That check
    /// is what stops a manifest for `runtime` writing into `saves`.
    pub fn plan(
        &self,
        release: &TrustedRelease,
        component_name: &str,
        manifest_bytes: &[u8],
        expected_root: DataDir,
        rebuilds: &RebuildState,
    ) -> Result<PlanOutcome> {
        // The hash pin from the signed release, and the component name, before
        // anything else looks at the contents.
        let component = release.accept_component(component_name, manifest_bytes)?;

        if !install_root_matches(expected_root, &component.install_root) {
            return Ok(PlanOutcome::Refused(PlanRefusal::WrongInstallRoot {
                component: component_name.to_string(),
                found: component.install_root.clone(),
                expected: expected_shape(expected_root),
            }));
        }

        let offenders = component.traversal_offenders();
        if !offenders.is_empty() {
            return Ok(PlanOutcome::Refused(PlanRefusal::UnsafePaths {
                component: component_name.to_string(),
                paths: offenders.into_iter().map(str::to_string).collect(),
            }));
        }

        let installed = self.installed(component_name)?;
        if let Some(pointer) = &installed {
            // Newer is an update. The same sequence with the same version is a
            // repair, which is the point of being able to re-fetch damaged files
            // without a version bump. Anything else is a replay of something
            // older, or a manifest reusing a sequence for different contents,
            // and neither is safe to install.
            let newer = component.sequence > pointer.current.sequence;
            let same_build = component.sequence == pointer.current.sequence
                && component.version == pointer.current.version;
            if !newer && !same_build {
                return Ok(PlanOutcome::Refused(PlanRefusal::Stale {
                    component: component_name.to_string(),
                    installed: pointer.current.clone(),
                    offered_version: component.version.clone(),
                    offered_sequence: component.sequence,
                }));
            }
        }

        let live_root = self.layout.live_root(&component)?;
        let report = Verifier::new(&live_root, rebuilds).verify(&component)?;

        let mut fetches = Vec::new();
        let mut already_present = Vec::new();
        let mut preserved = Vec::new();
        let mut retained_bytes = 0u64;

        for file in &component.files {
            match report.verdicts.get(&file.path) {
                Some(FileVerdict::Intact) => already_present.push(file.path.clone()),
                // A user's rebuilt file is not damage and is not an obstacle. It
                // is excluded here, which makes the promotion skip structural
                // rather than a check somebody could forget to write.
                Some(FileVerdict::UserModified) => preserved.push(file.path.clone()),
                Some(FileVerdict::Missing) => fetches.push(PlannedFetch {
                    path: file.path.clone(),
                    bytes: file.bytes,
                    sha256: file.sha256.clone(),
                    reason: FetchReason::Absent,
                }),
                Some(FileVerdict::Damaged { .. }) => {
                    retained_bytes = retained_bytes
                        .saturating_add(file_len(&resolve_under(&live_root, &file.path)?)?);
                    fetches.push(PlannedFetch {
                        path: file.path.clone(),
                        bytes: file.bytes,
                        sha256: file.sha256.clone(),
                        reason: FetchReason::Damaged,
                    });
                }
                // The verifier reports on the manifest's own file list, so this
                // is unreachable. Treating it as absent is the safe reading: it
                // costs a download, and the alternative costs a missing file.
                None => fetches.push(PlannedFetch {
                    path: file.path.clone(),
                    bytes: file.bytes,
                    sha256: file.sha256.clone(),
                    reason: FetchReason::Absent,
                }),
            }
        }

        let at_this_version = installed
            .as_ref()
            .is_some_and(|p| p.current.version == component.version);
        if fetches.is_empty() && at_this_version {
            return Ok(PlanOutcome::UpToDate {
                version: component.version.clone(),
            });
        }

        Ok(PlanOutcome::Update(Box::new(UpdatePlan {
            manifest: component,
            from: installed.map(|p| p.current),
            fetches,
            already_present,
            preserved,
            retained_bytes,
        })))
    }

    /// Fetch what the plan asks for, hash it, and stage what matches.
    ///
    /// Safe to call repeatedly. Each call resumes from whatever is on disk, and a
    /// file that is already staged is not fetched again.
    pub fn download(&self, plan: &UpdatePlan, source: &dyn BlobSource) -> Result<FetchReport> {
        let downloads = self.layout.downloads_dir(plan.component(), plan.version());
        let staging = self.layout.staging_dir(plan.component(), plan.version());
        std::fs::create_dir_all(&downloads).map_err(|source| Error::Io {
            path: downloads.clone(),
            source,
        })?;
        std::fs::create_dir_all(&staging).map_err(|source| Error::Io {
            path: staging.clone(),
            source,
        })?;

        self.record(Operation {
            component: plan.component().to_string(),
            version: plan.version().to_string(),
            state: OperationState::Downloading,
            live_root: self.layout.live_root(&plan.manifest)?,
            staging: staging.clone(),
            prior: self.layout.prior_dir(plan.component()),
        })?;

        let mut report = FetchReport::default();
        let preserved = plan.preserved_set();

        for fetch in &plan.fetches {
            // Belt and braces. A plan that both preserved and fetched a path
            // would be a contradiction, and the resolution that cannot destroy
            // anything is to leave the user's file alone.
            if preserved.contains(fetch.path.as_str()) {
                continue;
            }
            let file = plan.manifest_file(&fetch.path).ok_or_else(|| {
                refuse(format!(
                    "the plan asks for {:?}, which the component manifest for \
                     {} {} does not list. Rebuild the plan from the manifest",
                    fetch.path,
                    plan.component(),
                    plan.version()
                ))
            })?;

            let staged = resolve_under(&staging, &fetch.path)?;
            if staged.exists() {
                report
                    .results
                    .insert(fetch.path.clone(), FetchState::Staged);
                continue;
            }

            report
                .results
                .insert(fetch.path.clone(), self.fetch_one(plan, file, source)?);
        }

        if report.is_complete() {
            self.set_state(plan.component(), OperationState::Staged)?;
        }
        Ok(report)
    }

    fn fetch_one(
        &self,
        plan: &UpdatePlan,
        file: &ManifestFile,
        source: &dyn BlobSource,
    ) -> Result<FetchState> {
        let downloads = self.layout.downloads_dir(plan.component(), plan.version());
        let part = part_path(&downloads, &file.path)?;
        ensure_parent(&part)?;

        let mut have = file_len(&part)?;
        if have > file.bytes {
            // A part longer than the file can never become the file, so there is
            // nothing here to resume from.
            remove_file(&part)?;
            have = 0;
        }
        let resumed_from = have;

        if have < file.bytes {
            let mut reader = source.open(&BlobRequest {
                component: plan.component(),
                version: plan.version(),
                file,
                offset: have,
            })?;
            let mut sink = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&part)
                .map_err(|source| Error::Io {
                    path: part.clone(),
                    source,
                })?;

            // Read at most one byte past the end, so a source that sends too
            // much is detected rather than quietly trimmed to a length that
            // looks right.
            let limit = file.bytes.saturating_sub(have).saturating_add(1);
            let mut buf = vec![0u8; 1 << 20];
            let mut taken = 0u64;
            while taken < limit {
                let want = std::cmp::min(buf.len() as u64, limit - taken) as usize;
                let n = reader.read(&mut buf[..want]).map_err(|source| Error::Io {
                    path: part.clone(),
                    source,
                })?;
                if n == 0 {
                    break;
                }
                sink.write_all(&buf[..n]).map_err(|source| Error::Io {
                    path: part.clone(),
                    source,
                })?;
                taken += n as u64;
            }
            // Resume is only meaningful if the bytes are really on the disk.
            sink.sync_all().map_err(|source| Error::Io {
                path: part.clone(),
                source,
            })?;
            have = have.saturating_add(taken);
        }

        if have < file.bytes {
            return Ok(FetchState::Incomplete {
                have,
                want: file.bytes,
            });
        }

        let actual = hash_file(&part)?;
        if have > file.bytes || actual != file.sha256 {
            remove_file(&part)?;
            return Ok(FetchState::Corrupt {
                resumed_from,
                expected: file.sha256.clone(),
                actual,
            });
        }

        // The only door into staging, and it is on the far side of a hash check.
        // Everything downstream can therefore treat "present in staging" as
        // "verified at least once", and still checks again before promoting.
        let staged = resolve_under(
            &self.layout.staging_dir(plan.component(), plan.version()),
            &file.path,
        )?;
        ensure_parent(&staged)?;
        self.platform.files.atomic_replace(&part, &staged)?;
        Ok(FetchState::Staged)
    }

    /// Re-hash the staged set against the manifest.
    ///
    /// Called by [`ContentService::promote`] every time, and callable on its own
    /// after a restart. The download's own hash check is not reused: it proved
    /// what arrived, not what is on disk now.
    pub fn verify_staged(&self, plan: &UpdatePlan) -> Result<StagedVerdict> {
        let staging = self.layout.staging_dir(plan.component(), plan.version());
        let preserved = plan.preserved_set();
        let mut missing = Vec::new();
        let mut wrong = Vec::new();

        for fetch in &plan.fetches {
            if preserved.contains(fetch.path.as_str()) {
                continue;
            }
            let file = plan
                .manifest_file(&fetch.path)
                .ok_or_else(|| refuse(format!("{:?} is not in the manifest", fetch.path)))?;
            let path = resolve_under(&staging, &fetch.path)?;
            match std::fs::metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(fetch.path.clone())
                }
                Err(source) => return Err(Error::Io { path, source }),
                Ok(meta) => {
                    if meta.len() != file.bytes || hash_file(&path)? != file.sha256 {
                        wrong.push(fetch.path.clone());
                    }
                }
            }
        }

        if missing.is_empty() && wrong.is_empty() {
            Ok(StagedVerdict::Complete)
        } else {
            Ok(StagedVerdict::NotReady { missing, wrong })
        }
    }

    /// Move the verified staged files into the live tree.
    ///
    /// Order, and every step of it is load bearing:
    ///
    /// 1. Same volume, or refuse. A cross-volume swap is a copy and a delete.
    /// 2. Re-hash the staged set, or refuse.
    /// 3. Refuse if any live file is open elsewhere, rather than failing part
    ///    way through. This is the quiescence rule.
    /// 4. Journal the swap, so a process death is recoverable.
    /// 5. Per file: rename the live copy aside, then rename the staged copy in.
    ///    Any failure reverses every move made so far.
    /// 6. Write the pointer last. It is the only step that has to be atomic, and
    ///    it is what makes the new version official.
    pub fn promote(&self, plan: &UpdatePlan) -> Result<Promoted> {
        let live_root = self.layout.live_root(&plan.manifest)?;
        let staging = self.layout.staging_dir(plan.component(), plan.version());
        let prior_dir = self.layout.prior_dir(plan.component());
        std::fs::create_dir_all(&live_root).map_err(|source| Error::Io {
            path: live_root.clone(),
            source,
        })?;
        std::fs::create_dir_all(&staging).map_err(|source| Error::Io {
            path: staging.clone(),
            source,
        })?;

        if !self.platform.files.same_volume(&staging, &live_root)? {
            return Ok(Promoted::Refused(PromotionRefusal::CrossVolume {
                staging,
                live: live_root,
            }));
        }

        if let StagedVerdict::NotReady { missing, wrong } = self.verify_staged(plan)? {
            return Ok(Promoted::Refused(PromotionRefusal::StagingIncomplete {
                component: plan.component().to_string(),
                missing,
                wrong,
            }));
        }

        let preserved = plan.preserved_set();
        let to_promote: Vec<&PlannedFetch> = plan
            .fetches
            .iter()
            .filter(|f| !preserved.contains(f.path.as_str()))
            .collect();

        // Files the live version ships and the incoming one does not. Known
        // only from the manifest cached at the last promotion; with no cache,
        // nothing is retired, which is the safe direction.
        let retired = self.retired_paths(plan)?;

        for path in to_promote.iter().map(|f| f.path.as_str()).chain(retired.iter().map(String::as_str)) {
            let live = resolve_under(&live_root, path)?;
            if live.exists() && self.platform.files.is_locked(&live) {
                return Ok(Promoted::Refused(PromotionRefusal::InUse { path: live }));
            }
        }

        // Read the pointer again rather than trusting the plan's copy. The plan
        // may have been built minutes ago, and this decides what rollback will
        // be told to go back to.
        let installed = self.installed(plan.component())?;
        let previous = if plan.is_repair() {
            installed.as_ref().and_then(|p| p.previous.clone())
        } else {
            installed.as_ref().map(|p| p.current.clone())
        };

        self.record(Operation {
            component: plan.component().to_string(),
            version: plan.version().to_string(),
            state: OperationState::Swapping,
            live_root: live_root.clone(),
            staging: staging.clone(),
            prior: prior_dir.clone(),
        })?;

        // Exactly one prior version is retained. The copy about to be moved
        // aside is the one worth keeping, so whatever an earlier update left in
        // there is dead the moment this swap begins. Nothing live is deleted
        // here, and nothing is deleted at all until the swap is under way.
        remove_dir_all(&prior_dir)?;
        remove_file(&self.layout.prior_record_path(plan.component()))?;
        PriorRecord {
            schema: PRIOR_SCHEMA.to_string(),
            component: plan.component().to_string(),
            version: previous.clone(),
            live_root: live_root.clone(),
        }
        .save(&self.layout.prior_record_path(plan.component()))?;

        let mut done: Vec<DoneMove> = Vec::new();
        let mut promoted: Vec<String> = Vec::new();

        for fetch in &to_promote {
            let live = resolve_under(&live_root, &fetch.path)?;
            let staged = resolve_under(&staging, &fetch.path)?;
            let prior = resolve_under(&prior_dir, &fetch.path)?;

            let mut moved_aside = None;
            if live.exists() {
                if let Err(e) = ensure_parent(&prior)
                    .and_then(|()| self.platform.files.atomic_replace(&live, &prior))
                {
                    return Ok(self.reverse(done, plan, e.to_string()));
                }
                moved_aside = Some(prior);
            }
            if let Err(e) = ensure_parent(&live)
                .and_then(|()| self.platform.files.atomic_replace(&staged, &live))
            {
                // Record the aside even though the replacement never landed, or
                // the reversal would leave this path with no file at all.
                if let Some(prior) = moved_aside {
                    done.push(DoneMove {
                        live: live.clone(),
                        prior: Some(prior),
                        staged: staged.clone(),
                    });
                }
                return Ok(self.reverse(done, plan, e.to_string()));
            }
            done.push(DoneMove {
                live,
                prior: moved_aside,
                staged,
            });
            promoted.push(fetch.path.clone());
        }

        // Retire what the new version dropped: moved into prior, exactly like a
        // replaced file, so the same reversal and the same rollback bring it
        // back. Never deleted. A file the previous version never shipped is
        // not in the cached manifest and is not touched.
        let mut retired_now: Vec<String> = Vec::new();
        for path in &retired {
            let (live, prior, staged) = match (
                resolve_under(&live_root, path),
                resolve_under(&prior_dir, path),
                resolve_under(&staging, path),
            ) {
                (Ok(l), Ok(p), Ok(s)) => (l, p, s),
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                    return Ok(self.reverse(done, plan, e.to_string()))
                }
            };
            if !live.exists() {
                continue;
            }
            if let Err(e) = ensure_parent(&prior)
                .and_then(|()| self.platform.files.atomic_replace(&live, &prior))
            {
                return Ok(self.reverse(done, plan, e.to_string()));
            }
            done.push(DoneMove {
                live,
                prior: Some(prior),
                staged,
            });
            retired_now.push(path.clone());
        }

        let pointer = Pointer::new(
            plan.component(),
            InstalledVersion {
                version: plan.version().to_string(),
                sequence: plan.sequence(),
            },
            previous.clone(),
        );
        if let Err(e) = pointer.save(self.platform, &self.layout.pointer_path(plan.component())) {
            return Ok(self.reverse(done, plan, e.to_string()));
        }

        // After the pointer, on purpose. The swap is complete at this point and
        // the cache only serves the NEXT update's retirement pass. A failure
        // here degrades that pass to "retire nothing", which is safe; reversing
        // a finished swap over it would not be.
        if let Err(e) = self.save_manifest_cache(plan) {
            tracing::warn!(
                "could not record the live manifest for {}: {e}. The next update will \
                 not retire dropped files",
                plan.component()
            );
        }

        // In flight no longer. The pointer file is the record of what is live,
        // so a journal row here would only be a second answer to that question.
        self.clear_operation(plan.component())?;

        Ok(Promoted::Applied(PromotionReceipt {
            component: plan.component().to_string(),
            version: plan.version().to_string(),
            previous,
            promoted,
            preserved: plan.preserved.clone(),
            prior_dir: done.iter().any(|m| m.prior.is_some()).then_some(prior_dir),
            retired: retired_now,
        }))
    }

    /// Paths the cached live manifest lists that the incoming one does not.
    ///
    /// Empty when there is no cache, which is every first install and any
    /// machine whose cache was lost: nothing is retired on a guess. A cache
    /// that does not parse is treated the same way, with a warning, because a
    /// broken local record must not be able to block an update.
    fn retired_paths(&self, plan: &UpdatePlan) -> Result<Vec<String>> {
        let path = self.layout.manifest_path(plan.component());
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(Error::Io { path, source }),
        };
        let old: Component = match serde_json::from_slice(&bytes) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "the cached manifest at {} does not parse ({e}); retiring nothing",
                    path.display()
                );
                return Ok(Vec::new());
            }
        };
        let keep: BTreeSet<&str> = plan.manifest.files.iter().map(|f| f.path.as_str()).collect();
        Ok(old
            .files
            .iter()
            .map(|f| f.path.clone())
            .filter(|p| !keep.contains(p.as_str()))
            .collect())
    }

    /// Record the manifest that is now live, write-temp-then-rename.
    fn save_manifest_cache(&self, plan: &UpdatePlan) -> Result<()> {
        let path = self.layout.manifest_path(plan.component());
        ensure_parent(&path)?;
        let temp = path.with_file_name(format!("{MANIFEST_FILE}.new"));
        let json = serde_json::to_vec_pretty(&plan.manifest)
            .map_err(|e| refuse(format!("could not encode the live manifest: {e}")))?;
        std::fs::write(&temp, json).map_err(|source| Error::Io {
            path: temp.clone(),
            source,
        })?;
        self.platform.files.atomic_replace(&temp, &path)
    }

    /// Undo every move in `done`, newest first.
    ///
    /// Best effort, and failures here are logged rather than returned: the caller
    /// is already being told about the failure that started this, and replacing
    /// that reason with a second one hides why the update failed in the first
    /// place. The prior copy goes back last, so no path is ever left empty for
    /// longer than one rename.
    fn reverse(&self, done: Vec<DoneMove>, plan: &UpdatePlan, reason: String) -> Promoted {
        let mut restored = Vec::new();
        let mut returned = Vec::new();

        for m in done.iter().rev() {
            if m.live.exists() {
                // Put the new file back in staging so the retry does not have to
                // download it again.
                match ensure_parent(&m.staged)
                    .and_then(|()| self.platform.files.atomic_replace(&m.live, &m.staged))
                {
                    Ok(()) => returned.push(m.staged.clone()),
                    Err(e) => tracing::warn!(
                        "could not return {} to staging during rollback: {e}",
                        m.live.display()
                    ),
                }
            }
            if let Some(prior) = &m.prior {
                match self.platform.files.atomic_replace(prior, &m.live) {
                    Ok(()) => restored.push(m.live.clone()),
                    Err(e) => tracing::warn!(
                        "could not restore {} from {}: {e}",
                        m.live.display(),
                        prior.display()
                    ),
                }
            }
        }

        if let Err(e) = self.set_state(plan.component(), OperationState::Failed) {
            tracing::warn!("could not record the failed swap: {e}");
        }

        Promoted::RolledBack {
            reason,
            restored,
            returned_to_staging: returned,
        }
    }

    /// Go back to the retained prior version.
    ///
    /// The live files are moved into staging rather than deleted, so going
    /// forward again is a rename and not a download.
    pub fn rollback(&self, component: &str) -> Result<RollbackOutcome> {
        let Some(pointer) = self.installed(component)? else {
            return Ok(RollbackOutcome::Refused(RollbackRefusal::NotInstalled {
                component: component.to_string(),
            }));
        };
        let Some(previous) = pointer.previous.clone() else {
            return Ok(RollbackOutcome::Refused(RollbackRefusal::NoPrevious {
                component: component.to_string(),
                current_version: pointer.current.version,
            }));
        };

        let prior_dir = self.layout.prior_dir(component);
        let files = relative_files(&prior_dir)?;
        if files.is_empty() {
            return Ok(RollbackOutcome::Refused(RollbackRefusal::NoLocalPrior {
                component: component.to_string(),
                previous_version: previous.version,
            }));
        }

        // The retained bytes and the pointer have to agree about what is on
        // disk. Restoring anyway would install files under a version label that
        // was never checked against them.
        let record = PriorRecord::load(&self.layout.prior_record_path(component));
        let Some(record) = record.filter(|r| r.version.as_ref() == Some(&previous)) else {
            return Ok(RollbackOutcome::Refused(
                RollbackRefusal::UnknownPriorVersion {
                    component: component.to_string(),
                },
            ));
        };

        for rel in &files {
            let live = record.live_root.join(rel);
            if live.exists() && self.platform.files.is_locked(&live) {
                return Ok(RollbackOutcome::Refused(RollbackRefusal::InUse {
                    path: live,
                }));
            }
        }

        let staging = self.layout.staging_dir(component, &pointer.current.version);
        let (restored, returned) = self.restore_tree(&prior_dir, &record.live_root, &staging)?;

        // Exactly one prior version is retained, so after going back there is
        // nothing further to go back to. `None` is the honest answer.
        Pointer::new(component, previous.clone(), None)
            .save(self.platform, &self.layout.pointer_path(component))?;
        remove_dir_all(&prior_dir)?;
        remove_file(&self.layout.prior_record_path(component))?;
        // The cache described the version just rolled away from. Left in
        // place it would make the next update retire files against the wrong
        // baseline; with no cache the next update retires nothing, which is
        // the safe direction.
        remove_file(&self.layout.manifest_path(component))?;
        self.clear_operation(component)?;

        Ok(RollbackOutcome::Restored {
            to: previous,
            restored,
            returned_to_staging: returned,
        })
    }

    /// Resolve anything the last run left half done. Call once at startup,
    /// before planning anything new.
    ///
    /// An interrupted swap is always resolved backwards, to the version that was
    /// working. The prior directory, not the journal, decides which files move:
    /// a crash can interrupt the move-aside loop at any point, so the only
    /// truthful list of what was displaced is what is actually sitting there.
    pub fn recover(&self) -> Result<Vec<RecoveredSwap>> {
        let journal_path = self.layout.journal_path();
        let mut journal = Journal::load(&journal_path);
        let interrupted: Vec<Operation> = journal.interrupted().into_iter().cloned().collect();
        let mut recovered = Vec::new();

        for op in interrupted {
            let (restored, returned) = self.restore_tree(&op.prior, &op.live_root, &op.staging)?;

            let pointer_path = self.layout.pointer_path(&op.component);
            let mut pointer_rolled_back = false;
            let mut unresolved = None;
            if let Some(pointer) = Pointer::load(&pointer_path)? {
                if pointer.current.version == op.version {
                    match pointer.previous.clone() {
                        Some(prev) => {
                            Pointer::new(&op.component, prev, None)
                                .save(self.platform, &pointer_path)?;
                            pointer_rolled_back = true;
                        }
                        None => {
                            unresolved = Some(format!(
                                "{} was rolled back to the files that were on \
                                 disk before {} was installed, but nothing \
                                 records what version those are. Verify the \
                                 component before playing",
                                op.component, op.version
                            ));
                        }
                    }
                }
            }

            remove_dir_all(&op.prior)?;
            remove_file(&self.layout.prior_record_path(&op.component))?;
            journal.remove(&op.component);

            recovered.push(RecoveredSwap {
                component: op.component,
                interrupted_version: op.version,
                restored,
                returned_to_staging: returned,
                pointer_rolled_back,
                unresolved,
            });
        }

        // A row left in DOWNLOADING or STAGED needs no action: the parts and the
        // staged files are on disk, they are re-hashed before use, and the next
        // download call resumes them.
        journal.save(self.platform, &journal_path)?;
        Ok(recovered)
    }

    /// Throw away a partial or superseded staging set.
    ///
    /// Downloads and staging are derived by definition, so this is the one
    /// deletion in the module that needs no ceremony. It never touches the live
    /// tree or the retained prior version.
    pub fn discard_staging(&self, component: &str, version: &str) -> Result<()> {
        remove_dir_all(&self.layout.downloads_dir(component, version))?;
        remove_dir_all(&self.layout.staging_dir(component, version))?;
        self.clear_operation(component)
    }

    /// Move every file under `from` onto `live_root`, first putting whatever is
    /// live into `staging` so nothing that was verified is thrown away.
    fn restore_tree(
        &self,
        from: &Path,
        live_root: &Path,
        staging: &Path,
    ) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
        let mut restored = Vec::new();
        let mut returned = Vec::new();

        for rel in relative_files(from)? {
            let source = from.join(&rel);
            let live = live_root.join(&rel);
            let staged = staging.join(&rel);

            if live.exists() {
                match ensure_parent(&staged)
                    .and_then(|()| self.platform.files.atomic_replace(&live, &staged))
                {
                    Ok(()) => returned.push(staged),
                    // Not fatal. The restore below replaces the live file either
                    // way, and all that is lost is a cheap roll forward.
                    Err(e) => tracing::warn!("could not keep {} for a retry: {e}", live.display()),
                }
            }
            ensure_parent(&live)?;
            self.platform.files.atomic_replace(&source, &live)?;
            restored.push(live);
        }
        Ok((restored, returned))
    }

    fn record(&self, op: Operation) -> Result<()> {
        let path = self.layout.journal_path();
        let mut journal = Journal::load(&path);
        journal.upsert(op);
        journal.save(self.platform, &path)
    }

    fn set_state(&self, component: &str, state: OperationState) -> Result<()> {
        let path = self.layout.journal_path();
        let mut journal = Journal::load(&path);
        if let Some(op) = journal
            .operations
            .iter_mut()
            .find(|o| o.component == component)
        {
            op.state = state;
        }
        journal.save(self.platform, &path)
    }

    fn clear_operation(&self, component: &str) -> Result<()> {
        let path = self.layout.journal_path();
        let mut journal = Journal::load(&path);
        journal.remove(component);
        journal.save(self.platform, &path)
    }
}

fn part_path(dir: &Path, relative: &str) -> Result<PathBuf> {
    let resolved = resolve_under(dir, relative)?;
    let name = resolved
        .file_name()
        .ok_or_else(|| refuse(format!("{relative:?} names no file")))?;
    let mut part = name.to_os_string();
    part.push(".part");
    Ok(resolved.with_file_name(part))
}

/// Remove a file, treating absence as success.
fn remove_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Remove a directory tree, treating absence as success.
fn remove_dir_all(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Every file under `root`, as paths relative to it, sorted for a stable report.
fn relative_files(root: &Path) -> Result<Vec<PathBuf>> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(source) => {
                return Err(Error::Io {
                    path: dir.to_path_buf(),
                    source,
                })
            }
        };
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out)?;
            } else if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_path_buf());
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::COMPONENT_SCHEMA;
    use crate::platform::{FileOps, LockGuard};
    use std::sync::Mutex;

    // -----------------------------------------------------------------------
    // Test doubles
    // -----------------------------------------------------------------------

    /// Accepts exactly one signature. Deliberately not exported: the product has
    /// no key yet, and a permissive verifier that escaped this module would make
    /// every call site look checked while checking nothing.
    #[derive(Debug)]
    struct FixedSignature(&'static [u8]);

    impl SignatureVerifier for FixedSignature {
        fn verify(&self, _signed_bytes: &[u8], signature: &[u8]) -> Result<bool> {
            Ok(signature == self.0)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Serving {
        /// Honest: serves from the requested offset to the end.
        Whole,
        /// Drops the connection half way, which is what hotel wifi does.
        FirstHalf,
        /// Right length, wrong bytes.
        WrongBytes,
        /// Serves the whole blob whatever offset was asked for, which is what a
        /// host without `Range` support does.
        IgnoresOffset,
    }

    #[derive(Debug)]
    struct Blobs {
        bytes: BTreeMap<String, Vec<u8>>,
        serving: Serving,
        requests: Mutex<Vec<(String, u64)>>,
    }

    impl Blobs {
        fn new(serving: Serving) -> Self {
            Self {
                bytes: BTreeMap::new(),
                serving,
                requests: Mutex::new(Vec::new()),
            }
        }

        fn with(mut self, path: &str, bytes: &[u8]) -> Self {
            self.bytes.insert(path.to_string(), bytes.to_vec());
            self
        }

        fn requests(&self) -> Vec<(String, u64)> {
            self.requests.lock().expect("request log").clone()
        }
    }

    impl BlobSource for Blobs {
        fn open(&self, req: &BlobRequest<'_>) -> Result<Box<dyn Read>> {
            self.requests
                .lock()
                .expect("request log")
                .push((req.file.path.clone(), req.offset));
            let all = self
                .bytes
                .get(&req.file.path)
                .ok_or_else(|| refuse(format!("no blob for {:?}", req.file.path)))?;
            let from = (req.offset as usize).min(all.len());
            let body = match self.serving {
                Serving::Whole => all[from..].to_vec(),
                Serving::FirstHalf => {
                    let end = (all.len() / 2).max(from);
                    all[from..end].to_vec()
                }
                Serving::WrongBytes => vec![b'X'; all.len().saturating_sub(from)],
                Serving::IgnoresOffset => all.clone(),
            };
            Ok(Box::new(std::io::Cursor::new(body)))
        }
    }

    /// Fails the first rename that lands on `poison`, then behaves normally.
    ///
    /// Keyed on the destination rather than a call count so the test says what
    /// it means: this is the file whose replacement fails. Once, because that is
    /// what an antivirus scan window or a momentarily open handle looks like,
    /// and because a permanently unwritable destination cannot be rolled back
    /// to either, which would be testing a different thing.
    struct FailingReplace {
        inner: Box<dyn FileOps>,
        poison: PathBuf,
        fired: Mutex<bool>,
    }

    impl std::fmt::Debug for FailingReplace {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FailingReplace")
                .field("poison", &self.poison)
                .finish()
        }
    }

    impl FileOps for FailingReplace {
        fn atomic_replace(&self, src: &Path, dest: &Path) -> Result<()> {
            let mut fired = self.fired.lock().expect("injection flag");
            if dest == self.poison && !*fired {
                *fired = true;
                return Err(refuse(format!(
                    "injected failure replacing {}",
                    dest.display()
                )));
            }
            drop(fired);
            self.inner.atomic_replace(src, dest)
        }
        fn same_volume(&self, a: &Path, b: &Path) -> Result<bool> {
            self.inner.same_volume(a, b)
        }
        fn hardlink(&self, src: &Path, dest: &Path) -> Result<()> {
            self.inner.hardlink(src, dest)
        }
        fn is_locked(&self, path: &Path) -> bool {
            self.inner.is_locked(path)
        }
        fn lock_exclusive(&self, path: &Path) -> Result<Option<LockGuard>> {
            self.inner.lock_exclusive(path)
        }
    }

    /// Only exists so the real [`FileOps`] can be moved out of a `Platform`.
    #[derive(Debug)]
    struct NoFiles;

    impl FileOps for NoFiles {
        fn atomic_replace(&self, _src: &Path, _dest: &Path) -> Result<()> {
            Err(refuse("placeholder file ops"))
        }
        fn same_volume(&self, _a: &Path, _b: &Path) -> Result<bool> {
            Err(refuse("placeholder file ops"))
        }
        fn hardlink(&self, _src: &Path, _dest: &Path) -> Result<()> {
            Err(refuse("placeholder file ops"))
        }
        fn is_locked(&self, _path: &Path) -> bool {
            false
        }
        fn lock_exclusive(&self, _path: &Path) -> Result<Option<LockGuard>> {
            Ok(None)
        }
    }

    fn platform_failing_on(poison: &Path) -> Platform {
        let mut platform = Platform::detect();
        let real = std::mem::replace(&mut platform.files, Box::new(NoFiles));
        platform.files = Box::new(FailingReplace {
            inner: real,
            poison: poison.to_path_buf(),
            fired: Mutex::new(false),
        });
        platform
    }

    // -----------------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------------

    fn file_of(path: &str, bytes: &[u8]) -> ManifestFile {
        ManifestFile {
            path: path.to_string(),
            bytes: bytes.len() as u64,
            sha256: Sha256Hex::from_bytes(bytes),
        }
    }

    fn manifest(
        version: &str,
        sequence: u64,
        customizable: bool,
        files: Vec<ManifestFile>,
    ) -> Vec<u8> {
        let component = Component {
            schema: COMPONENT_SCHEMA.to_string(),
            component: "offline-content".to_string(),
            version: version.to_string(),
            sequence,
            install_root: DataDir::LibraryOffline.relative().to_string(),
            customizable,
            files,
        };
        serde_json::to_vec(&component).expect("a component manifest encodes")
    }

    fn signed(manifest_bytes: &[u8], release_sequence: u64) -> TrustedRelease {
        let pin = Sha256Hex::from_bytes(manifest_bytes);
        let json = format!(
            r#"{{"schema":"nsr.release.v1","version":"2.4.0","sequence":{release_sequence},
                 "components":{{"offline-content":
                   {{"url":"https://example.invalid/offline.json","sha256":"{pin}"}}}}}}"#
        );
        TrustedRelease::open(&FixedSignature(b"good"), json.as_bytes(), b"good")
            .expect("a release signed by the trusted key opens")
    }

    fn live_root(data_root: &Path) -> PathBuf {
        data_root.join(DataDir::LibraryOffline.relative())
    }

    fn write_live(data_root: &Path, rel: &str, bytes: &[u8]) {
        let path = live_root(data_root).join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("live directory");
        }
        std::fs::write(path, bytes).expect("write live file");
    }

    fn read_live(data_root: &Path, rel: &str) -> Vec<u8> {
        std::fs::read(live_root(data_root).join(rel)).expect("read live file")
    }

    fn install_pointer(platform: &Platform, service: &ContentService<'_>, version: &str, seq: u64) {
        Pointer::new(
            "offline-content",
            InstalledVersion {
                version: version.to_string(),
                sequence: seq,
            },
            None,
        )
        .save(platform, &service.layout().pointer_path("offline-content"))
        .expect("write the installed-version record");
    }

    fn expect_plan(outcome: PlanOutcome) -> Box<UpdatePlan> {
        match outcome {
            PlanOutcome::Update(plan) => plan,
            other => panic!("expected an update plan, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Invariant 1: nothing is promoted before its hash matches the manifest
    // -----------------------------------------------------------------------

    #[test]
    fn bytes_that_do_not_match_the_manifest_hash_never_reach_staging_or_the_live_tree() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let good = b"the offline image, release 2.4.0";
        let bytes = manifest("2.4.0", 8, false, vec![file_of("NSR.iso", good)]);
        let release = signed(&bytes, 12);
        let plan = expect_plan(
            service
                .plan(
                    &release,
                    "offline-content",
                    &bytes,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("planning is not fallible here"),
        );

        let source = Blobs::new(Serving::WrongBytes).with("NSR.iso", good);
        let report = service.download(&plan, &source).expect("download runs");

        assert!(!report.is_complete());
        assert_eq!(report.corrupt(), vec!["NSR.iso"]);
        assert!(
            !service
                .layout()
                .staging_dir("offline-content", "2.4.0")
                .join("NSR.iso")
                .exists(),
            "unverified bytes must never reach staging"
        );
        assert!(
            !service
                .layout()
                .downloads_dir("offline-content", "2.4.0")
                .join("NSR.iso.part")
                .exists(),
            "a poisoned part must be deleted, or every resume carries the bad bytes"
        );

        match service.promote(&plan).expect("promotion runs") {
            Promoted::Refused(PromotionRefusal::StagingIncomplete { missing, .. }) => {
                assert_eq!(missing, vec!["NSR.iso"]);
            }
            other => panic!("promotion must refuse an unverified set, got {other:?}"),
        }
        assert!(
            !live_root(dir.path()).join("NSR.iso").exists(),
            "the live tree must be untouched"
        );
        assert!(
            service
                .installed("offline-content")
                .expect("pointer read")
                .is_none(),
            "nothing may claim to be installed"
        );
    }

    /// Staging is re-hashed at promotion time, not trusted because a download
    /// once said it was fine. Something editing a staged file between the two is
    /// exactly the case the second hash exists for.
    #[test]
    fn a_staged_file_altered_after_download_is_caught_before_promotion() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let good = b"the offline image, release 2.4.0";
        let bytes = manifest("2.4.0", 8, false, vec![file_of("NSR.iso", good)]);
        let release = signed(&bytes, 12);
        let plan = expect_plan(
            service
                .plan(
                    &release,
                    "offline-content",
                    &bytes,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("plan"),
        );

        let source = Blobs::new(Serving::Whole).with("NSR.iso", good);
        assert!(service
            .download(&plan, &source)
            .expect("download")
            .is_complete());

        let staged = service
            .layout()
            .staging_dir("offline-content", "2.4.0")
            .join("NSR.iso");
        std::fs::write(&staged, b"the offline image, tampered wi").expect("edit staged file");

        match service.promote(&plan).expect("promotion runs") {
            Promoted::Refused(PromotionRefusal::StagingIncomplete { wrong, .. }) => {
                assert_eq!(wrong, vec!["NSR.iso"]);
            }
            other => panic!("a changed staged file must not be promoted, got {other:?}"),
        }
        assert!(!live_root(dir.path()).join("NSR.iso").exists());
    }

    // -----------------------------------------------------------------------
    // Invariant 2: a partial download is never mistaken for a complete one
    // -----------------------------------------------------------------------

    #[test]
    fn a_truncated_download_stays_a_part_file_and_resumes_from_where_it_stopped() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let good = b"forty two bytes of offline image content..";
        let want = good.len() as u64;
        let half = (good.len() / 2) as u64;
        let bytes = manifest("2.4.0", 8, false, vec![file_of("NSR.iso", good)]);
        let release = signed(&bytes, 12);
        let plan = expect_plan(
            service
                .plan(
                    &release,
                    "offline-content",
                    &bytes,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("plan"),
        );

        let flaky = Blobs::new(Serving::FirstHalf).with("NSR.iso", good);
        let report = service.download(&plan, &flaky).expect("download");
        assert_eq!(
            report.results["NSR.iso"],
            FetchState::Incomplete { have: half, want }
        );
        assert!(!report.is_complete(), "half a file is not a file");
        assert_eq!(report.remaining_bytes(&plan), want - half);

        let part = service
            .layout()
            .downloads_dir("offline-content", "2.4.0")
            .join("NSR.iso.part");
        assert_eq!(
            std::fs::metadata(&part).expect("part file").len(),
            half,
            "the partial bytes are kept so the resume is cheap"
        );
        assert!(
            !service
                .layout()
                .staging_dir("offline-content", "2.4.0")
                .join("NSR.iso")
                .exists(),
            "a partial download must not appear as a staged file"
        );
        assert!(matches!(
            service.promote(&plan).expect("promotion runs"),
            Promoted::Refused(PromotionRefusal::StagingIncomplete { .. })
        ));

        let honest = Blobs::new(Serving::Whole).with("NSR.iso", good);
        let report = service.download(&plan, &honest).expect("download");
        assert!(report.is_complete());
        assert_eq!(
            honest.requests(),
            vec![("NSR.iso".to_string(), half)],
            "the resume must ask for the remainder, not the whole file again"
        );

        assert!(matches!(
            service.promote(&plan).expect("promotion runs"),
            Promoted::Applied(_)
        ));
        assert_eq!(read_live(dir.path(), "NSR.iso"), good);
    }

    /// A host with no `Range` support serves the whole blob and the appended
    /// copy makes the file too long. That has to read as corruption, not as a
    /// complete download, and the part has to go so the retry starts clean.
    #[test]
    fn a_source_that_ignores_the_resume_offset_cannot_produce_a_promotable_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let good = b"twenty one bytes here";
        let half = (good.len() / 2) as u64;
        let bytes = manifest("2.4.0", 8, false, vec![file_of("NSR.iso", good)]);
        let release = signed(&bytes, 12);
        let plan = expect_plan(
            service
                .plan(
                    &release,
                    "offline-content",
                    &bytes,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("plan"),
        );

        let flaky = Blobs::new(Serving::FirstHalf).with("NSR.iso", good);
        service.download(&plan, &flaky).expect("download");

        let rude = Blobs::new(Serving::IgnoresOffset).with("NSR.iso", good);
        let report = service.download(&plan, &rude).expect("download");
        match &report.results["NSR.iso"] {
            FetchState::Corrupt { resumed_from, .. } => assert_eq!(
                *resumed_from, half,
                "the report must show the resume, so the cause is diagnosable"
            ),
            other => panic!("an over-long body must read as corrupt, got {other:?}"),
        }
        assert!(!service
            .layout()
            .staging_dir("offline-content", "2.4.0")
            .join("NSR.iso")
            .exists());
    }

    // -----------------------------------------------------------------------
    // Invariant 3: a failed update leaves the previous version working
    // -----------------------------------------------------------------------

    #[test]
    fn a_swap_that_fails_part_way_restores_the_previous_version_and_keeps_the_staged_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let (old_a, old_b) = (b"version one, file a", b"version one, file b");
        let (new_a, new_b) = (b"version two, file a", b"version two, file b");
        write_live(dir.path(), "a.dat", old_a);
        write_live(dir.path(), "b.dat", old_b);
        install_pointer(&platform, &service, "1.0.0", 7);

        let bytes = manifest(
            "2.0.0",
            8,
            false,
            vec![file_of("a.dat", new_a), file_of("b.dat", new_b)],
        );
        let release = signed(&bytes, 12);
        let plan = expect_plan(
            service
                .plan(
                    &release,
                    "offline-content",
                    &bytes,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("plan"),
        );
        assert_eq!(plan.fetches.len(), 2);

        let source = Blobs::new(Serving::Whole)
            .with("a.dat", new_a)
            .with("b.dat", new_b);
        assert!(service
            .download(&plan, &source)
            .expect("download")
            .is_complete());

        // The second file's replacement fails. The first file has already been
        // swapped by then, which is the state that used to be unrecoverable.
        let poisoned = platform_failing_on(&live_root(dir.path()).join("b.dat"));
        let failing = ContentService::new(&poisoned, dir.path());
        match failing.promote(&plan).expect("promotion runs") {
            Promoted::RolledBack {
                restored,
                returned_to_staging,
                ..
            } => {
                assert_eq!(restored.len(), 2, "both files must be put back");
                assert_eq!(
                    returned_to_staging.len(),
                    1,
                    "only the file that actually landed goes back to staging"
                );
            }
            other => panic!("a failed swap must roll back, got {other:?}"),
        }

        assert_eq!(
            read_live(dir.path(), "a.dat"),
            old_a,
            "the game must still run"
        );
        assert_eq!(read_live(dir.path(), "b.dat"), old_b);
        assert_eq!(
            service
                .installed("offline-content")
                .expect("pointer")
                .expect("still installed")
                .current
                .version,
            "1.0.0",
            "a failed update must not claim to have installed anything"
        );

        let staging = service.layout().staging_dir("offline-content", "2.0.0");
        assert_eq!(
            std::fs::read(staging.join("a.dat")).expect("staged a"),
            new_a,
            "the verified bytes must survive the rollback, or the retry re-downloads"
        );
        assert_eq!(
            std::fs::read(staging.join("b.dat")).expect("staged b"),
            new_b
        );

        // And the retry, on a healthy filesystem, completes.
        let receipt = match service.promote(&plan).expect("promotion runs") {
            Promoted::Applied(receipt) => receipt,
            other => panic!("the retry must apply, got {other:?}"),
        };
        assert_eq!(read_live(dir.path(), "a.dat"), new_a);
        assert_eq!(read_live(dir.path(), "b.dat"), new_b);
        assert_eq!(
            receipt.previous.expect("a previous version").version,
            "1.0.0"
        );
    }

    #[test]
    fn an_interrupted_swap_is_reversed_on_the_next_start() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let (old_a, new_a) = (b"version one, file a", b"version two, file a");
        let staging = service.layout().staging_dir("offline-content", "2.0.0");
        let prior = service.layout().prior_dir("offline-content");
        std::fs::create_dir_all(&staging).expect("staging");
        std::fs::create_dir_all(&prior).expect("prior");

        // Exactly the state a kill leaves behind: the new file is live, the old
        // one is aside, the pointer has not been written yet.
        write_live(dir.path(), "a.dat", new_a);
        std::fs::write(prior.join("a.dat"), old_a).expect("prior file");
        PriorRecord {
            schema: PRIOR_SCHEMA.to_string(),
            component: "offline-content".to_string(),
            version: Some(InstalledVersion {
                version: "1.0.0".to_string(),
                sequence: 7,
            }),
            live_root: live_root(dir.path()),
        }
        .save(&service.layout().prior_record_path("offline-content"))
        .expect("prior record");
        install_pointer(&platform, &service, "1.0.0", 7);
        Journal {
            schema: JOURNAL_SCHEMA.to_string(),
            operations: vec![Operation {
                component: "offline-content".to_string(),
                version: "2.0.0".to_string(),
                state: OperationState::Swapping,
                live_root: live_root(dir.path()),
                staging: staging.clone(),
                prior: prior.clone(),
            }],
        }
        .save(&platform, &service.layout().journal_path())
        .expect("journal");

        let recovered = service.recover().expect("recovery runs");
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].interrupted_version, "2.0.0");
        assert_eq!(
            read_live(dir.path(), "a.dat"),
            old_a,
            "an unfinished swap resolves to the version that was working"
        );
        assert_eq!(
            std::fs::read(staging.join("a.dat")).expect("staged"),
            new_a,
            "the new bytes are kept, so finishing the update later is free"
        );
        assert!(
            Journal::load(&service.layout().journal_path())
                .operations
                .is_empty(),
            "recovery must be idempotent, not replay the same swap forever"
        );
        assert!(
            service.recover().expect("second recovery").is_empty(),
            "a resolved swap must not be recovered twice"
        );
    }

    /// The pointer is written last, so a crash after it means the swap had
    /// finished in every respect except being recorded as finished. It is still
    /// reversed: an unfinished swap is not one to trust, and the reversal is the
    /// state the launcher can verify.
    #[test]
    fn a_swap_that_reached_the_pointer_is_still_reversed_and_the_pointer_goes_back() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let (old_a, new_a) = (b"version one, file a", b"version two, file a");
        let staging = service.layout().staging_dir("offline-content", "2.0.0");
        let prior = service.layout().prior_dir("offline-content");
        std::fs::create_dir_all(&staging).expect("staging");
        std::fs::create_dir_all(&prior).expect("prior");
        write_live(dir.path(), "a.dat", new_a);
        std::fs::write(prior.join("a.dat"), old_a).expect("prior file");

        let previous = InstalledVersion {
            version: "1.0.0".to_string(),
            sequence: 7,
        };
        Pointer::new(
            "offline-content",
            InstalledVersion {
                version: "2.0.0".to_string(),
                sequence: 8,
            },
            Some(previous.clone()),
        )
        .save(&platform, &service.layout().pointer_path("offline-content"))
        .expect("pointer");
        Journal {
            schema: JOURNAL_SCHEMA.to_string(),
            operations: vec![Operation {
                component: "offline-content".to_string(),
                version: "2.0.0".to_string(),
                state: OperationState::Swapping,
                live_root: live_root(dir.path()),
                staging,
                prior,
            }],
        }
        .save(&platform, &service.layout().journal_path())
        .expect("journal");

        let recovered = service.recover().expect("recovery runs");
        assert!(recovered[0].pointer_rolled_back);
        assert_eq!(read_live(dir.path(), "a.dat"), old_a);
        let pointer = service
            .installed("offline-content")
            .expect("pointer")
            .expect("installed");
        assert_eq!(pointer.current, previous);
        assert!(
            pointer.previous.is_none(),
            "only one prior version is kept, so there is nothing behind this one"
        );
    }

    // -----------------------------------------------------------------------
    // Invariant 4: a customizable component is never clobbered
    // -----------------------------------------------------------------------

    #[test]
    fn an_update_never_overwrites_a_rebuilt_customizable_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let shipped_image = b"the shipped 2.4.0 image";
        let rebuilt_image = b"the 1.0.0 image with my own soundtrack in it";
        let new_notes = b"patch notes for 2.4.0";
        write_live(dir.path(), "NSR.iso", rebuilt_image);
        write_live(dir.path(), "notes.txt", b"patch notes for 1.0.0");
        install_pointer(&platform, &service, "1.0.0", 7);

        let mut rebuilds = RebuildState::default();
        rebuilds.record("NSR.iso", Sha256Hex::from_bytes(rebuilt_image));

        let bytes = manifest(
            "2.4.0",
            8,
            true,
            vec![
                file_of("NSR.iso", shipped_image),
                file_of("notes.txt", new_notes),
            ],
        );
        let release = signed(&bytes, 12);
        let plan = expect_plan(
            service
                .plan(
                    &release,
                    "offline-content",
                    &bytes,
                    DataDir::LibraryOffline,
                    &rebuilds,
                )
                .expect("plan"),
        );

        assert_eq!(plan.preserved, vec!["NSR.iso"]);
        assert_eq!(
            plan.fetches
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            vec!["notes.txt"],
            "a rebuilt image must not be re-downloaded"
        );

        let source = Blobs::new(Serving::Whole)
            .with("NSR.iso", shipped_image)
            .with("notes.txt", new_notes);
        service.download(&plan, &source).expect("download");
        assert_eq!(
            source.requests(),
            vec![("notes.txt".to_string(), 0)],
            "the customizable file must not even be requested"
        );

        // A staged copy turning up anyway, from a discarded plan or a source
        // that pushed it, must still not be promoted over the user's file.
        let staged_image = service
            .layout()
            .staging_dir("offline-content", "2.4.0")
            .join("NSR.iso");
        std::fs::write(&staged_image, shipped_image).expect("plant a staged image");

        let receipt = match service.promote(&plan).expect("promotion runs") {
            Promoted::Applied(receipt) => receipt,
            other => panic!("expected the update to apply, got {other:?}"),
        };

        assert_eq!(
            read_live(dir.path(), "NSR.iso"),
            rebuilt_image,
            "the user's soundtrack must survive an update"
        );
        assert_eq!(read_live(dir.path(), "notes.txt"), new_notes);
        assert_eq!(receipt.preserved, vec!["NSR.iso"]);
        assert!(
            !service
                .layout()
                .prior_dir("offline-content")
                .join("NSR.iso")
                .exists(),
            "a preserved file is not even moved aside"
        );
    }

    /// The exemption is narrow. The online image is not customizable, which is
    /// what lets matchmaking trust it, so a local edit there is damage and is
    /// replaced.
    #[test]
    fn the_preservation_rule_does_not_leak_into_a_non_customizable_component() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let edited = b"someone edited the online image";
        let shipped = b"the shipped online image";
        write_live(dir.path(), "NSR.iso", edited);
        install_pointer(&platform, &service, "1.0.0", 7);

        let mut rebuilds = RebuildState::default();
        rebuilds.record("NSR.iso", Sha256Hex::from_bytes(edited));

        let bytes = manifest("2.4.0", 8, false, vec![file_of("NSR.iso", shipped)]);
        let release = signed(&bytes, 12);
        let plan = expect_plan(
            service
                .plan(
                    &release,
                    "offline-content",
                    &bytes,
                    DataDir::LibraryOffline,
                    &rebuilds,
                )
                .expect("plan"),
        );
        assert!(plan.preserved.is_empty());
        assert_eq!(plan.fetches[0].reason, FetchReason::Damaged);

        let source = Blobs::new(Serving::Whole).with("NSR.iso", shipped);
        service.download(&plan, &source).expect("download");
        assert!(matches!(
            service.promote(&plan).expect("promotion runs"),
            Promoted::Applied(_)
        ));
        assert_eq!(read_live(dir.path(), "NSR.iso"), shipped);
    }

    // -----------------------------------------------------------------------
    // Trust, replay, and paths
    // -----------------------------------------------------------------------

    #[test]
    fn a_release_that_does_not_verify_cannot_be_opened_and_a_tampered_manifest_cannot_be_planned() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let json =
            br#"{"schema":"nsr.release.v1","version":"2.4.0","sequence":12,"components":{}}"#;
        let err = TrustedRelease::open(&FixedSignature(b"good"), json, b"forged")
            .expect_err("a forged signature must not open a release");
        let message = err.to_string();
        assert!(message.contains("did not verify"), "{message}");
        assert!(
            message.contains("Nothing was downloaded"),
            "the message must say the install is untouched: {message}"
        );

        let bytes = manifest("2.4.0", 8, false, vec![file_of("NSR.iso", b"x")]);
        let release = signed(&bytes, 12);
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] = b' ';
        let err = service
            .plan(
                &release,
                "offline-content",
                &tampered,
                DataDir::LibraryOffline,
                &RebuildState::default(),
            )
            .expect_err("a manifest that is not the pinned one must not be planned");
        assert!(err.to_string().contains("hashes to"), "{err}");
    }

    #[test]
    fn an_older_component_is_refused_while_the_installed_one_is_a_repair() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let current = b"the 2.0.0 file";
        write_live(dir.path(), "a.dat", current);
        install_pointer(&platform, &service, "2.0.0", 8);

        // Genuinely signed, genuinely older. Still refused, because reinstalling
        // it reintroduces whatever 2.0.0 fixed.
        let older = manifest("1.0.0", 7, false, vec![file_of("a.dat", b"the 1.0.0 file")]);
        match service
            .plan(
                &signed(&older, 11),
                "offline-content",
                &older,
                DataDir::LibraryOffline,
                &RebuildState::default(),
            )
            .expect("plan")
        {
            PlanOutcome::Refused(PlanRefusal::Stale { installed, .. }) => {
                assert_eq!(installed.sequence, 8);
            }
            other => panic!("an older sequence must be refused, got {other:?}"),
        }

        // The same sequence with a different version is a manifest reusing a
        // sequence number, which is equally unsafe to install.
        let sideways = manifest("3.0.0", 8, false, vec![file_of("a.dat", b"sideways")]);
        assert!(matches!(
            service
                .plan(
                    &signed(&sideways, 12),
                    "offline-content",
                    &sideways,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("plan"),
            PlanOutcome::Refused(PlanRefusal::Stale { .. })
        ));

        // The installed version with an intact tree has nothing to do.
        let same = manifest("2.0.0", 8, false, vec![file_of("a.dat", current)]);
        assert!(matches!(
            service
                .plan(
                    &signed(&same, 12),
                    "offline-content",
                    &same,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("plan"),
            PlanOutcome::UpToDate { .. }
        ));

        // The installed version with a damaged tree is a repair, not a
        // downgrade. Refusing this would make repair impossible without a
        // version bump.
        std::fs::write(live_root(dir.path()).join("a.dat"), b"bit rot").expect("corrupt");
        let plan = expect_plan(
            service
                .plan(
                    &signed(&same, 12),
                    "offline-content",
                    &same,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("plan"),
        );
        assert!(plan.is_repair());
        assert_eq!(plan.fetches.len(), 1);
    }

    #[test]
    fn a_manifest_path_that_escapes_the_install_root_is_refused_before_anything_is_fetched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let bytes = manifest(
            "2.4.0",
            8,
            false,
            vec![file_of("../../saves/MemoryCardA.USA.raw", b"gotcha")],
        );
        match service
            .plan(
                &signed(&bytes, 12),
                "offline-content",
                &bytes,
                DataDir::LibraryOffline,
                &RebuildState::default(),
            )
            .expect("plan")
        {
            PlanOutcome::Refused(PlanRefusal::UnsafePaths { paths, .. }) => {
                assert_eq!(paths.len(), 1);
            }
            other => panic!("a traversal must be refused, got {other:?}"),
        }
        assert!(
            !dir.path().join(DataDir::Downloads.relative()).exists(),
            "a refused plan must not create so much as a directory"
        );

        // Second line of defence, for a plan that did not come from `plan`.
        for evil in ["../x", "a/../../b", "/etc/passwd", "C:\\Windows\\evil.dll"] {
            assert!(
                resolve_under(dir.path(), evil).is_err(),
                "{evil} must not resolve"
            );
        }
        assert!(resolve_under(dir.path(), "textures/GN7E69/tex.dds").is_ok());
    }

    #[test]
    fn a_manifest_that_installs_somewhere_else_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let bytes = manifest("2.4.0", 8, false, vec![file_of("a.dat", b"x")]);
        match service
            .plan(
                &signed(&bytes, 12),
                "offline-content",
                &bytes,
                // The manifest says library/offline. The caller expected saves,
                // and the mismatch is what stops one component writing into
                // another's directory.
                DataDir::Saves,
                &RebuildState::default(),
            )
            .expect("plan")
        {
            PlanOutcome::Refused(PlanRefusal::WrongInstallRoot {
                found, expected, ..
            }) => {
                assert_eq!(found, "library/offline");
                assert_eq!(expected, "saves");
            }
            other => panic!("expected an install root refusal, got {other:?}"),
        }
    }

    /// A textures manifest rooted one directory below `library/textures`,
    /// which is the only shape the publisher will emit for it.
    fn textures_release(install_root: &str) -> (Vec<u8>, TrustedRelease) {
        let component = Component {
            schema: COMPONENT_SCHEMA.to_string(),
            component: "textures".to_string(),
            version: "1.0.1".to_string(),
            sequence: 3,
            install_root: install_root.to_string(),
            customizable: false,
            files: vec![file_of("tex1_128x128_m_ffc5151a141db330_14.dds", b"dds")],
        };
        let bytes = serde_json::to_vec(&component).expect("encodes");
        let pin = Sha256Hex::from_bytes(&bytes);
        let json = format!(
            r#"{{"schema":"nsr.release.v1","version":"2.4.0","sequence":12,
                 "components":{{"textures":
                   {{"url":"components/textures-1.0.1.json","sha256":"{pin}"}}}}}}"#
        );
        let release = TrustedRelease::open(&FixedSignature(b"good"), json.as_bytes(), b"good")
            .expect("opens");
        (bytes, release)
    }

    #[test]
    fn a_textures_manifest_rooted_at_its_game_id_is_planned_and_any_other_shape_is_refused() {
        // The publisher refuses to root textures anywhere but
        // library/textures/<game id>. Until this test the client demanded
        // exact equality with library/textures, so the two halves could never
        // agree and no textures update was plannable at all.
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let (bytes, release) = textures_release("library/textures/GN7E69");
        match service
            .plan(&release, "textures", &bytes, DataDir::LibraryTextures, &RebuildState::default())
            .expect("plan")
        {
            PlanOutcome::Update(plan) => {
                assert_eq!(plan.fetches.len(), 1, "one absent file to fetch");
                assert_eq!(plan.manifest.install_root, "library/textures/GN7E69");
            }
            other => panic!("expected an update plan, got {other:?}"),
        }

        // The bare data directory is not a textures root: two game ids would
        // share it and overwrite each other.
        for wrong in ["library/textures", "library/textures/GN7E69/sub", "library/textures/"] {
            let (bytes, release) = textures_release(wrong);
            match service
                .plan(&release, "textures", &bytes, DataDir::LibraryTextures, &RebuildState::default())
                .expect("plan")
            {
                PlanOutcome::Refused(PlanRefusal::WrongInstallRoot { found, expected, .. }) => {
                    assert_eq!(found, wrong);
                    assert_eq!(expected, "library/textures/<game id>", "the refusal names the shape");
                }
                other => panic!("{wrong:?}: expected a root refusal, got {other:?}"),
            }
        }

        // And the game-id shape is not accepted for a component that does
        // not take one: offline-content still needs exact equality.
        let bytes = manifest("2.4.0", 8, true, vec![file_of("a.dat", b"x")]);
        let mut wrong: Component = serde_json::from_slice(&bytes).expect("parses");
        wrong.install_root = "library/offline/extra".to_string();
        let wrong_bytes = serde_json::to_vec(&wrong).expect("encodes");
        match service
            .plan(&signed(&wrong_bytes, 12), "offline-content", &wrong_bytes, DataDir::LibraryOffline, &RebuildState::default())
            .expect("plan")
        {
            PlanOutcome::Refused(PlanRefusal::WrongInstallRoot { expected, .. }) => {
                assert_eq!(expected, "library/offline");
            }
            other => panic!("expected a root refusal for offline-content, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Retiring files a new version drops
    // -----------------------------------------------------------------------

    fn install_two_files(service: &ContentService<'_>) {
        let v1 = manifest(
            "1.0.0",
            7,
            false,
            vec![file_of("a.dat", b"a1"), file_of("b.dat", b"b1")],
        );
        let plan = expect_plan(
            service
                .plan(&signed(&v1, 11), "offline-content", &v1, DataDir::LibraryOffline, &RebuildState::default())
                .expect("plan"),
        );
        let source = Blobs::new(Serving::Whole).with("a.dat", b"a1").with("b.dat", b"b1");
        service.download(&plan, &source).expect("download");
        assert!(matches!(service.promote(&plan).expect("promote"), Promoted::Applied(_)));
    }

    fn drop_b(service: &ContentService<'_>) -> Box<UpdatePlan> {
        let v2 = manifest("2.0.0", 8, false, vec![file_of("a.dat", b"a1")]);
        let plan = expect_plan(
            service
                .plan(&signed(&v2, 12), "offline-content", &v2, DataDir::LibraryOffline, &RebuildState::default())
                .expect("plan"),
        );
        assert!(plan.fetches.is_empty(), "a is unchanged, nothing to fetch: {:?}", plan.fetches);
        plan
    }

    #[test]
    fn a_file_the_new_version_drops_is_retired_into_prior_and_rollback_brings_it_back() {
        // Before this, an update could add and replace files but never remove
        // one: a texture deleted from the pack stayed on every player's disk.
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());
        install_two_files(&service);
        assert!(
            service.layout().manifest_path("offline-content").is_file(),
            "the live manifest is cached at promotion, so the next update knows what this one had"
        );
        // Something no manifest ever listed sits beside them.
        write_live(dir.path(), "mine.dat", b"not from any manifest");

        let plan = drop_b(&service);
        let receipt = match service.promote(&plan).expect("promote") {
            Promoted::Applied(r) => r,
            other => panic!("expected an applied swap, got {other:?}"),
        };
        assert_eq!(receipt.retired, vec!["b.dat".to_string()]);
        assert!(!live_root(dir.path()).join("b.dat").exists(), "b is gone from the live tree");
        assert_eq!(
            std::fs::read(service.layout().prior_dir("offline-content").join("b.dat")).expect("retired copy"),
            b"b1",
            "and sits in prior, not deleted"
        );
        assert_eq!(read_live(dir.path(), "a.dat"), b"a1");
        assert_eq!(
            read_live(dir.path(), "mine.dat"),
            b"not from any manifest",
            "a file no manifest ever listed is nobody's to retire"
        );

        match service.rollback("offline-content").expect("rollback") {
            RollbackOutcome::Restored { to, .. } => assert_eq!(to.version, "1.0.0"),
            other => panic!("expected a restore, got {other:?}"),
        }
        assert_eq!(read_live(dir.path(), "b.dat"), b"b1", "rollback brings the retired file back");
        assert!(
            !service.layout().manifest_path("offline-content").exists(),
            "the cache for the version rolled away from is gone, so the next update retires nothing on a stale baseline"
        );
    }

    #[test]
    fn a_swap_that_fails_after_retiring_a_file_puts_it_back() {
        let dir = tempfile::tempdir().expect("tempdir");
        let good = Platform::detect();
        install_two_files(&ContentService::new(&good, dir.path()));

        // The pointer write fails, which is after b has already been moved aside.
        let pointer_path = ContentLayout::new(dir.path()).pointer_path("offline-content");
        let failing = platform_failing_on(&pointer_path);
        let service = ContentService::new(&failing, dir.path());
        let plan = drop_b(&service);
        match service.promote(&plan).expect("promote runs") {
            Promoted::RolledBack { restored, .. } => {
                assert!(restored.iter().any(|p| p.ends_with("b.dat")), "{restored:?}")
            }
            other => panic!("expected a reversed swap, got {other:?}"),
        }
        assert_eq!(read_live(dir.path(), "b.dat"), b"b1", "the retired file is back where it was");
        let pointer = service.installed("offline-content").expect("pointer").expect("installed");
        assert_eq!(pointer.current.version, "1.0.0");
    }

    #[test]
    fn with_no_cached_manifest_nothing_is_retired() {
        // A machine updated by a launcher from before this record exists has a
        // pointer and no cache. Retiring on a guess would be deleting on a
        // guess, so it retires nothing and the update still goes through.
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());
        write_live(dir.path(), "a.dat", b"a1");
        write_live(dir.path(), "b.dat", b"b1");
        install_pointer(&platform, &service, "1.0.0", 7);

        let plan = drop_b(&service);
        let receipt = match service.promote(&plan).expect("promote") {
            Promoted::Applied(r) => r,
            other => panic!("expected an applied swap, got {other:?}"),
        };
        assert!(receipt.retired.is_empty());
        assert_eq!(read_live(dir.path(), "b.dat"), b"b1");
        assert!(
            service.layout().manifest_path("offline-content").is_file(),
            "and from now on the cache exists, so the following update can retire"
        );
    }

    // -----------------------------------------------------------------------
    // Records on disk
    // -----------------------------------------------------------------------

    #[test]
    fn an_unreadable_journal_is_discarded_but_an_unreadable_pointer_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let journal_path = service.layout().journal_path();
        std::fs::write(&journal_path, b"{\"schema\": \"nsr.content-jour").expect("write");
        assert!(
            Journal::load(&journal_path).operations.is_empty(),
            "a torn journal must be discarded, because it can only ever save work"
        );
        std::fs::write(
            &journal_path,
            br#"{"schema":"nsr.content-journal.v99","operations":[]}"#,
        )
        .expect("write");
        assert_eq!(Journal::load(&journal_path).schema, JOURNAL_SCHEMA);

        let pointer_path = service.layout().pointer_path("offline-content");
        ensure_parent(&pointer_path).expect("parent");
        std::fs::write(&pointer_path, b"{\"schema\": \"nsr.point").expect("write");
        let err = Pointer::load(&pointer_path)
            .expect_err("an unreadable pointer must not read as nothing installed");
        assert!(
            err.to_string().contains("Delete the file"),
            "no way forward given: {err}"
        );

        std::fs::write(
            &pointer_path,
            br#"{"schema":"nsr.pointer.v99","component":"offline-content",
                 "current":{"version":"9.0.0","sequence":99},"previous":null}"#,
        )
        .expect("write");
        let err = Pointer::load(&pointer_path).expect_err("an unknown schema must not be guessed");
        assert!(err.to_string().contains("Update the launcher"), "{err}");
    }

    #[test]
    fn the_pointer_round_trips_and_absence_means_nothing_is_installed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());
        let path = service.layout().pointer_path("runtime");

        assert!(
            Pointer::load(&path).expect("read").is_none(),
            "no record means not installed, which is not an error"
        );

        let pointer = Pointer::new(
            "runtime",
            InstalledVersion {
                version: "2.4.0".to_string(),
                sequence: 8,
            },
            Some(InstalledVersion {
                version: "2.3.0".to_string(),
                sequence: 7,
            }),
        );
        pointer.save(&platform, &path).expect("save");
        assert_eq!(Pointer::load(&path).expect("read"), Some(pointer));

        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
        assert_eq!(raw["schema"], POINTER_SCHEMA);
        assert_eq!(raw["current"]["sequence"], 8);
        assert_eq!(
            raw["previous"]["version"], "2.3.0",
            "rollback needs a name for where it goes back to"
        );
    }

    // -----------------------------------------------------------------------
    // Rollback
    // -----------------------------------------------------------------------

    #[test]
    fn rollback_restores_the_one_retained_version_and_then_says_there_is_no_other() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let (old_a, new_a) = (b"version one, file a", b"version two, file a");
        write_live(dir.path(), "a.dat", old_a);
        install_pointer(&platform, &service, "1.0.0", 7);

        let bytes = manifest("2.0.0", 8, false, vec![file_of("a.dat", new_a)]);
        let release = signed(&bytes, 12);
        let plan = expect_plan(
            service
                .plan(
                    &release,
                    "offline-content",
                    &bytes,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("plan"),
        );
        let source = Blobs::new(Serving::Whole).with("a.dat", new_a);
        service.download(&plan, &source).expect("download");
        assert!(matches!(
            service.promote(&plan).expect("promotion"),
            Promoted::Applied(_)
        ));
        assert_eq!(read_live(dir.path(), "a.dat"), new_a);

        match service.rollback("offline-content").expect("rollback runs") {
            RollbackOutcome::Restored {
                to,
                returned_to_staging,
                ..
            } => {
                assert_eq!(to.version, "1.0.0");
                assert_eq!(returned_to_staging.len(), 1);
            }
            other => panic!("expected a restore, got {other:?}"),
        }
        assert_eq!(read_live(dir.path(), "a.dat"), old_a);
        let pointer = service
            .installed("offline-content")
            .expect("pointer")
            .expect("installed");
        assert_eq!(pointer.current.version, "1.0.0");
        assert!(pointer.previous.is_none());
        assert_eq!(
            std::fs::read(
                service
                    .layout()
                    .staging_dir("offline-content", "2.0.0")
                    .join("a.dat")
            )
            .expect("staged"),
            new_a,
            "going forward again must not need a second download"
        );

        match service.rollback("offline-content").expect("rollback runs") {
            RollbackOutcome::Refused(RollbackRefusal::NoPrevious { .. }) => {}
            other => panic!("only one prior version is kept, got {other:?}"),
        }
    }

    /// The disc images are not retained locally, by design, so their rollback is
    /// a download. The refusal has to say that rather than looking like a bug.
    #[test]
    fn a_rollback_with_no_retained_copy_says_it_would_be_a_download() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        Pointer::new(
            "offline-content",
            InstalledVersion {
                version: "2.0.0".to_string(),
                sequence: 8,
            },
            Some(InstalledVersion {
                version: "1.0.0".to_string(),
                sequence: 7,
            }),
        )
        .save(&platform, &service.layout().pointer_path("offline-content"))
        .expect("pointer");

        match service.rollback("offline-content").expect("rollback runs") {
            RollbackOutcome::Refused(RollbackRefusal::NoLocalPrior {
                previous_version, ..
            }) => {
                assert_eq!(previous_version, "1.0.0");
            }
            other => panic!("expected a no-local-copy refusal, got {other:?}"),
        }

        assert!(matches!(
            service.rollback("textures").expect("rollback runs"),
            RollbackOutcome::Refused(RollbackRefusal::NotInstalled { .. })
        ));
    }

    // -----------------------------------------------------------------------
    // Reporting
    // -----------------------------------------------------------------------

    #[test]
    fn the_plan_counts_only_what_it_will_fetch_and_measures_what_it_will_retain() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        let intact = b"unchanged between releases";
        let new_damaged = b"twenty three bytes here";
        let new_absent = b"eleven byte";
        write_live(dir.path(), "same.dat", intact);
        // Seven bytes on disk against 23 in the manifest, so a retained-bytes
        // figure taken from the manifest instead of the disk would be wrong.
        write_live(dir.path(), "changed.dat", b"stale!!");
        install_pointer(&platform, &service, "1.0.0", 7);

        let bytes = manifest(
            "2.0.0",
            8,
            false,
            vec![
                file_of("same.dat", intact),
                file_of("changed.dat", new_damaged),
                file_of("added.dat", new_absent),
            ],
        );
        let plan = expect_plan(
            service
                .plan(
                    &signed(&bytes, 12),
                    "offline-content",
                    &bytes,
                    DataDir::LibraryOffline,
                    &RebuildState::default(),
                )
                .expect("plan"),
        );

        assert_eq!(plan.already_present, vec!["same.dat"]);
        assert_eq!(
            plan.download_bytes(),
            (new_damaged.len() + new_absent.len()) as u64,
            "an unchanged file must not be counted as a download"
        );
        assert_eq!(
            plan.retained_bytes, 7,
            "the retained figure is measured on disk, not taken from the manifest"
        );
        assert_eq!(
            plan.peak_extra_bytes(),
            plan.download_bytes() + 7,
            "the caller needs the real peak to check free space against"
        );
    }

    #[test]
    fn every_refusal_says_what_to_do_about_it() {
        let plan_refusals = [
            PlanRefusal::Stale {
                component: "offline-content".into(),
                installed: InstalledVersion {
                    version: "2.0.0".into(),
                    sequence: 8,
                },
                offered_version: "1.0.0".into(),
                offered_sequence: 7,
            },
            PlanRefusal::UnsafePaths {
                component: "offline-content".into(),
                paths: vec!["../../saves/card.raw".into()],
            },
            PlanRefusal::WrongInstallRoot {
                component: "runtime".into(),
                found: "saves".into(),
                expected: "runtime".into(),
            },
        ];
        let promotion_refusals = [
            PromotionRefusal::StagingIncomplete {
                component: "offline-content".into(),
                missing: vec!["NSR.iso".into()],
                wrong: vec![],
            },
            PromotionRefusal::InUse {
                path: PathBuf::from("NSR.iso"),
            },
            PromotionRefusal::CrossVolume {
                staging: PathBuf::from("D:/staging"),
                live: PathBuf::from("C:/live"),
            },
        ];
        let rollback_refusals = [
            RollbackRefusal::NotInstalled {
                component: "mods".into(),
            },
            RollbackRefusal::NoPrevious {
                component: "runtime".into(),
                current_version: "2.4.0".into(),
            },
            RollbackRefusal::NoLocalPrior {
                component: "offline-content".into(),
                previous_version: "2.3.0".into(),
            },
            RollbackRefusal::UnknownPriorVersion {
                component: "textures".into(),
            },
            RollbackRefusal::InUse {
                path: PathBuf::from("NSR.iso"),
            },
        ];

        let messages = plan_refusals
            .iter()
            .map(PlanRefusal::message)
            .chain(promotion_refusals.iter().map(PromotionRefusal::message))
            .chain(rollback_refusals.iter().map(RollbackRefusal::message));
        for message in messages {
            assert!(message.len() > 40, "too terse to act on: {message}");
            assert!(
                message.trim_end().ends_with('.'),
                "not a sentence: {message}"
            );
            assert!(
                !message.contains('\u{2014}'),
                "no em dashes in user-facing text: {message}"
            );
        }
    }

    #[test]
    fn discarding_staging_leaves_the_live_tree_and_the_prior_copy_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let platform = Platform::detect();
        let service = ContentService::new(&platform, dir.path());

        write_live(dir.path(), "a.dat", b"live bytes");
        let prior = service.layout().prior_dir("offline-content");
        std::fs::create_dir_all(&prior).expect("prior");
        std::fs::write(prior.join("a.dat"), b"prior bytes").expect("prior file");
        let staging = service.layout().staging_dir("offline-content", "2.0.0");
        std::fs::create_dir_all(&staging).expect("staging");
        std::fs::write(staging.join("a.dat"), b"staged bytes").expect("staged file");

        service
            .discard_staging("offline-content", "2.0.0")
            .expect("discard");

        assert!(!staging.exists());
        assert_eq!(read_live(dir.path(), "a.dat"), b"live bytes");
        assert_eq!(
            std::fs::read(prior.join("a.dat")).expect("prior survives"),
            b"prior bytes",
            "discarding derived bytes must not touch the rollback copy"
        );
    }
}
