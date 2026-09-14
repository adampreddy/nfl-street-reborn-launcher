//! Generating a manifest from what is actually on disk.
//!
//! # Why this exists
//!
//! [`crate::manifest`] defines the schemas and [`crate::library`] verifies and
//! repairs against them, but nothing in the product produced a manifest, so
//! neither verify nor repair had ever run against real content. This module is
//! the missing half. It walks a directory, hashes every file, and returns the
//! three objects the rest of the pipeline consumes:
//!
//! - the [`Component`] that [`crate::library::Verifier`] checks a tree against,
//! - the [`Release`] that pins it by hash, as [`UnsignedRelease`],
//! - the [`RebuildState`] that tells a user's own soundtrack apart from
//!   corruption, via [`record_rebuild`].
//!
//! # Nothing here signs anything, and the type says so
//!
//! The release object is the only signed thing in the product, and the key is
//! not available to this work. So this module produces [`UnsignedRelease`],
//! which is named for what it is, exposes the exact bytes a signer must cover,
//! and stops. There is deliberately no way to get a
//! [`crate::content::TrustedRelease`] out of this module: that type can only be
//! opened with a signature that verified over the served bytes, and keeping
//! that the only door is what stops a call site from looking checked while
//! nothing is checked.
//!
//! # The two things this module is careful about
//!
//! **`customizable` is derived from the component identity, never passed in.**
//! [`ComponentId::is_customizable`] is the only source of it, so a generated
//! manifest cannot get it wrong by omission or by a mistyped argument. Getting
//! it wrong is not cosmetic in either direction: `true` on `runtime` makes
//! repair skip a tampered emulator, and `false` on `offline-content` makes
//! repair re-fetch 1.5 GB over the user's own music. [`crate::manifest`]
//! deliberately gives the field no serde default so that a manifest omitting it
//! fails to parse, and [`SerializedComponent::new`] re-parses its own output to
//! prove the field survived serialization.
//!
//! **Hashing is the expensive part and it is interruptible.** The texture
//! component on this machine is 27,025 files, and the offline image is a single
//! 1,459,978,240 byte file. A scan therefore reports [`ScanProgress`] and honors
//! [`ScanControl::Cancel`] both between files and every megabyte *within* a
//! file. Per-file cancellation alone would be useless for a disc image, which is
//! one file and the entire wait.
//!
//! A cancelled scan returns [`ScanOutcome::Cancelled`] and **no**
//! [`Component`]. That is the point of the enum: a truncated file list is not a
//! partial manifest, it is a manifest that silently stops managing every file it
//! did not reach, and repair never notices those files are gone.
//!
//! # What the caller has to get right
//!
//! The generator lists every file it finds and filters nothing. It has no way to
//! know that the 19 PNG files sitting in the live texture tree are an applied
//! mod rather than content, which is the case
//! `docs/architecture/CONTENT_AND_UPDATE_MODEL.md` section 2A records. Scan a
//! pristine tree. A generator that quietly dropped files would be deciding
//! content policy in the wrong place, and a dropped file is a file repair will
//! never restore.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::library::RebuildState;
use crate::manifest::{
    Component, ManifestFile, PinnedComponent, Release, Sha256Hex, COMPONENT_SCHEMA, RELEASE_SCHEMA,
};
use crate::platform::{DataDir, Error, Result};

/// Read size, and therefore the cancellation granularity inside one file.
///
/// Matches the buffer in [`crate::library::hash_file`], so a change to one is
/// visible next to the other. One megabyte means a 1,459,978,240 byte image
/// offers about 1,392 chances to cancel instead of one.
const READ_CHUNK: usize = 1 << 20;

/// A refusal reported as an error because there is no safe way to continue.
///
/// Mirrors the helper in [`crate::content`]. Both exist because
/// [`crate::platform::Error`] is a shared type that should not grow a variant
/// per module, and a refusal needs to carry a sentence rather than a code.
fn refuse(message: impl Into<String>) -> Error {
    Error::PlainIo(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.into(),
    ))
}

/// Join a manifest-relative path onto a root, refusing anything that escapes.
///
/// Duplicated from [`crate::content`] rather than shared, because that copy is
/// private and this change is scoped to one file. Consolidating the two is a
/// call for whoever owns the module boundaries.
fn resolve_under(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.starts_with('/') || relative.starts_with('\\') || relative.contains(':') {
        return Err(refuse(format!(
            "refusing {relative:?}: a manifest path must be relative to the \
             component install root, and this one is not"
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
                 root, which is how a repair writes over saves"
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

// ---------------------------------------------------------------------------
// Component identity
// ---------------------------------------------------------------------------

/// One of the seven content components.
///
/// A closed enum rather than a string, because `customizable` and the install
/// root are both derived from the identity here. With a string, a typo would
/// produce a manifest that parses and verifies while exempting the wrong tree
/// from repair, and nothing would report it.
///
/// The list is settled at seven in
/// `docs/architecture/CONTENT_AND_UPDATE_MODEL.md` section 3. Two other
/// documents say eight; the eighth was news and configuration, which is not a
/// content component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentId {
    Launcher,
    Runtime,
    OfflineContent,
    OnlineContent,
    Textures,
    Mods,
    ControllerDb,
    /// The roster the site published: one `.rebornroster` file, applied to
    /// the live image on the player's machine by the Reborn Builder. Small,
    /// frequent, and the reason a roster change does not cost a 1.4 GB
    /// image download.
    Roster,
    /// Game updates as a chain of xdeltas between consecutive Reborn builds,
    /// applied to the player's pristine image. About a megabyte per build,
    /// against 1.46 GB for the image itself.
    GamePatch,
    /// The complete head-build disc, 1,459,978,240 bytes. Recovery only, for
    /// a player whose own copy cannot be converted. It is pinned and carried
    /// forward like every other component, so recovery does not expire; what
    /// keeps a normal install from ever fetching it is
    /// [`crate::content::pipeline::UPDATABLE`], which does not list it, so
    /// only the recovery command asks for it and only on a button press.
    GameFull,
}

impl ComponentId {
    /// Every component, for iteration and for tests.
    pub const ALL: &'static [ComponentId] = &[
        ComponentId::Launcher,
        ComponentId::Runtime,
        ComponentId::OfflineContent,
        ComponentId::OnlineContent,
        ComponentId::Textures,
        ComponentId::Mods,
        ComponentId::ControllerDb,
        ComponentId::Roster,
        ComponentId::GamePatch,
        ComponentId::GameFull,
    ];

    /// The name that goes in the manifest and in the release's component map.
    pub fn as_str(self) -> &'static str {
        match self {
            ComponentId::Launcher => "launcher",
            ComponentId::Runtime => "runtime",
            ComponentId::OfflineContent => "offline-content",
            ComponentId::OnlineContent => "online-content",
            ComponentId::Textures => "textures",
            ComponentId::Mods => "mods",
            ComponentId::ControllerDb => "controller-db",
            ComponentId::Roster => "roster",
            ComponentId::GamePatch => "game-patch",
            ComponentId::GameFull => "game-full",
        }
    }

    /// Recognise a wire name. `None` for anything not a component.
    ///
    /// An option rather than a default, because a caller holding an unknown
    /// name has a bug or a tampered file, and picking a component for it would
    /// decide where files land on disk.
    pub fn parse(name: &str) -> Option<Self> {
        ComponentId::ALL
            .iter()
            .copied()
            .find(|id| id.as_str() == name)
    }

    /// Whether the user is allowed to modify these files locally.
    ///
    /// True for `offline-content` and nothing else, per the `nsr.component.v1`
    /// ruling in `docs/architecture/RECONCILIATION_RULINGS.md`. Music is
    /// offline only, so the online image is always pristine, and that is
    /// precisely what lets matchmaking trust it.
    ///
    /// A property of the identity rather than an argument to the generator, so
    /// that it cannot be supplied wrongly. A `runtime` manifest marked
    /// customizable makes repair skip a tampered emulator; an `offline-content`
    /// manifest marked not customizable makes repair treat every user's rebuilt
    /// soundtrack as corruption and download over it.
    pub fn is_customizable(self) -> bool {
        matches!(self, ComponentId::OfflineContent)
    }

    /// The install root, relative to the data root, when it follows from the
    /// component identity alone.
    ///
    /// `None` for the three where it genuinely does not, and inventing one for
    /// them is how a manifest ends up writing somewhere nobody chose:
    ///
    /// - `textures` lives in `library/textures/<game id>`, and the game id
    ///   (`GN7E69` offline, `FR4N31` online) is not derivable from the component
    ///   name. Use [`ComponentSpec::with_install_root`].
    /// - `launcher` installs into the install root, not the data root, and that
    ///   layout is not settled.
    /// - `controller-db` has no artifact yet, so it has no root.
    pub fn install_root(self) -> Option<&'static str> {
        match self {
            ComponentId::Runtime => Some(DataDir::Runtime.relative()),
            ComponentId::OfflineContent => Some(DataDir::LibraryOffline.relative()),
            ComponentId::OnlineContent => Some(DataDir::LibraryOnline.relative()),
            ComponentId::Mods => Some(DataDir::ModsPackages.relative()),
            ComponentId::Roster => Some(DataDir::LibraryRoster.relative()),
            ComponentId::GamePatch => Some(DataDir::LibraryGamePatch.relative()),
            // NOT `LibraryOffline`. That directory holds the BOOTED image
            // (`crate::game_image::booted_path`), which a roster apply
            // rewrites in place, and it is not what this component's
            // manifest describes: a downloaded disc is a PRISTINE image
            // (`crate::game_image` module doc), and pristine images live at
            // `library/imported/<sha256>`, a path this method cannot express
            // because it takes no version or hash. `library/game-full` is a
            // neutral landing spot that collides with neither layer; turning
            // a landed download into a pristine image is a separate, later
            // step, not this component's install root.
            ComponentId::GameFull => Some(DataDir::LibraryGameFull.relative()),
            ComponentId::Textures | ComponentId::Launcher | ComponentId::ControllerDb => None,
        }
    }
}

/// What to generate, settled before any file is read.
///
/// Separate from the scan so that every argument deciding where files land is
/// validated up front. A 27,025 file scan that fails at the end on a bad install
/// root has spent minutes to learn something knowable in microseconds.
#[derive(Debug, Clone)]
pub struct ComponentSpec {
    id: ComponentId,
    version: String,
    sequence: u64,
    install_root: String,
}

impl ComponentSpec {
    /// For a component whose install root follows from its identity.
    ///
    /// Refuses the three where it does not, naming the constructor to use
    /// instead rather than guessing a root.
    pub fn new(id: ComponentId, version: &str, sequence: u64) -> Result<Self> {
        let Some(root) = id.install_root() else {
            return Err(refuse(format!(
                "the install root of {:?} does not follow from its name: \
                 textures is rooted at {}/<game id>, the launcher installs into \
                 the install root rather than the data root, and controller-db \
                 has no artifact yet. Call ComponentSpec::with_install_root and \
                 state the root",
                id.as_str(),
                DataDir::LibraryTextures.relative()
            )));
        };
        Self::with_install_root(id, version, sequence, root)
    }

    /// For a component whose install root has to be stated, chiefly `textures`.
    ///
    /// The root is checked against the identity, so a `runtime` manifest cannot
    /// be pointed at `saves`. [`Component::check_install_root`] performs the
    /// same check on the client, and a generator able to emit a root the client
    /// will refuse is a generator that produces unusable releases.
    pub fn with_install_root(
        id: ComponentId,
        version: &str,
        sequence: u64,
        install_root: &str,
    ) -> Result<Self> {
        check_version(version)?;
        check_sequence(sequence)?;
        check_install_root(id, install_root)?;
        Ok(Self {
            id,
            version: version.to_string(),
            sequence,
            install_root: install_root.to_string(),
        })
    }

    pub fn id(&self) -> ComponentId {
        self.id
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn install_root(&self) -> &str {
        &self.install_root
    }
}

/// A version string is also a directory name, so it cannot contain a separator.
///
/// [`crate::content::ContentLayout::downloads_dir`] and `staging_dir` join the
/// version straight onto a path. A version of `../../saves` would therefore
/// stage a download on top of somebody's memory cards.
fn check_version(version: &str) -> Result<()> {
    if version.trim().is_empty() {
        return Err(refuse(
            "a component version cannot be empty. It is what the user sees and \
             what names the download and staging directories, so there is no \
             value to invent for it",
        ));
    }
    if version != version.trim() {
        return Err(refuse(format!(
            "refusing the version {version:?}: it has leading or trailing \
             whitespace, which would silently become part of a directory name"
        )));
    }
    if version.contains('/')
        || version.contains('\\')
        || version.contains(':')
        || version.split(['/', '\\']).any(|s| s == "..")
    {
        return Err(refuse(format!(
            "refusing the version {version:?}: ContentLayout::downloads_dir and \
             staging_dir join the version onto a path, so a separator or a drive \
             letter in it can move a download outside the data root"
        )));
    }
    Ok(())
}

/// Sequence 0 can never be accepted, so generating it produces dead content.
///
/// [`Release::check_sequence`] requires strictly newer than the installed
/// sequence, and an install with no record counts as 0. So a release published
/// at sequence 0 is refused by every client, forever, including a completely
/// fresh one. Component sequences are compared the same way in
/// [`crate::content`], and letting the two rules differ here would be a trap.
fn check_sequence(sequence: u64) -> Result<()> {
    if sequence == 0 {
        return Err(refuse(
            "refusing sequence 0: Release::check_sequence accepts only a \
             sequence strictly newer than the installed one, and an install with \
             no record counts as 0, so nothing at sequence 0 is ever \
             installable. Start at 1",
        ));
    }
    Ok(())
}

fn check_install_root(id: ComponentId, install_root: &str) -> Result<()> {
    if install_root.is_empty() {
        return Err(refuse(
            "an install root cannot be empty: it is the field that decides where \
             the component's files land on disk",
        ));
    }
    if install_root != install_root.trim() {
        return Err(refuse(format!(
            "refusing the install root {install_root:?}: leading or trailing \
             whitespace would become part of a directory name"
        )));
    }
    if install_root.starts_with('/') || install_root.starts_with('\\') {
        return Err(refuse(format!(
            "refusing the install root {install_root:?}: it must be relative to \
             the data root, and an absolute path escapes it"
        )));
    }
    if install_root.contains('\\') {
        return Err(refuse(format!(
            "refusing the install root {install_root:?}: manifests are read on \
             every platform, so the separator is always a forward slash. See \
             DataDir::relative"
        )));
    }
    if install_root.contains(':') {
        return Err(refuse(format!(
            "refusing the install root {install_root:?}: a drive letter in a \
             manifest is not a mistake worth tolerating"
        )));
    }
    if install_root
        .split('/')
        .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(refuse(format!(
            "refusing the install root {install_root:?}: an empty, `.` or `..` \
             segment either climbs out of the data root or names nothing"
        )));
    }

    if let Some(known) = id.install_root() {
        if install_root != known {
            return Err(refuse(format!(
                "refusing to root {:?} at {install_root:?}: it installs at \
                 {known:?}. This field decides where files land, and \
                 Component::check_install_root makes the client refuse a \
                 mismatch anyway",
                id.as_str()
            )));
        }
        return Ok(());
    }

    if id == ComponentId::Textures {
        let prefix = DataDir::LibraryTextures.relative();
        let game_id = install_root
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or("");
        if game_id.is_empty() || game_id.contains('/') {
            return Err(refuse(format!(
                "refusing the textures install root {install_root:?}: it must be \
                 {prefix}/<game id>, for example {prefix}/GN7E69. The game id \
                 directory is not optional: 2,252 of FR4N31's texture files are \
                 byte-identical to GN7E69's, so two game ids sharing one root \
                 would overwrite each other mostly invisibly"
            )));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Progress and cancellation
// ---------------------------------------------------------------------------

/// Which half of a scan is running.
///
/// The two halves have different costs and only one of them has a denominator.
/// Enumerating 27,025 files is thousands of metadata calls with no idea how many
/// are left; hashing them is minutes with an exact total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanPhase {
    /// Walking the tree. The totals are not known yet.
    Enumerating,
    /// Reading and hashing. The totals are final.
    Hashing,
}

/// How far a scan has got.
///
/// The totals are [`Option`] on purpose. While the walk is still running there
/// is no denominator, and a percentage computed from a guessed total produces a
/// bar that goes backwards, which is the launcher showing the user a number it
/// did not measure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanProgress {
    pub phase: ScanPhase,
    /// Files the walk has found. Final once `phase` is [`ScanPhase::Hashing`].
    pub files_found: u64,
    /// Files hashed all the way through.
    pub files_hashed: u64,
    /// Bytes hashed, including the part already read of the file in progress.
    pub bytes_hashed: u64,
    /// Total bytes to hash. Absent until the walk has finished.
    pub bytes_total: Option<u64>,
    /// The file being read, relative to the scan root, when one is.
    pub current: Option<String>,
}

impl ScanProgress {
    fn new() -> Self {
        Self {
            phase: ScanPhase::Enumerating,
            files_found: 0,
            files_hashed: 0,
            bytes_hashed: 0,
            bytes_total: None,
            current: None,
        }
    }

    /// The number of files to hash, once that is known.
    pub fn files_total(&self) -> Option<u64> {
        match self.phase {
            ScanPhase::Enumerating => None,
            ScanPhase::Hashing => Some(self.files_found),
        }
    }

    /// Fraction of the work done, from 0.0 to 1.0, when it is knowable.
    ///
    /// Absent while enumerating, and absent for a tree whose every file is zero
    /// bytes, because there is no denominator in either case. A caller with no
    /// fraction should show activity rather than a number it invented.
    pub fn fraction(&self) -> Option<f32> {
        let total = self.bytes_total?;
        if total == 0 {
            return None;
        }
        Some((self.bytes_hashed as f64 / total as f64).min(1.0) as f32)
    }
}

/// Whether the scan should carry on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum ScanControl {
    Continue,
    Cancel,
}

impl ScanControl {
    pub fn is_cancel(self) -> bool {
        self == ScanControl::Cancel
    }
}

/// Where progress goes, and where a cancel comes from.
///
/// One trait for both directions because they happen at the same instants. A
/// separate cancel token would be polled on a different schedule from the
/// progress it is meant to interrupt, and the schedule is the whole point.
///
/// Called once per file found during the walk, once per megabyte read, and once
/// per file completed: roughly 55,000 calls for the texture component. An
/// implementation that redraws a window every call should throttle. Throttling
/// is the sink's business rather than the scanner's, because only the sink knows
/// what it costs to draw.
pub trait ScanObserver: std::fmt::Debug {
    fn progress(&self, progress: &ScanProgress) -> ScanControl;
}

/// An observer that reports nothing and never cancels.
///
/// For a command line tool or a test: somewhere with no place to draw progress
/// and nobody to press cancel.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunToCompletion;

impl ScanObserver for RunToCompletion {
    fn progress(&self, _progress: &ScanProgress) -> ScanControl {
        ScanControl::Continue
    }
}

/// A cancel that can be set from another thread, with no progress reporting.
///
/// Provided because the hand-rolled version is a `bool` behind a mutex the UI
/// thread forgets to lock, or an `AtomicBool` with an ordering chosen by guess.
/// A scan runs on a worker and the cancel arrives from the UI, so this is the
/// ordinary case rather than an exotic one.
#[derive(Debug, Clone, Default)]
pub struct CancelSignal(Arc<AtomicBool>);

impl CancelSignal {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Ask the scan to stop. Safe from any thread, and safe to call twice.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl ScanObserver for CancelSignal {
    fn progress(&self, _progress: &ScanProgress) -> ScanControl {
        if self.is_cancelled() {
            ScanControl::Cancel
        } else {
            ScanControl::Continue
        }
    }
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

/// A finished scan.
#[derive(Debug, Clone)]
pub struct Scan {
    /// The generated manifest, ready to serialize.
    pub component: Component,

    /// Directories under the root that hold no file at any depth.
    ///
    /// `nsr.component.v1` has no row type for a directory, so these do not
    /// survive a fetch and install. Reported rather than dropped, because a tree
    /// that needs an empty directory to exist would otherwise fail only at
    /// runtime, on the user's machine, with nothing pointing at the cause.
    pub empty_dirs: Vec<String>,

    /// The final counts, for a caller that wants to report what it did.
    pub progress: ScanProgress,
}

/// The result of a scan that was allowed to be interrupted.
#[derive(Debug, Clone)]
pub enum ScanOutcome {
    Complete(Scan),

    /// The observer asked to stop. There is no [`Component`] here on purpose.
    ///
    /// A manifest built from a partial file list is worse than no manifest: it
    /// parses, it verifies clean, and every file the walk did not reach is
    /// silently unmanaged, so repair never notices when one of them goes
    /// missing.
    Cancelled {
        progress: ScanProgress,
    },
}

impl ScanOutcome {
    /// The scan, if it finished. `None` after a cancel.
    pub fn completed(&self) -> Option<&Scan> {
        match self {
            ScanOutcome::Complete(scan) => Some(scan),
            ScanOutcome::Cancelled { .. } => None,
        }
    }
}

/// One file the walk found.
#[derive(Debug)]
struct Found {
    relative: String,
    path: PathBuf,
    /// Length as the walk saw it. Compared against the length actually read.
    enumerated_bytes: u64,
}

/// Walk `root`, hash every file, and build the component manifest.
///
/// The tree must be quiescent. Every file is stat'd during the walk and read
/// during the hash, and a length that changed between the two is refused rather
/// than recorded: a manifest describing a state the tree was never in fails its
/// own verification on a pristine install.
pub fn scan_component(
    root: &Path,
    spec: &ComponentSpec,
    observer: &dyn ScanObserver,
) -> Result<ScanOutcome> {
    let meta = std::fs::metadata(root).map_err(|source| Error::Io {
        path: root.to_path_buf(),
        source,
    })?;
    if !meta.is_dir() {
        return Err(refuse(format!(
            "refusing to scan {}: it is not a directory, and a component \
             manifest describes a tree",
            root.display()
        )));
    }

    let mut progress = ScanProgress::new();

    let (found, empty_dirs) = match enumerate(root, &mut progress, observer)? {
        Enumeration::Complete { files, empty_dirs } => (files, empty_dirs),
        Enumeration::Cancelled => return Ok(ScanOutcome::Cancelled { progress }),
    };

    if found.is_empty() {
        return Err(refuse(format!(
            "refusing to generate a manifest for {}: it contains no files. An \
             empty file list verifies clean against an empty install, so \
             publishing one would make a component that never arrived look \
             healthy",
            root.display()
        )));
    }

    refuse_case_collisions(found.iter().map(|f| f.relative.as_str()))?;

    progress.phase = ScanPhase::Hashing;
    progress.bytes_total = Some(found.iter().map(|f| f.enumerated_bytes).sum());

    let mut buf = vec![0u8; READ_CHUNK];
    let mut files = Vec::with_capacity(found.len());

    for entry in &found {
        progress.current = Some(entry.relative.clone());
        match hash_with_progress(&entry.path, &mut buf, &mut progress, observer)? {
            Hashed::Cancelled => return Ok(ScanOutcome::Cancelled { progress }),
            Hashed::Done { sha256, bytes } => {
                if bytes != entry.enumerated_bytes {
                    return Err(refuse(format!(
                        "refusing to generate a manifest: {} was {} bytes when \
                         the tree was walked and {} bytes when it was read. \
                         Something is writing to the tree, and a manifest \
                         recorded from a moving tree describes a state that \
                         never existed, so it fails its own verification. \
                         Re-run with nothing writing to {}",
                        entry.relative,
                        entry.enumerated_bytes,
                        bytes,
                        root.display()
                    )));
                }
                files.push(ManifestFile {
                    path: entry.relative.clone(),
                    // The length actually read, never the size on disk. The
                    // shared football texture is 1,398,276 bytes long and
                    // Explorer reports 1,400,832 for it, which is 342 x 4,096 of
                    // NTFS allocation. A manifest carrying the allocated size
                    // fails its own presence-and-size check on a pristine
                    // install, on every platform, forever.
                    bytes,
                    sha256,
                });
            }
        }
        progress.files_hashed += 1;
        if observer.progress(&progress).is_cancel() {
            return Ok(ScanOutcome::Cancelled { progress });
        }
    }

    progress.current = None;

    let component = Component {
        schema: COMPONENT_SCHEMA.to_string(),
        component: spec.id.as_str().to_string(),
        version: spec.version.clone(),
        sequence: spec.sequence,
        install_root: spec.install_root.clone(),
        // Derived, never passed. See ComponentId::is_customizable.
        customizable: spec.id.is_customizable(),
        files,
    };

    let offenders = component.traversal_offenders();
    if !offenders.is_empty() {
        return Err(refuse(format!(
            "refusing the generated manifest for {}: {} of its paths escape the \
             install root, starting with {:?}. That should be impossible from a \
             walk, so the walk or the tree is not what it appears to be",
            spec.id.as_str(),
            offenders.len(),
            offenders
        )));
    }

    Ok(ScanOutcome::Complete(Scan {
        component,
        empty_dirs,
        progress,
    }))
}

enum Enumeration {
    Complete {
        files: Vec<Found>,
        empty_dirs: Vec<String>,
    },
    Cancelled,
}

/// Walk the tree, collecting files, empty directories, and reasons to refuse.
///
/// Iterative rather than recursive so a pathologically deep tree cannot exhaust
/// the stack, and sorted at the end so the same tree produces the same manifest
/// bytes on every machine. That matters more than tidiness: the release pins the
/// manifest by the hash of its bytes, so a manifest whose file order depended on
/// the order the filesystem happened to return would produce a different pin per
/// build machine.
fn enumerate(
    root: &Path,
    progress: &mut ScanProgress,
    observer: &dyn ScanObserver,
) -> Result<Enumeration> {
    let mut files: Vec<Found> = Vec::new();
    let mut dirs: Vec<String> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    let mut oddities: Vec<String> = Vec::new();

    let mut stack: Vec<(PathBuf, String)> = vec![(root.to_path_buf(), String::new())];

    while let Some((dir, prefix)) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|source| Error::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io {
                path: dir.clone(),
                source,
            })?;
            let name_os = entry.file_name();
            let Some(name) = name_os.to_str() else {
                return Err(refuse(format!(
                    "refusing to scan {}: the name {:?} is not valid UTF-8, and \
                     a manifest path is JSON text. Converting it lossily would \
                     put a row in the manifest naming a file that does not \
                     exist, so repair would report it missing and re-fetch it \
                     forever. Rename it",
                    dir.display(),
                    name_os.to_string_lossy()
                )));
            };

            let relative = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            let path = entry.path();

            // symlink_metadata, not metadata: a link has to be seen as a link.
            let meta = std::fs::symlink_metadata(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            let kind = meta.file_type();

            if kind.is_symlink() {
                links.push(relative);
                continue;
            }
            if kind.is_dir() {
                dirs.push(relative.clone());
                stack.push((path, relative));
                continue;
            }
            if !kind.is_file() {
                oddities.push(relative);
                continue;
            }

            files.push(Found {
                relative,
                path,
                enumerated_bytes: meta.len(),
            });
            progress.files_found += 1;
            if observer.progress(progress).is_cancel() {
                return Ok(Enumeration::Cancelled);
            }
        }
    }

    if !links.is_empty() {
        links.sort();
        return Err(refuse_links(root, &links));
    }
    if !oddities.is_empty() {
        oddities.sort();
        return Err(refuse(format!(
            "refusing to scan {}: {} entries are neither a regular file nor a \
             directory, starting with {:?}. A manifest row names a file with a \
             length and a hash, and these have neither in any stable sense",
            root.display(),
            oddities.len(),
            &oddities[..oddities.len().min(3)]
        )));
    }

    files.sort_by(|a, b| a.relative.cmp(&b.relative));
    dirs.sort();

    let mut ancestors: BTreeSet<&str> = BTreeSet::new();
    for f in &files {
        let mut rest = f.relative.as_str();
        while let Some(cut) = rest.rfind('/') {
            rest = &rest[..cut];
            if !ancestors.insert(rest) {
                break;
            }
        }
    }
    let empty_dirs: Vec<String> = dirs
        .into_iter()
        .filter(|d| !ancestors.contains(d.as_str()))
        .collect();

    Ok(Enumeration::Complete { files, empty_dirs })
}

/// Extracted so the message is testable without the privilege needed to create
/// a symbolic link, and so it says what to do rather than only what is wrong.
fn refuse_links(root: &Path, links: &[String]) -> Error {
    refuse(format!(
        "refusing to scan {}: {} entries are symbolic links or directory \
         junctions, starting with {:?}. A link would be hashed as whatever it \
         points at, so the manifest would describe content that is not in the \
         tree; a link pointing outside the tree would publish whatever is there; \
         and a link forming a cycle would make the walk run forever. Replace the \
         link with the real file, or scan the directory it points to",
        root.display(),
        links.len(),
        &links[..links.len().min(3)]
    ))
}

/// Refuse two paths that differ only in case.
///
/// Legal on Linux, impossible on Windows and on a default macOS volume. A
/// manifest generated on Linux carrying both `A.dds` and `a.dds` describes two
/// files that are one file on the user's machine, so one row can never be
/// satisfied: repair reports it damaged, re-fetches it, overwrites the other,
/// and reports that one damaged instead, without end.
fn refuse_case_collisions<'a>(paths: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    let mut collisions: Vec<(String, String)> = Vec::new();
    for path in paths {
        let folded = path.to_lowercase();
        match seen.get(&folded) {
            Some(first) => collisions.push(((*first).to_string(), path.to_string())),
            None => {
                seen.insert(folded, path);
            }
        }
    }
    if collisions.is_empty() {
        return Ok(());
    }
    Err(refuse(format!(
        "refusing to generate a manifest: {} pairs of paths differ only in case, \
         starting with {:?}. On Windows and on a default macOS volume those are \
         one file, so one of the two rows can never be satisfied and repair \
         re-fetches them over each other without end",
        collisions.len(),
        &collisions[..collisions.len().min(3)]
    )))
}

enum Hashed {
    Done { sha256: Sha256Hex, bytes: u64 },
    Cancelled,
}

/// Stream a file into SHA-256, reporting progress and offering a cancel every
/// chunk.
///
/// Not [`crate::library::hash_file`], for two reasons that both bite at this
/// scale: that function has no cancellation point, so a 1.5 GB image is
/// uninterruptible, and it allocates a fresh megabyte per call, which is 27,025
/// allocations for the texture component. The digest it computes is identical,
/// and a test in this module holds the two to that. If they ever disagreed,
/// every file in the library would verify as damaged and repair would re-fetch
/// all of it.
fn hash_with_progress(
    path: &Path,
    buf: &mut [u8],
    progress: &mut ScanProgress,
    observer: &dyn ScanObserver,
) -> Result<Hashed> {
    use sha2::{Digest, Sha256};

    let mut file = std::fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut read: u64 = 0;

    loop {
        let n = file.read(buf).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        read += n as u64;
        progress.bytes_hashed += n as u64;
        if observer.progress(progress).is_cancel() {
            return Ok(Hashed::Cancelled);
        }
    }

    let encoded = hex::encode(hasher.finalize());
    let sha256 = Sha256Hex::parse(&encoded).map_err(|e| {
        refuse(format!(
            "the hex encoding of a sha-256 digest did not parse as one ({e}). \
             This cannot happen unless the hashing dependency changed shape, so \
             report it rather than working around it"
        ))
    })?;
    Ok(Hashed::Done {
        sha256,
        bytes: read,
    })
}

// ---------------------------------------------------------------------------
// Serializing, and pinning
// ---------------------------------------------------------------------------

/// A component manifest together with the exact bytes it serialized to and the
/// hash of those bytes.
///
/// The three travel together because the release pins the manifest by the hash
/// of the bytes *as served*. Serializing once for the pin and again for the file
/// is how a release comes to pin a hash no published file has, and then every
/// client refuses the component with a hash mismatch that looks like tampering.
/// So the bytes are produced once, here, and
/// [`SerializedComponent::write_to`] writes those same bytes.
#[derive(Debug, Clone)]
pub struct SerializedComponent {
    component: Component,
    bytes: Vec<u8>,
    sha256: Sha256Hex,
}

impl SerializedComponent {
    /// Serialize a component, and prove the result is usable before returning.
    ///
    /// The output is parsed back with [`Component::parse`] and the mandatory
    /// `customizable` field is compared, because that field has no serde
    /// default: were it ever skipped on the way out, the manifest would fail to
    /// parse on every client, and the failure would surface as a broken update
    /// rather than as a broken generator.
    pub fn new(component: Component) -> Result<Self> {
        if component.schema != COMPONENT_SCHEMA {
            return Err(refuse(format!(
                "refusing to serialize a component manifest whose schema is \
                 {:?}: it must be {COMPONENT_SCHEMA}, or Component::parse \
                 refuses it on the client",
                component.schema
            )));
        }
        let offenders = component.traversal_offenders();
        if !offenders.is_empty() {
            return Err(refuse(format!(
                "refusing to serialize the manifest for {:?}: {} of its paths \
                 escape the install root, starting with {:?}",
                component.component,
                offenders.len(),
                offenders
            )));
        }

        let bytes = serde_json::to_vec_pretty(&component).map_err(|e| {
            refuse(format!(
                "the component manifest for {:?} could not be serialized: {e}",
                component.component
            ))
        })?;

        let parsed = Component::parse(&bytes).map_err(|e| {
            refuse(format!(
                "the manifest this generator produced for {:?} does not parse \
                 back ({e}). Publishing it would pin a manifest no client can \
                 read",
                component.component
            ))
        })?;
        if parsed.customizable != component.customizable {
            return Err(refuse(format!(
                "the mandatory customizable field did not survive serialization \
                 for {:?}: generated {}, parsed back {}. That field is what \
                 stops repair deleting a user's music",
                component.component, component.customizable, parsed.customizable
            )));
        }

        let sha256 = Sha256Hex::from_bytes(&bytes);
        Ok(Self {
            component,
            bytes,
            sha256,
        })
    }

    pub fn component(&self) -> &Component {
        &self.component
    }

    /// The bytes to publish. These, and not a re-serialization of them, are what
    /// [`SerializedComponent::sha256`] covers.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn sha256(&self) -> &Sha256Hex {
        &self.sha256
    }

    /// Write the pinned bytes to disk, creating parent directories.
    pub fn write_to(&self, path: &Path) -> Result<()> {
        write_exact(path, &self.bytes)
    }
}

fn write_exact(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, bytes).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// A release object that has not been signed, and cannot pretend otherwise.
///
/// The release is the only signed thing in the product. The signing key is not
/// available to this module and no signature is produced here, so the type is
/// named for what it is and there is no method on it that yields a
/// [`crate::content::TrustedRelease`]. Getting from here to a client that will
/// accept anything requires a real signature over
/// [`UnsignedRelease::bytes_to_sign`].
#[derive(Debug, Clone)]
pub struct UnsignedRelease {
    release: Release,
    bytes: Vec<u8>,
}

impl UnsignedRelease {
    pub fn release(&self) -> &Release {
        &self.release
    }

    /// The exact bytes a signature must cover.
    ///
    /// Signing a re-serialization of the same object is how signature
    /// verification silently starts passing on content nobody signed:
    /// [`crate::content::SignatureVerifier`] is handed the served bytes and
    /// nothing else, so those bytes have to be these.
    pub fn bytes_to_sign(&self) -> &[u8] {
        &self.bytes
    }

    /// Write the unsigned release to disk, creating parent directories.
    ///
    /// It is not usable by a client in this state, and that is the intended dead
    /// end: a client can only open a release together with a signature.
    pub fn write_to(&self, path: &Path) -> Result<()> {
        write_exact(path, &self.bytes)
    }
}

/// Collects pinned component manifests into a release.
#[derive(Debug, Clone)]
pub struct UnsignedReleaseBuilder {
    version: String,
    sequence: u64,
    components: BTreeMap<String, PinnedComponent>,
}

impl UnsignedReleaseBuilder {
    pub fn new(version: &str, sequence: u64) -> Result<Self> {
        check_version(version)?;
        check_sequence(sequence)?;
        Ok(Self {
            version: version.to_string(),
            sequence,
            components: BTreeMap::new(),
        })
    }

    /// Pin one component manifest at the URL it will be served from.
    ///
    /// Refuses a second pin for the same component name rather than replacing
    /// the first. A silent replacement is how a release ends up pinning the hash
    /// of a manifest that was never published, which every client then reports
    /// as tampering.
    pub fn pin(mut self, url: &str, component: &SerializedComponent) -> Result<Self> {
        let name = component.component().component.clone();
        if ComponentId::parse(&name).is_none() {
            return Err(refuse(format!(
                "refusing to pin the component {name:?}: it is not a component name. \
                 A release pinning a name no client asks for is a release with a \
                 typo in it"
            )));
        }
        if url.trim().is_empty() {
            return Err(refuse(format!(
                "refusing to pin {name:?} with an empty url: the url is how the \
                 client fetches the manifest, and there is no value to invent \
                 for it"
            )));
        }
        if self.components.contains_key(&name) {
            return Err(refuse(format!(
                "refusing to pin {name:?} twice: the second pin would silently \
                 replace the first, and a release pinning the hash of a manifest \
                 that was never published is refused by every client as a hash \
                 mismatch"
            )));
        }
        self.components.insert(
            name,
            PinnedComponent {
                url: url.to_string(),
                sha256: component.sha256().clone(),
            },
        );
        Ok(self)
    }

    /// Serialize the release, and prove it parses before returning.
    pub fn build(self) -> Result<UnsignedRelease> {
        if self.components.is_empty() {
            return Err(refuse(
                "refusing to build a release that pins no components: it refuses \
                 every component a client asks for, so publishing it would look \
                 like an update and deliver nothing",
            ));
        }

        let release = Release {
            schema: RELEASE_SCHEMA.to_string(),
            version: self.version,
            sequence: self.sequence,
            components: self.components,
        };

        let bytes = serde_json::to_vec_pretty(&release)
            .map_err(|e| refuse(format!("the release manifest could not be serialized: {e}")))?;

        // Parse the output back, for the same reason the component does: a
        // release that does not parse is one a signature would be wasted on.
        Release::parse(&bytes).map_err(|e| {
            refuse(format!(
                "the release this generator produced does not parse back ({e})"
            ))
        })?;

        Ok(UnsignedRelease { release, bytes })
    }
}

// ---------------------------------------------------------------------------
// The rebuild record
// ---------------------------------------------------------------------------

/// What [`record_rebuild`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebuildRecordOutcome {
    /// Every named file was hashed and recorded.
    Recorded { paths: Vec<String> },

    /// The observer asked to stop.
    ///
    /// Records completed before the cancel are kept in the state, because each
    /// one is independently true and dropping them would cost the user another
    /// pass over a 1,459,978,240 byte image. The file that was mid-hash is not
    /// recorded, because a hash of part of a file is not a hash of the file.
    Cancelled {
        recorded: Vec<String>,
        progress: ScanProgress,
    },
}

/// Record what the launcher's own rebuild produced, so repair can tell a user's
/// music from corruption.
///
/// `rebuilt` names exactly the files the launcher wrote, and it is deliberately
/// not "everything in the tree". A state file listing every file blanket-exempts
/// the whole component from repair, and then real corruption of an unrelated
/// file reads as the user's music and is never fixed. That is the failure that
/// makes this an explicit list rather than a second scan.
///
/// `state` is taken by reference and added to rather than returned fresh. A
/// generator that returned a new state would silently drop the records of every
/// earlier rebuild, and each dropped record is a file whose next verify reports
/// damage and whose repair overwrites the user's own content.
pub fn record_rebuild(
    root: &Path,
    component: &Component,
    rebuilt: &[&str],
    state: &mut RebuildState,
    observer: &dyn ScanObserver,
) -> Result<RebuildRecordOutcome> {
    if !component.customizable {
        return Err(refuse(format!(
            "refusing to record a rebuild for {:?}: the component is not \
             customizable, so nothing is entitled to rebuild it. Verify ignores \
             rebuild records for such a component anyway, and writing them \
             creates a file that looks like permission to skip repair, which \
             would start excusing tampering the moment a manifest flipped the \
             flag",
            component.component
        )));
    }
    if rebuilt.is_empty() {
        return Err(refuse(format!(
            "refusing to record a rebuild for {:?} with no files named: there is \
             nothing to record, and an empty call is more likely a caller that \
             lost track of what it wrote",
            component.component
        )));
    }

    let mut targets = Vec::with_capacity(rebuilt.len());
    for relative in rebuilt {
        if !component.files.iter().any(|f| f.path == *relative) {
            return Err(refuse(format!(
                "refusing to record a rebuild of {relative:?}: the manifest for \
                 {:?} does not list it. Verify only looks up paths taken from \
                 the manifest, so the record would never be consulted, and the \
                 file would be unmanaged content that repair never restores",
                component.component
            )));
        }
        let path = resolve_under(root, relative)?;
        let meta = std::fs::metadata(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        targets.push((relative.to_string(), path, meta.len()));
    }

    let mut progress = ScanProgress {
        phase: ScanPhase::Hashing,
        files_found: targets.len() as u64,
        files_hashed: 0,
        bytes_hashed: 0,
        bytes_total: Some(targets.iter().map(|(_, _, len)| *len).sum()),
        current: None,
    };

    let mut buf = vec![0u8; READ_CHUNK];
    let mut recorded = Vec::with_capacity(targets.len());

    for (relative, path, _) in &targets {
        progress.current = Some(relative.clone());
        match hash_with_progress(path, &mut buf, &mut progress, observer)? {
            Hashed::Cancelled => return Ok(RebuildRecordOutcome::Cancelled { recorded, progress }),
            Hashed::Done { sha256, .. } => {
                state.record(relative, sha256);
                recorded.push(relative.clone());
            }
        }
        progress.files_hashed += 1;
        if observer.progress(&progress).is_cancel() {
            return Ok(RebuildRecordOutcome::Cancelled { recorded, progress });
        }
    }

    progress.current = None;
    Ok(RebuildRecordOutcome::Recorded { paths: recorded })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{hash_file, FileVerdict, Verifier};
    use std::cell::{Cell, RefCell};

    // -----------------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------------

    fn write_file(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
    }

    fn offline_spec() -> ComponentSpec {
        ComponentSpec::new(ComponentId::OfflineContent, "2.4.0", 7).unwrap()
    }

    fn scan_to_completion(root: &Path, spec: &ComponentSpec) -> Scan {
        match scan_component(root, spec, &RunToCompletion).unwrap() {
            ScanOutcome::Complete(scan) => scan,
            ScanOutcome::Cancelled { .. } => panic!("RunToCompletion never cancels"),
        }
    }

    /// Continues for `n` calls and then cancels, so a test can stop a scan at a
    /// chosen point including part way through one file.
    #[derive(Debug)]
    struct CancelAfter(Cell<u32>);

    impl CancelAfter {
        fn calls(n: u32) -> Self {
            Self(Cell::new(n))
        }
    }

    impl ScanObserver for CancelAfter {
        fn progress(&self, _progress: &ScanProgress) -> ScanControl {
            let left = self.0.get();
            if left == 0 {
                return ScanControl::Cancel;
            }
            self.0.set(left - 1);
            ScanControl::Continue
        }
    }

    /// Keeps every phase and denominator it was handed.
    #[derive(Debug, Default)]
    struct RecordProgress(RefCell<Vec<(ScanPhase, Option<u64>, bool)>>);

    impl ScanObserver for RecordProgress {
        fn progress(&self, progress: &ScanProgress) -> ScanControl {
            self.0.borrow_mut().push((
                progress.phase,
                progress.bytes_total,
                progress.fraction().is_some(),
            ));
            ScanControl::Continue
        }
    }

    /// Rewrites a file as soon as hashing begins, which is a tree moving under
    /// the scan. Enumeration is finished by then, so the length the walk
    /// recorded is already fixed.
    #[derive(Debug)]
    struct RewriteWhenHashingStarts {
        path: PathBuf,
        contents: Vec<u8>,
        done: Cell<bool>,
    }

    impl ScanObserver for RewriteWhenHashingStarts {
        fn progress(&self, progress: &ScanProgress) -> ScanControl {
            if progress.phase == ScanPhase::Hashing && !self.done.get() {
                self.done.set(true);
                std::fs::write(&self.path, &self.contents).unwrap();
            }
            ScanControl::Continue
        }
    }

    // -----------------------------------------------------------------------
    // Identity, and the field that stops repair deleting music
    // -----------------------------------------------------------------------

    #[test]
    fn component_ids_round_trip_their_wire_names() {
        for id in ComponentId::ALL {
            assert_eq!(ComponentId::parse(id.as_str()), Some(*id), "{id:?}");
        }
        assert_eq!(ComponentId::parse("news"), None, "the eighth is not one");
        assert_eq!(ComponentId::parse("Runtime"), None, "names are exact");
    }

    #[test]
    fn the_full_disc_is_a_component_the_publisher_knows() {
        assert_eq!(ComponentId::GameFull.as_str(), "game-full");
        assert!(
            ComponentId::ALL.contains(&ComponentId::GameFull),
            "publishing and name validation need to see it"
        );
    }

    /// The downloaded disc is a PRISTINE image, so it must never install into
    /// the directory holding the booted one. The booted image is the mutable
    /// layer: a roster apply rewrites it in place, and pointing the update
    /// engine's staging and prior machinery at that path aims it at the file
    /// the player is currently playing.
    #[test]
    fn the_full_disc_never_installs_over_the_booted_image() {
        assert_ne!(
            ComponentId::GameFull.install_root(),
            Some(DataDir::LibraryOffline.relative()),
            "game-full would install over the player's live game"
        );
    }

    /// The whole reason `customizable` exists. If a second component were ever
    /// marked customizable, repair would stop treating tampering with it as
    /// damage, and the online image is trusted by matchmaking precisely because
    /// it is not customizable.
    #[test]
    fn exactly_one_component_is_customizable() {
        let customizable: Vec<&str> = ComponentId::ALL
            .iter()
            .filter(|id| id.is_customizable())
            .map(|id| id.as_str())
            .collect();
        assert_eq!(customizable, vec!["offline-content"]);
    }

    /// A generated manifest must carry the right value per component rather
    /// than a default. The roots for `launcher` and `controller-db` here are
    /// arbitrary because neither layout is settled; the assertion is about the
    /// flag, not the path.
    #[test]
    fn a_generated_manifest_takes_customizable_from_the_component_identity() {
        for id in ComponentId::ALL {
            let dir = tempfile::tempdir().unwrap();
            write_file(dir.path(), "content.bin", b"anything");

            let spec = match id.install_root() {
                Some(_) => ComponentSpec::new(*id, "1.0.0", 1).unwrap(),
                None if *id == ComponentId::Textures => {
                    ComponentSpec::with_install_root(*id, "1.0.0", 1, "library/textures/GN7E69")
                        .unwrap()
                }
                None => ComponentSpec::with_install_root(*id, "1.0.0", 1, "runtime/aside").unwrap(),
            };

            let scan = scan_to_completion(dir.path(), &spec);
            assert_eq!(
                scan.component.customizable,
                *id == ComponentId::OfflineContent,
                "{} got customizable = {}",
                id.as_str(),
                scan.component.customizable
            );
            assert_eq!(scan.component.component, id.as_str());
        }
    }

    /// `customizable` has no serde default in `manifest.rs` deliberately, so a
    /// manifest omitting it fails to parse. This proves the generator's own
    /// output actually contains it, rather than relying on a default that does
    /// not exist.
    #[test]
    fn the_generated_bytes_contain_customizable_and_will_not_parse_without_it() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"the game");
        let scan = scan_to_completion(dir.path(), &offline_spec());
        let serialized = SerializedComponent::new(scan.component).unwrap();

        let text = String::from_utf8(serialized.bytes().to_vec()).unwrap();
        assert!(text.contains("\"customizable\": true"), "{text}");
        assert!(Component::parse(serialized.bytes()).is_ok());

        // Drop the field the way a careless publisher might. The pretty form
        // puts it on its own line, so removing that line leaves valid JSON, and
        // the client still refuses it rather than assuming false.
        let without: String = text
            .lines()
            .filter(|line| !line.contains("\"customizable\""))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            serde_json::from_str::<serde_json::Value>(&without).is_ok(),
            "the test needs to remove only the field, not break the json:\n{without}"
        );
        assert!(
            Component::parse(without.as_bytes()).is_err(),
            "omitting customizable must fail to parse:\n{without}"
        );
    }

    // -----------------------------------------------------------------------
    // The point of the module: verify and repair can now run
    // -----------------------------------------------------------------------

    #[test]
    fn a_generated_manifest_verifies_clean_against_the_tree_it_came_from() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"the game");
        write_file(dir.path(), "extras/readme.txt", b"notes");
        write_file(dir.path(), "extras/deep/thing.bin", &[7u8; 4096]);

        let scan = scan_to_completion(dir.path(), &offline_spec());
        assert_eq!(scan.component.files.len(), 3);
        assert!(
            scan.component.traversal_offenders().is_empty(),
            "a walk must not produce an escaping path"
        );

        let state = RebuildState::default();
        let report = Verifier::new(dir.path(), &state)
            .verify(&scan.component)
            .unwrap();
        assert!(report.is_healthy(), "{report:?}");
        assert!(report.to_repair().is_empty());
    }

    #[test]
    fn against_a_generated_manifest_a_deleted_file_is_missing_and_an_edited_one_is_damaged() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"the game");
        write_file(dir.path(), "extras/readme.txt", b"notes");
        let scan = scan_to_completion(dir.path(), &offline_spec());

        std::fs::remove_file(dir.path().join("NSR.iso")).unwrap();
        std::fs::write(dir.path().join("extras").join("readme.txt"), b"edited!!").unwrap();

        let state = RebuildState::default();
        let report = Verifier::new(dir.path(), &state)
            .verify(&scan.component)
            .unwrap();
        assert_eq!(report.verdicts["NSR.iso"], FileVerdict::Missing);
        assert!(matches!(
            report.verdicts["extras/readme.txt"],
            FileVerdict::Damaged { .. }
        ));
        assert!(!report.is_healthy());
    }

    /// The end to end case the whole schema exists for: generate a manifest,
    /// let the user rebuild the image with their own music, record what was
    /// produced, and confirm repair leaves it alone.
    #[test]
    fn a_generated_manifest_plus_a_recorded_rebuild_spares_a_users_music() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"the shipped game");
        let scan = scan_to_completion(dir.path(), &offline_spec());

        std::fs::write(
            dir.path().join("NSR.iso"),
            b"the game with my own music in it",
        )
        .unwrap();

        let mut state = RebuildState::default();
        let outcome = record_rebuild(
            dir.path(),
            &scan.component,
            &["NSR.iso"],
            &mut state,
            &RunToCompletion,
        )
        .unwrap();
        assert_eq!(
            outcome,
            RebuildRecordOutcome::Recorded {
                paths: vec!["NSR.iso".to_string()]
            }
        );

        let report = Verifier::new(dir.path(), &state)
            .verify(&scan.component)
            .unwrap();
        assert_eq!(report.verdicts["NSR.iso"], FileVerdict::UserModified);
        assert!(report.is_healthy(), "custom music is not a fault");
        assert!(report.to_repair().is_empty(), "repair must not touch it");
    }

    /// Data loss if broken. A generator that replaced the state instead of
    /// adding to it would drop the exemption for every previously rebuilt file,
    /// and the next repair would download over the user's content.
    #[test]
    fn recording_a_rebuild_keeps_records_that_were_already_there() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"shipped");
        let scan = scan_to_completion(dir.path(), &offline_spec());

        let mut state = RebuildState::default();
        state.record("some/older/file.bin", Sha256Hex::from_bytes(b"earlier"));

        record_rebuild(
            dir.path(),
            &scan.component,
            &["NSR.iso"],
            &mut state,
            &RunToCompletion,
        )
        .unwrap();

        assert_eq!(
            state.rebuilt.get("some/older/file.bin"),
            Some(&Sha256Hex::from_bytes(b"earlier")),
            "an earlier rebuild record must survive"
        );
        assert!(state.rebuilt.contains_key("NSR.iso"));
    }

    #[test]
    fn recording_a_rebuild_is_refused_for_a_component_that_is_not_customizable() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "FR4N31.iso", b"pristine online");
        let spec = ComponentSpec::new(ComponentId::OnlineContent, "2.4.0", 7).unwrap();
        let scan = scan_to_completion(dir.path(), &spec);

        let mut state = RebuildState::default();
        let err = record_rebuild(
            dir.path(),
            &scan.component,
            &["FR4N31.iso"],
            &mut state,
            &RunToCompletion,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not customizable"), "{err}");
        assert!(state.rebuilt.is_empty(), "nothing may be recorded");
    }

    /// A record keyed on a path the manifest does not list can never be
    /// consulted, because verify only looks up manifest paths. Accepting it
    /// would leave the caller believing a file is protected when it is not.
    #[test]
    fn recording_a_path_the_manifest_does_not_list_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"shipped");
        write_file(dir.path(), "notes.txt", b"mine");
        let scan = scan_to_completion(dir.path(), &offline_spec());

        // Drop a row, so the manifest no longer lists a file that is on disk.
        let mut component = scan.component.clone();
        component.files.retain(|f| f.path != "notes.txt");

        let mut state = RebuildState::default();
        let err = record_rebuild(
            dir.path(),
            &component,
            &["notes.txt"],
            &mut state,
            &RunToCompletion,
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not list it"), "{err}");
        assert!(state.rebuilt.is_empty());
    }

    #[test]
    fn recording_a_rebuild_with_nothing_named_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"shipped");
        let scan = scan_to_completion(dir.path(), &offline_spec());
        let mut state = RebuildState::default();
        assert!(record_rebuild(
            dir.path(),
            &scan.component,
            &[],
            &mut state,
            &RunToCompletion
        )
        .is_err());
    }

    // -----------------------------------------------------------------------
    // Hashing and sizes
    // -----------------------------------------------------------------------

    /// `bytes` must be the length of the file, never the space it occupies. The
    /// shared football texture is 1,398,276 bytes long and Explorer reports
    /// 1,400,832, which is 342 x 4,096 of NTFS allocation. A manifest carrying
    /// the allocated size fails its own size check on a pristine install.
    #[test]
    fn bytes_is_the_length_of_the_file_not_its_size_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let contents = vec![0xABu8; 1234];
        write_file(dir.path(), "tex.dds", &contents);

        let scan = scan_to_completion(dir.path(), &offline_spec());
        assert_eq!(scan.component.files[0].bytes, 1234);
        assert_eq!(scan.component.total_bytes(), 1234);
    }

    /// If the generator and the verifier ever disagreed on a digest, every file
    /// in the library would verify as damaged and repair would re-fetch all of
    /// it. The file is larger than one read chunk so the streaming loop runs
    /// several times in both.
    #[test]
    fn the_generator_and_the_verifier_compute_the_same_digest() {
        let dir = tempfile::tempdir().unwrap();
        let contents = vec![0x5Au8; 3 * READ_CHUNK + 17];
        write_file(dir.path(), "big.bin", &contents);

        let scan = scan_to_completion(dir.path(), &offline_spec());
        let generated = &scan.component.files[0].sha256;
        assert_eq!(
            *generated,
            hash_file(&dir.path().join("big.bin")).unwrap(),
            "library::hash_file and the generator must agree"
        );
        assert_eq!(*generated, Sha256Hex::from_bytes(&contents));
    }

    /// The release pins the manifest by the hash of its bytes, so the same tree
    /// has to produce the same bytes. Filesystem enumeration order is not
    /// stable, so without the sort the pin would depend on the build machine.
    #[test]
    fn the_same_tree_produces_byte_identical_manifest_bytes_twice() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["z.bin", "a.bin", "m/inner.bin", "m/aaa.bin", "b.bin"] {
            write_file(dir.path(), name, name.as_bytes());
        }

        let first =
            SerializedComponent::new(scan_to_completion(dir.path(), &offline_spec()).component)
                .unwrap();
        let second =
            SerializedComponent::new(scan_to_completion(dir.path(), &offline_spec()).component)
                .unwrap();

        assert_eq!(first.bytes(), second.bytes());
        assert_eq!(first.sha256(), second.sha256());

        let paths: Vec<&str> = first
            .component()
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "file rows must be in a stable order");
    }

    /// A manifest is read on every platform. A backslash would make the same
    /// tree hash differently depending on where it was generated, and
    /// `DataDir::relative` holds the rest of the codebase to the same rule.
    #[test]
    fn nested_paths_use_forward_slashes() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "extras/deep/thing.bin", b"x");
        let scan = scan_to_completion(dir.path(), &offline_spec());
        assert_eq!(scan.component.files[0].path, "extras/deep/thing.bin");
        assert!(!scan.component.files[0].path.contains('\\'));
    }

    /// An empty file list verifies clean against an empty install, so it would
    /// make a component that never arrived look healthy.
    #[test]
    fn a_root_with_no_files_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("empty")).unwrap();
        let err = scan_component(dir.path(), &offline_spec(), &RunToCompletion).unwrap_err();
        assert!(err.to_string().contains("no files"), "{err}");
    }

    #[test]
    fn a_missing_root_and_a_file_as_a_root_are_both_refused() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("not-there");
        assert!(scan_component(&absent, &offline_spec(), &RunToCompletion).is_err());

        write_file(dir.path(), "just-a-file.bin", b"x");
        let err = scan_component(
            &dir.path().join("just-a-file.bin"),
            &offline_spec(),
            &RunToCompletion,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not a directory"), "{err}");
    }

    /// The schema has no row for a directory, so an empty one cannot survive an
    /// install. Reported rather than dropped, because otherwise a tree that
    /// needs it fails only on the user's machine.
    #[test]
    fn empty_directories_are_reported_rather_than_dropped() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "nested/deep/thing.bin", b"x");
        std::fs::create_dir_all(dir.path().join("logs")).unwrap();

        let scan = scan_to_completion(dir.path(), &offline_spec());
        assert_eq!(scan.empty_dirs, vec!["logs".to_string()]);
    }

    // -----------------------------------------------------------------------
    // Progress and cancellation
    // -----------------------------------------------------------------------

    /// A cancelled scan must not yield a manifest. A truncated file list parses
    /// and verifies clean while silently un-managing every file the walk did
    /// not reach, so repair never notices those files going missing.
    #[test]
    fn a_cancelled_scan_returns_no_component() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.bin", b"one");
        write_file(dir.path(), "b.bin", b"two");

        let outcome = scan_component(dir.path(), &offline_spec(), &CancelAfter::calls(0)).unwrap();
        assert!(outcome.completed().is_none());
        match outcome {
            ScanOutcome::Cancelled { progress } => {
                assert_eq!(progress.phase, ScanPhase::Enumerating);
                assert_eq!(progress.bytes_total, None);
            }
            ScanOutcome::Complete(_) => panic!("the observer cancelled immediately"),
        }
    }

    /// A disc image is one file and the entire wait, so a cancel that is only
    /// checked between files would be ignored for the whole scan.
    #[test]
    fn a_cancel_is_honored_part_way_through_a_single_large_file() {
        let dir = tempfile::tempdir().unwrap();
        let contents = vec![0u8; 3 * READ_CHUNK + 17];
        write_file(dir.path(), "one-big.iso", &contents);

        // Call 1 is the file being found; call 2 is the first megabyte read.
        let outcome = scan_component(dir.path(), &offline_spec(), &CancelAfter::calls(1)).unwrap();
        match outcome {
            ScanOutcome::Cancelled { progress } => {
                assert_eq!(progress.phase, ScanPhase::Hashing);
                assert_eq!(progress.files_hashed, 0, "the file never finished");
                assert_eq!(progress.bytes_hashed, READ_CHUNK as u64);
                assert_eq!(progress.bytes_total, Some(contents.len() as u64));
                let fraction = progress.fraction().expect("a denominator exists now");
                assert!(fraction > 0.0 && fraction < 1.0, "{fraction}");
            }
            ScanOutcome::Complete(_) => panic!("a mid-file cancel was ignored"),
        }
    }

    /// While the walk is running there is no denominator. Reporting one would
    /// mean showing the user a percentage the launcher did not measure.
    #[test]
    fn there_is_no_denominator_until_the_walk_has_finished() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.bin", &[1u8; 64]);
        write_file(dir.path(), "b.bin", &[2u8; 64]);

        let observer = RecordProgress::default();
        let scan = match scan_component(dir.path(), &offline_spec(), &observer).unwrap() {
            ScanOutcome::Complete(scan) => scan,
            ScanOutcome::Cancelled { .. } => panic!("RecordProgress never cancels"),
        };

        let seen = observer.0.borrow();
        assert!(!seen.is_empty());
        for (phase, total, has_fraction) in seen.iter() {
            match phase {
                ScanPhase::Enumerating => {
                    assert_eq!(*total, None, "no total while walking");
                    assert!(!has_fraction, "no fraction while walking");
                }
                ScanPhase::Hashing => {
                    assert_eq!(*total, Some(128), "the total is final once walking ends");
                    assert!(has_fraction);
                }
            }
        }
        assert!(seen
            .iter()
            .any(|(phase, ..)| *phase == ScanPhase::Enumerating));
        assert!(seen.iter().any(|(phase, ..)| *phase == ScanPhase::Hashing));

        assert_eq!(scan.progress.files_hashed, 2);
        assert_eq!(scan.progress.files_total(), Some(2));
        assert_eq!(
            scan.progress.current, None,
            "nothing is in progress at the end"
        );
    }

    #[test]
    fn a_cancel_signal_set_from_elsewhere_stops_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.bin", b"one");

        let signal = CancelSignal::new();
        let handle = signal.clone();
        handle.cancel();
        assert!(signal.is_cancelled());

        let outcome = scan_component(dir.path(), &offline_spec(), &signal).unwrap();
        assert!(outcome.completed().is_none());
    }

    /// A manifest recorded from a tree that is being written to describes a
    /// state that never existed, and it fails its own verification on a
    /// pristine install. The observer here rewrites the second file once
    /// hashing has begun, which is after the walk fixed its length.
    #[test]
    fn a_tree_that_changes_under_the_scan_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.bin", &[1u8; 10]);
        write_file(dir.path(), "b.bin", &[2u8; 10]);

        let observer = RewriteWhenHashingStarts {
            path: dir.path().join("b.bin"),
            contents: vec![3u8; 40],
            done: Cell::new(false),
        };
        let err = scan_component(dir.path(), &offline_spec(), &observer).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("b.bin"), "{message}");
        assert!(
            message.contains("Something is writing to the tree"),
            "{message}"
        );
    }

    // -----------------------------------------------------------------------
    // Refusals that protect where files land
    // -----------------------------------------------------------------------

    #[test]
    fn an_install_root_that_contradicts_the_component_is_refused() {
        let err = ComponentSpec::with_install_root(ComponentId::Runtime, "1.0.0", 1, "saves")
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("runtime"), "{message}");
        assert!(message.contains("saves"), "{message}");

        for bad in [
            "/library/offline",
            "library\\offline",
            "C:/library/offline",
            "library/../saves",
            "library//offline",
            " library/offline",
            "",
        ] {
            assert!(
                ComponentSpec::with_install_root(ComponentId::Launcher, "1.0.0", 1, bad).is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    /// The three components whose root is not derivable must not get a guessed
    /// one, because that field decides where files land on disk.
    #[test]
    fn a_component_with_no_derivable_root_must_be_told_its_root() {
        for id in [
            ComponentId::Textures,
            ComponentId::Launcher,
            ComponentId::ControllerDb,
        ] {
            let err = ComponentSpec::new(id, "1.0.0", 1).unwrap_err();
            assert!(
                err.to_string().contains("with_install_root"),
                "{id:?}: {err}"
            );
        }
        assert_eq!(
            ComponentSpec::new(ComponentId::OfflineContent, "1.0.0", 1)
                .unwrap()
                .install_root(),
            "library/offline"
        );
    }

    /// Two game ids sharing one texture root would overwrite each other, and
    /// 2,252 of FR4N31's files are byte-identical to GN7E69's, so most of the
    /// damage would be invisible.
    #[test]
    fn textures_needs_a_game_id_in_its_install_root() {
        assert!(ComponentSpec::with_install_root(
            ComponentId::Textures,
            "1.0.1",
            1,
            "library/textures/GN7E69"
        )
        .is_ok());

        for bad in [
            "library/textures",
            "library/textures/GN7E69/sub",
            "library/offline",
        ] {
            assert!(
                ComponentSpec::with_install_root(ComponentId::Textures, "1.0.1", 1, bad).is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    /// `ContentLayout::downloads_dir` joins the version onto a path, so a
    /// separator in it can move a download outside the data root.
    #[test]
    fn a_version_that_could_escape_its_directory_is_refused() {
        for bad in [
            "",
            "  ",
            "2.4.0 ",
            "../../saves",
            "2.4/0",
            "2.4\\0",
            "C:2.4",
        ] {
            assert!(
                ComponentSpec::with_install_root(ComponentId::Launcher, bad, 1, "launcher")
                    .is_err(),
                "{bad:?} should be refused"
            );
        }
        assert!(
            ComponentSpec::with_install_root(ComponentId::Launcher, "1.4.0", 1, "launcher").is_ok()
        );
    }

    /// Proven against the client's own rule rather than asserted: a release at
    /// sequence 0 is refused even by an install that has no record at all.
    #[test]
    fn sequence_zero_is_refused_because_no_client_can_ever_accept_it() {
        let release = Release::parse(
            br#"{"schema":"nsr.release.v1","version":"1","sequence":0,"components":{}}"#,
        )
        .unwrap();
        assert!(
            release.check_sequence(0).is_err(),
            "an install with no record counts as 0, so sequence 0 is never newer"
        );

        assert!(ComponentSpec::new(ComponentId::Runtime, "1.0.0", 0).is_err());
        assert!(UnsignedReleaseBuilder::new("2.4.0", 0).is_err());
    }

    /// Legal on Linux, impossible on Windows. One of the two rows could never
    /// be satisfied, so repair would re-fetch them over each other without end.
    #[test]
    fn paths_differing_only_in_case_are_refused() {
        assert!(refuse_case_collisions(["a/B.dds", "a/c.dds"].into_iter()).is_ok());
        let err = refuse_case_collisions(["a/B.dds", "a/b.dds"].into_iter()).unwrap_err();
        assert!(err.to_string().contains("differ only in case"), "{err}");
    }

    /// Creating a real symbolic link needs a platform specific attribute that
    /// this crate confines to the platform module, so the refusal is pinned by
    /// its message: it has to name the offender and say what to do.
    #[test]
    fn the_symlink_refusal_names_the_link_and_says_what_to_do() {
        let err = refuse_links(Path::new("tree"), &["Load/link.dds".to_string()]);
        let message = err.to_string();
        assert!(message.contains("Load/link.dds"), "{message}");
        assert!(message.contains("Replace the link"), "{message}");
    }

    // -----------------------------------------------------------------------
    // Pinning, and the fact that nothing here is signed
    // -----------------------------------------------------------------------

    /// The release pins the manifest by the hash of the bytes as served, so the
    /// bytes that were hashed have to be the bytes that get published. Hashing
    /// one serialization and publishing another makes every client refuse the
    /// component with what looks like a tampering error.
    #[test]
    fn a_release_pins_the_exact_bytes_the_component_serialized_to() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"the game");
        let scan = scan_to_completion(dir.path(), &offline_spec());
        let serialized = SerializedComponent::new(scan.component).unwrap();

        let unsigned = UnsignedReleaseBuilder::new("2.4.0", 12)
            .unwrap()
            .pin("https://example/offline.json", &serialized)
            .unwrap()
            .build()
            .unwrap();

        // What a client does: parse the release, then accept the component
        // manifest only if it hashes to the pin.
        let parsed = Release::parse(unsigned.bytes_to_sign()).unwrap();
        let accepted = parsed
            .accept_component("offline-content", serialized.bytes())
            .unwrap();
        assert!(accepted.customizable, "the flag survived the round trip");

        let mut tampered = serialized.bytes().to_vec();
        let last = tampered.len() - 1;
        tampered[last] = b' ';
        assert!(
            parsed
                .accept_component("offline-content", &tampered)
                .is_err(),
            "one byte different must be refused"
        );

        // And what is written to disk is byte for byte what was pinned.
        let out = dir.path().join("published").join("offline.json");
        serialized.write_to(&out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), serialized.bytes());
        assert_eq!(
            Sha256Hex::from_bytes(&std::fs::read(&out).unwrap()),
            *serialized.sha256()
        );
    }

    #[test]
    fn a_release_that_pins_nothing_is_refused() {
        let err = UnsignedReleaseBuilder::new("2.4.0", 12)
            .unwrap()
            .build()
            .unwrap_err();
        assert!(err.to_string().contains("pins no components"), "{err}");
    }

    /// A silent replacement is how a release ends up pinning the hash of a
    /// manifest that was never published.
    #[test]
    fn pinning_one_component_twice_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"the game");
        let first =
            SerializedComponent::new(scan_to_completion(dir.path(), &offline_spec()).component)
                .unwrap();

        std::fs::write(dir.path().join("NSR.iso"), b"a different game").unwrap();
        let second =
            SerializedComponent::new(scan_to_completion(dir.path(), &offline_spec()).component)
                .unwrap();
        assert_ne!(first.sha256(), second.sha256());

        let err = UnsignedReleaseBuilder::new("2.4.0", 12)
            .unwrap()
            .pin("https://example/a.json", &first)
            .unwrap()
            .pin("https://example/b.json", &second)
            .unwrap_err();
        assert!(err.to_string().contains("twice"), "{err}");
    }

    #[test]
    fn a_pin_needs_a_url_and_a_known_component_name() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"the game");
        let scan = scan_to_completion(dir.path(), &offline_spec());

        let good = SerializedComponent::new(scan.component.clone()).unwrap();
        assert!(UnsignedReleaseBuilder::new("2.4.0", 12)
            .unwrap()
            .pin("   ", &good)
            .is_err());

        let mut renamed = scan.component;
        renamed.component = "news-config".to_string();
        let unknown = SerializedComponent::new(renamed).unwrap();
        let err = UnsignedReleaseBuilder::new("2.4.0", 12)
            .unwrap()
            .pin("https://example/x.json", &unknown)
            .unwrap_err();
        assert!(err.to_string().contains("not a component name"), "{err}");
    }

    /// The release produced here is unsigned, and the type says so. This test
    /// exists to record that the bytes handed to a signer are the same bytes
    /// that get published, because signing a re-serialization is how a
    /// signature quietly starts covering something else.
    #[test]
    fn the_bytes_offered_for_signing_are_the_bytes_that_get_written() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"the game");
        let serialized =
            SerializedComponent::new(scan_to_completion(dir.path(), &offline_spec()).component)
                .unwrap();
        let unsigned = UnsignedReleaseBuilder::new("2.4.0", 12)
            .unwrap()
            .pin("https://example/offline.json", &serialized)
            .unwrap()
            .build()
            .unwrap();

        let out = dir.path().join("release.json");
        unsigned.write_to(&out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), unsigned.bytes_to_sign());
        assert_eq!(unsigned.release().sequence, 12);
        assert_eq!(unsigned.release().components.len(), 1);
    }

    #[test]
    fn a_component_manifest_with_the_wrong_schema_is_not_serialized() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "NSR.iso", b"the game");
        let mut component = scan_to_completion(dir.path(), &offline_spec()).component;
        component.schema = "reborn.manifest.v1".to_string();
        let err = SerializedComponent::new(component).unwrap_err();
        assert!(err.to_string().contains("nsr.component.v1"), "{err}");
    }
}
