//! The Xbox Dev Mode payload wizard: a guided copy, not an installer.
//!
//! # What this is, in Adam's words
//!
//! > "What I mean by installer is legit just move all the files for the user."
//!
//! There is no console side to this. Nothing is deployed, no package is signed,
//! no Device Portal is contacted, and the wizard never speaks to an Xbox. It
//! chooses a removable drive, checks the drive can hold what is about to be
//! written, copies a measured file set, verifies what landed, and stops.
//! `docs/architecture/XBOX_DEV_MODE_INSTALLER.md` sections 5 to 9 are the
//! authority for every rule below.
//!
//! # The four failures this module is built around
//!
//! **A drive that fills at file 2,600 of 2,798.** The payload is 2,798 files and
//! about 3.01 GB, and 2,782 of those files sit in one flat texture directory.
//! Space is therefore checked once, before the first byte, against the number of
//! bytes that will actually be written. No path in this module starts copying
//! and then discovers it cannot finish, because [`CopyPlan::approve`] is the
//! only way to obtain a value [`copy`] will accept.
//!
//! **Writing to something that is not a USB stick.** A drive letter is not
//! evidence of anything. [`Target`] can only be built by [`choose`] out of a
//! [`DriveSurvey`], so a caller cannot hand a bare path to the copy engine and
//! have it believed. The same gate refuses a removable volume that holds the
//! launcher's own files, which is a real shape: a portable install living on the
//! stick the user is about to overwrite.
//!
//! **Overwriting somebody's file silently.** No manifest history ships yet, so
//! the wizard genuinely cannot tell its own previous release's file from a file
//! the user put there by hand. It therefore treats every destination that
//! already holds different content as an overwrite that has to be listed, and
//! [`CopyPlan::approve`] refuses unless the caller passes back the exact paths it
//! displayed. Consent is a parameter, not a comment.
//!
//! **The drive being pulled out mid-copy.** That is the normal way this
//! operation ends badly, so it is a return value ([`Interruption::DriveWentAway`])
//! rather than an error: the record of what did land is worth more than a stack
//! of io errors, and it is what a resume would be built from.
//!
//! # The memory card rule, and why one line of it looks contradictory
//!
//! `GC/` holds the only irreplaceable data on the drive. The shipped 2.1 payload
//! carries the author's own 16 MB card with his saves inside it, so a literal
//! "copy the package" hands every user someone else's saves and destroys their
//! own. Here that cannot be expressed: a profile naming any destination under
//! `GC/` is refused when it loads, and the authoring walk never emits a file
//! from there.
//!
//! Empty directories under `GC/` are still allowed, and that is deliberate
//! rather than an oversight. `Config/GraphicMods`, `GC/EUR`, `GC/JAP` and
//! `GC/USA/Card A` are empty in the payload, a copy driven by a file list omits
//! all four without a word, and whether the app needs them is unknown. Creating
//! a directory cannot truncate, rename or replace a card, so the contract holds:
//! no *file* under `GC/` is ever written, and no card is ever touched at all.
//!
//! # What is deliberately not here
//!
//! - **The PC-side journal and resume** (`<data root>/xbox/journal-*.json`).
//!   [`Target::journal_key`] exists and is honest about being absent when the
//!   platform reports no volume identity, but nothing here writes a journal, so
//!   [`CopyPlan::build`] re-hashes the existing install every time instead of
//!   trusting a cache. Slow and correct.
//! - **Save backup and restore** (modes 4 and 5). They are the only paths
//!   allowed to touch `GC/`, and putting them in this module would put the one
//!   write that may touch a memory card next to the 2,798 that may not.
//! - **Stale-file cleanup.** Telling "content we shipped and have since dropped"
//!   from "content the user added" needs the shipped manifest history that
//!   section 9.5 requires and that does not exist yet. Without it every
//!   unmanaged file is foreign, and foreign files are reported and left alone.
//!   Nothing in this module deletes anything it did not itself write.
//!
//! # Two namespaces that look alike
//!
//! Every path the wizard copies *to* is PC-side. Every path *inside* a config
//! file is Xbox-side: the payload ships `ISOPath0 = E:\Games`, and rewriting
//! that to whichever letter the stick got on the PC produces a config that is
//! correct on the machine that made it and broken on the console it was made
//! for. This module moves bytes and never parses or edits file contents, so that
//! bug is unreachable rather than merely avoided.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::hash_file;
use crate::manifest::Sha256Hex;
use crate::platform::{Error, Platform, RemovableDrive, Result};

/// The schema of a destination profile, checked on load.
///
/// A profile written by a newer launcher is refused rather than guessed at, for
/// the same reason a copy journal must never parse a schema from the future: a
/// field whose meaning changed is worse than a field that is missing.
pub const PROFILE_SCHEMA: &str = "nsr.xbox.profile.v1";

/// The suffix every staged write uses before it is renamed into place.
///
/// Reserved: no payload file may end in it. These are the one class of file the
/// wizard removes without naming each one individually, because it wrote every
/// one of them and no other tool produces the suffix.
pub const STAGING_SUFFIX: &str = ".rbn-part";

/// Slack required on the drive on top of the bytes being written.
///
/// **A policy choice, not a measurement.** Nothing in the source material states
/// a figure. It covers directory entries for a 2,782 file flat directory and the
/// wizard's own staging names, and it is deliberately larger than any plausible
/// metadata cost so that a refusal here is never a near miss.
pub const STAGING_MARGIN_BYTES: u64 = 16 * 1024 * 1024;

/// The one directory name that is off limits for files, at any depth.
const GC_DIR: &str = "gc";

/// How many surplus files a verify report will name before it stops listing.
///
/// The count stays exact and [`VerifyReport::surplus_truncated`] says the list
/// was cut. A user with 40,000 unrelated files on the stick should not get a
/// 40,000 line report or a 40,000 element allocation.
const MAX_SURPLUS_LISTED: usize = 500;

fn invalid_profile(path: &Path, message: String) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, message),
    }
}

// ---------------------------------------------------------------------------
// The destination profile: data, because the layout is not known
// ---------------------------------------------------------------------------

/// How a payload entry can honestly be checked once it is on the drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedBy {
    /// Content proved by digest.
    Sha256,
    /// The file is zero bytes, so a digest match proves nothing whatsoever.
    /// Five of the payload's config files are empty, and a green check that
    /// cannot fail is worse than no check, because somebody will trust it.
    PresenceAndLength,
    /// The profile carries no digest for this entry, so only its length was
    /// compared. Never counted as content verification.
    LengthOnly,
}

impl VerifiedBy {
    /// Whether this check says anything at all about the file's contents.
    pub fn proves_content(self) -> bool {
        matches!(self, VerifiedBy::Sha256)
    }
}

/// One file to move, named on both sides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadEntry {
    /// Path relative to the payload root on the PC, forward slashes.
    pub src: String,
    /// Path relative to the drive root, forward slashes.
    pub dst: String,
    /// Absent means the profile cannot prove this file's contents. That is
    /// allowed and is reported as such, rather than being filled in with
    /// something that merely looks like a digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<Sha256Hex>,
    pub size: u64,
}

impl PayloadEntry {
    pub fn verified_by(&self) -> VerifiedBy {
        if self.size == 0 {
            VerifiedBy::PresenceAndLength
        } else if self.sha256.is_some() {
            VerifiedBy::Sha256
        } else {
            VerifiedBy::LengthOnly
        }
    }
}

/// A subtree of the payload root, and where it lands on the drive.
///
/// Used only by [`DestinationProfile::author`]. Two are expected: the
/// `Step 2 - Transfer to Xbox` tree onto the app's user root, and the verified
/// disc image with its cover into `Games/`.
#[derive(Debug, Clone)]
pub struct SourceTree {
    /// Relative to the payload root. `"."` means the payload root itself.
    pub relative_root: String,
    /// Relative to the drive root. `"."` means the drive root itself.
    pub dst_prefix: String,
}

/// Where every file goes on the drive.
///
/// **This is data on purpose.** Open item 1 of the architecture document is
/// BLOCKING: nobody has confirmed where the Xbox-side Dolphin app reads its user
/// directory from, and no README, manifest or instruction file exists anywhere in
/// the package. A wrong constant compiled into the copy engine is a re-release; a
/// wrong value in a profile file is an edit. So there are no destination paths in
/// this module's code, and none in its tests beyond the ones a test authors for
/// itself.
///
/// For the same reason there is no built-in default profile.
/// [`DestinationProfile::load`] on a missing file is an error that says where to
/// put one, not an empty profile that copies nothing and reports success.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DestinationProfile {
    schema: String,
    /// A name for this layout, carried into the verify report so a user can say
    /// which one they used.
    profile: String,
    /// **Required, with no serde default.** A profile that does not state this
    /// cannot be loaded, because an absent-field default of `false` would be
    /// indistinguishable from a deliberate `false`. The UI has to be able to say
    /// "this layout has not been tested on a console" and mean it.
    confirmed_on_hardware: bool,
    game_dir: String,
    user_dir: String,
    entries: Vec<PayloadEntry>,
    #[serde(default)]
    empty_dirs: Vec<String>,
}

impl DestinationProfile {
    /// Read a profile from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                invalid_profile(
                    path,
                    format!(
                        "no Xbox destination profile at {}. The layout the Xbox \
                         app expects is not documented anywhere in the shipped \
                         package, so the wizard will not guess one: install a \
                         profile file, or author one from a measured payload \
                         with DestinationProfile::author.",
                        path.display()
                    ),
                )
            } else {
                Error::Io {
                    path: path.to_path_buf(),
                    source,
                }
            }
        })?;
        Self::from_json(&raw).map_err(|e| invalid_profile(path, e))
    }

    /// Parse and validate a profile.
    ///
    /// The error is a sentence rather than a serde message wherever the problem
    /// is one the caller can act on.
    pub fn from_json(raw: &str) -> std::result::Result<Self, String> {
        let profile: Self = serde_json::from_str(raw).map_err(|e| {
            format!(
                "this is not a v1 Xbox destination profile: {e}. Required keys \
                 are schema, profile, confirmed_on_hardware, game_dir, user_dir \
                 and entries."
            )
        })?;
        profile.validate()?;
        Ok(profile)
    }

    /// Every check that has to pass before a profile is allowed near a drive.
    ///
    /// All of these are cheap, and every one of them would otherwise surface as
    /// a half-written install: a bad digest discovered at file 2,600, a duplicate
    /// destination that silently lets the last entry win, a `..` that climbs out
    /// of the drive root into the user's Windows install.
    fn validate(&self) -> std::result::Result<(), String> {
        if self.schema != PROFILE_SCHEMA {
            return Err(format!(
                "profile schema is {:?}, expected {:?}. Refusing: a profile \
                 written by a different version of the launcher may mean \
                 something different by the same field.",
                self.schema, PROFILE_SCHEMA
            ));
        }
        if self.profile.trim().is_empty() {
            return Err("the profile has no name, and the verify report has to \
                        be able to say which layout was used."
                .to_string());
        }
        check_relative_dir("game_dir", &self.game_dir)?;
        check_relative_dir("user_dir", &self.user_dir)?;
        if self.entries.is_empty() {
            return Err("this profile names no files. A wizard that copies \
                        nothing and then reports success is worse than one that \
                        refuses."
                .to_string());
        }

        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        for entry in &self.entries {
            check_relative_file("src", &entry.src)?;
            check_relative_file("dst", &entry.dst)?;
            if let Some(digest) = &entry.sha256 {
                Sha256Hex::parse(digest.as_str()).map_err(|_| {
                    format!(
                        "{:?} carries {:?}, which is not a sha-256 digest. \
                         Refusing now rather than at file 2,600 of a copy.",
                        entry.dst, digest
                    )
                })?;
            }
            if has_component(&entry.dst, GC_DIR) {
                return Err(format!(
                    "{:?} writes a file under GC/, which holds the only \
                     irreplaceable data on the drive. The wizard never writes a \
                     file there in any mode, so remove the entry. Restoring a \
                     memory card is its own separately confirmed action.",
                    entry.dst
                ));
            }
            if entry.dst.ends_with(STAGING_SUFFIX) {
                return Err(format!(
                    "{:?} ends in {}, which is the wizard's own staging suffix. \
                     Files with that suffix are deleted without being listed \
                     individually, so a payload file may not use it.",
                    entry.dst, STAGING_SUFFIX
                ));
            }
            if !self.is_inside_declared_roots(&entry.dst) {
                return Err(format!(
                    "{:?} lands outside both game_dir ({:?}) and user_dir \
                     ({:?}). Those two are the only places on the drive this \
                     profile is allowed to write.",
                    entry.dst, self.game_dir, self.user_dir
                ));
            }
            // Case-insensitive, because the filesystems this lands on are.
            // Two entries differing only in case are one file on the drive, and
            // the second copy would silently win.
            let key = entry.dst.to_lowercase();
            if let Some(first) = seen.insert(key, entry.dst.clone()) {
                return Err(format!(
                    "{:?} and {:?} are the same file on a case-insensitive \
                     drive, so one would silently overwrite the other.",
                    first, entry.dst
                ));
            }
        }

        let mut dirs = BTreeSet::new();
        for dir in &self.empty_dirs {
            check_relative_dir("empty_dirs entry", dir)?;
            if dir == "." {
                return Err("\".\" is not an empty directory to create.".to_string());
            }
            if !dirs.insert(dir.to_lowercase()) {
                return Err(format!("{dir:?} is listed twice in empty_dirs."));
            }
        }
        Ok(())
    }

    fn is_inside_declared_roots(&self, dst: &str) -> bool {
        let lowered = dst.to_lowercase();
        for root in [self.game_dir.as_str(), self.user_dir.as_str()] {
            if root == "." {
                return true;
            }
            let prefix = format!("{}/", root.to_lowercase());
            if lowered.len() > prefix.len() && lowered.starts_with(&prefix) {
                return true;
            }
        }
        false
    }

    /// Measure a payload on disk and write down what is actually there.
    ///
    /// Section 9.1: no checksum, hash list or manifest exists anywhere in the
    /// shipped package, so there is nothing to inherit and the launcher has to
    /// author its own. Everything here is read off the files, which is what
    /// separates this from inventing an entry list.
    ///
    /// Two things are not measurable and so are not claimed.
    /// `confirmed_on_hardware` is always `false`, because authoring a profile on
    /// a PC says nothing about a console. And no file under `GC/` is ever
    /// emitted, so the author's own memory card cannot be swept into a payload
    /// the way the shipped 2.1 package swept it in.
    pub fn author(
        name: &str,
        game_dir: &str,
        user_dir: &str,
        payload_root: &Path,
        trees: &[SourceTree],
    ) -> Result<Self> {
        let mut entries = Vec::new();
        let mut empty_dirs = BTreeSet::new();

        for tree in trees {
            let root = join_relative(payload_root, &tree.relative_root);
            if !root.is_dir() {
                return Err(Error::Io {
                    path: root,
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "cannot author a profile from a payload subtree that is \
                         not there",
                    ),
                });
            }
            author_tree(
                &root,
                &tree.relative_root,
                &tree.dst_prefix,
                &mut entries,
                &mut empty_dirs,
            )?;
        }

        entries.sort_by(|a: &PayloadEntry, b: &PayloadEntry| a.dst.cmp(&b.dst));
        let profile = Self {
            schema: PROFILE_SCHEMA.to_string(),
            profile: name.to_string(),
            confirmed_on_hardware: false,
            game_dir: game_dir.to_string(),
            user_dir: user_dir.to_string(),
            entries,
            empty_dirs: empty_dirs.into_iter().collect(),
        };
        profile.validate().map_err(|e| Error::Io {
            path: payload_root.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;
        Ok(profile)
    }

    /// Write the profile out, creating the parent directory if needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| Error::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(e),
        })?;
        std::fs::write(path, json).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn name(&self) -> &str {
        &self.profile
    }

    pub fn entries(&self) -> &[PayloadEntry] {
        &self.entries
    }

    pub fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    pub fn confirmed_on_hardware(&self) -> bool {
        self.confirmed_on_hardware
    }

    /// What the UI must show when the layout has never been tested on a console,
    /// which today is every profile that exists.
    pub fn layout_caveat(&self) -> Option<&'static str> {
        if self.confirmed_on_hardware {
            None
        } else {
            Some(
                "This layout has not been tested on a console. Where the Xbox \
                 app reads its files from is not documented anywhere in the \
                 package, so the copy can land in exactly the right shape and \
                 still not be found by the app.",
            )
        }
    }

    pub fn file_count(&self) -> usize {
        self.entries.len()
    }

    /// Bytes the whole file set occupies once it is on the drive.
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    /// The largest single file, which decides how much room a staged
    /// replacement needs.
    pub fn largest_entry_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.size).max().unwrap_or(0)
    }
}

fn author_tree(
    dir: &Path,
    src_prefix: &str,
    dst_prefix: &str,
    entries: &mut Vec<PayloadEntry>,
    empty_dirs: &mut BTreeSet<String>,
) -> Result<()> {
    let read = std::fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut found_anything = false;

    for item in read {
        let item = item.map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let name = item.file_name().to_string_lossy().to_string();
        let path = item.path();
        let src = extend(src_prefix, &name);
        let dst = extend(dst_prefix, &name);

        let meta = std::fs::symlink_metadata(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;

        if meta.is_dir() {
            found_anything = true;
            author_tree(&path, &src, &dst, entries, empty_dirs)?;
            continue;
        }
        if !meta.is_file() {
            // A symlink or a device node in a payload is not something to follow
            // silently. Skipped here, and it shows up as a missing source if
            // anything expected it.
            continue;
        }
        if has_component(&dst, GC_DIR) {
            // The shipped payload's GC/ holds the author's real saves. It can
            // never enter a profile, so it can never be written. `found_anything`
            // is still set: the directory is not empty, it is just not ours.
            found_anything = true;
            continue;
        }
        if name.ends_with(STAGING_SUFFIX) {
            // Somebody else's interrupted copy, not payload content.
            found_anything = true;
            continue;
        }
        found_anything = true;
        let size = meta.len();
        let sha256 = if size == 0 {
            // Hashing an empty file is vacuous. Recording the digest anyway
            // would produce a verify line that cannot fail.
            None
        } else {
            Some(hash_file(&path)?)
        };
        entries.push(PayloadEntry {
            src,
            dst,
            sha256,
            size,
        });
    }

    if !found_anything && dst_prefix != "." {
        // Four directories in the payload are empty, and a copy driven by a file
        // list omits all four without a word. Whether the app needs them is
        // unknown, so they are recorded and created.
        empty_dirs.insert(dst_prefix.to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Choosing a drive
// ---------------------------------------------------------------------------

/// Paths the wizard must never write over, whatever the drive seam reports.
///
/// Comparison is case-insensitive on every platform. That can only ever produce
/// one extra refusal on a case-sensitive filesystem, never one extra write, and
/// the volume this actually runs against is usually exFAT.
#[derive(Debug, Clone)]
pub struct Guards {
    protected: Vec<PathBuf>,
}

impl Guards {
    /// The launcher's own roots, plus wherever this executable is running from.
    ///
    /// The seam reports removable volumes and nothing more: it has no field for
    /// "is this the boot volume". So the guard is expressed in terms of paths
    /// the launcher knows about, and that covers the case that actually happens,
    /// a portable install on the stick being written.
    pub fn for_platform(platform: &Platform) -> Result<Self> {
        let mut protected = vec![
            platform.paths.install_root()?,
            platform.paths.data_root()?,
            platform.paths.cache_root()?,
        ];
        if let Ok(exe) = std::env::current_exe() {
            protected.push(exe);
        }
        Ok(Self { protected })
    }

    pub fn protecting(protected: Vec<PathBuf>) -> Self {
        Self { protected }
    }

    /// The first protected path that lives on this volume, if any.
    fn violated_by(&self, mount_point: &Path) -> Option<PathBuf> {
        let mount = mount_point.to_string_lossy().to_lowercase();
        let mount = mount.trim_end_matches(['/', '\\']).to_string();
        if mount.is_empty() {
            return None;
        }
        self.protected
            .iter()
            .find(|p| {
                let candidate = p.to_string_lossy().to_lowercase();
                candidate == mount
                    || candidate.starts_with(&format!("{mount}/"))
                    || candidate.starts_with(&format!("{mount}\\"))
            })
            .cloned()
    }
}

/// Why a drive that was found will not be written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveRefusal {
    /// The launcher's own files are on this volume. A portable install on the
    /// stick the user is about to overwrite is the shape this catches.
    HoldsLauncherFiles { protected: PathBuf },
    /// The seam reported no capacity, so no space check is possible and the
    /// wizard will not write blind.
    CapacityUnknown,
    /// A filesystem the wizard cannot reason about. See [`filesystem_verdict`].
    FilesystemUnsupported { detected: Option<String> },
}

impl DriveRefusal {
    /// The greyed-out line in the picker. A drive that was found and not offered
    /// has to say why, or the user replugs it forever.
    pub fn message(&self) -> String {
        match self {
            DriveRefusal::HoldsLauncherFiles { protected } => format!(
                "NFL Street Reborn's own files are on this drive ({}). Copying \
                 the Xbox payload over them could break your PC install, so this \
                 drive is not offered. Use a different stick.",
                protected.display()
            ),
            DriveRefusal::CapacityUnknown => {
                "This drive reported no size, so there is no way to check that \
                 the payload fits before copying about 3 GB to it. Reconnect it \
                 and scan again."
                    .to_string()
            }
            DriveRefusal::FilesystemUnsupported { detected } => match detected {
                Some(name) => format!(
                    "This drive is formatted {name}. NFL Street Reborn cannot \
                     tell whether an Xbox can read it, and copying about 3 GB to \
                     find out wastes your time. Use an exFAT drive."
                ),
                None => "The filesystem on this drive could not be detected, so \
                     there is no way to tell whether an Xbox can read it. \
                     Reconnect it and scan again."
                    .to_string(),
            },
        }
    }
}

/// One drive, and whether it can be chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveOffer {
    pub drive: RemovableDrive,
    /// `None` means selectable. Refused drives stay in the survey rather than
    /// being filtered out, because a user whose stick is missing from the list
    /// needs to see that it was found at all.
    pub refusal: Option<DriveRefusal>,
}

impl DriveOffer {
    pub fn is_selectable(&self) -> bool {
        self.refusal.is_none()
    }

    pub fn why_not(&self) -> Option<String> {
        self.refusal.as_ref().map(DriveRefusal::message)
    }
}

/// What was found when the wizard looked for drives.
#[derive(Debug, Clone)]
pub struct DriveSurvey {
    pub offers: Vec<DriveOffer>,
}

impl DriveSurvey {
    pub fn selectable(&self) -> Vec<&DriveOffer> {
        self.offers.iter().filter(|o| o.is_selectable()).collect()
    }

    /// Drives that were found and will not be written to, with reasons.
    pub fn refused(&self) -> Vec<&DriveOffer> {
        self.offers.iter().filter(|o| !o.is_selectable()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.offers.is_empty()
    }

    // There is deliberately no `preselected` or `sole_drive` here. Section 7,
    // tightened: preselect nothing, ever, even when exactly one removable drive
    // is present. The extra confirmation costs one button press and makes the
    // rule checkable by inspection instead of by simulating drive counts.
    // [`choose`] is where it is enforced.
}

/// Ask the platform what removable drives exist, and classify each one.
///
/// Today every platform's drive seam is an honest stub, so this returns
/// [`Error::NotImplemented`]. That is the right behavior: an empty picker would
/// tell the user their stick is not plugged in, which is a different statement
/// and a false one. Everything downstream is reachable through [`survey_of`], so
/// the wizard's rules do not wait on the seam.
pub fn survey(platform: &Platform) -> Result<DriveSurvey> {
    let guards = Guards::for_platform(platform)?;
    let drives = platform.drives.enumerate()?;
    Ok(survey_of(drives, &guards))
}

/// Classify an already-enumerated set of drives.
pub fn survey_of(drives: Vec<RemovableDrive>, guards: &Guards) -> DriveSurvey {
    let offers = drives
        .into_iter()
        .map(|drive| {
            let refusal = if let Some(protected) = guards.violated_by(&drive.mount_point) {
                Some(DriveRefusal::HoldsLauncherFiles { protected })
            } else if drive.total_bytes == 0 {
                Some(DriveRefusal::CapacityUnknown)
            } else if !filesystem_verdict(drive.filesystem.as_deref()).allows_writing() {
                Some(DriveRefusal::FilesystemUnsupported {
                    detected: drive.filesystem.clone(),
                })
            } else {
                None
            };
            DriveOffer { drive, refusal }
        })
        .collect();
    DriveSurvey { offers }
}

/// Why the wizard will not start, expressed as a state the user is in rather
/// than a fault. Each one has a different thing to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No drive was chosen. Reached even when exactly one drive is present.
    NothingSelected,
    /// The path is not one of the removable drives that were found.
    NotRemovable { path: PathBuf },
    /// The drive was found and is not usable.
    DriveRefused { path: PathBuf, reason: DriveRefusal },
    /// Free space is short. Refused before a single byte is written.
    NotEnoughSpace { required: u64, free: u64 },
    /// The drive could not hold this payload even if it were empty.
    DriveTooSmall { required: u64, capacity: u64 },
    /// The payload on the PC does not match the profile, so the copy would be
    /// partial.
    PayloadIncomplete { missing: usize, first: String },
    /// A destination that already holds different content was not shown to the
    /// user before approval.
    OverwriteNotShown { dst: String },
}

impl Refusal {
    /// A sentence a player can act on. Every one names a next step.
    pub fn message(&self) -> String {
        match self {
            Refusal::NothingSelected => "Choose the drive to copy to. Nothing is \
                 selected for you, even when only one drive is plugged in, \
                 because this writes about 3 GB to whatever is chosen."
                .to_string(),
            Refusal::NotRemovable { path } => format!(
                "{} is not one of the removable drives that were found, so the \
                 wizard will not write to it. Plug in the Xbox USB drive and \
                 scan again.",
                path.display()
            ),
            Refusal::DriveRefused { path, reason } => {
                format!("{}: {}", path.display(), reason.message())
            }
            Refusal::NotEnoughSpace { required, free } => format!(
                "This drive has {} free and the copy needs {}, so it would run \
                 out partway through. Free up {} or use a larger drive. Nothing \
                 has been written.",
                describe_bytes(*free),
                describe_bytes(*required),
                describe_bytes(required.saturating_sub(*free))
            ),
            Refusal::DriveTooSmall { required, capacity } => format!(
                "This drive holds {} in total and the payload needs {} of room \
                 while it is being written, so it can never fit. Use a larger \
                 drive. Nothing has been written.",
                describe_bytes(*capacity),
                describe_bytes(*required)
            ),
            Refusal::PayloadIncomplete { missing, first } => format!(
                "{missing} file(s) this release names are missing or the wrong \
                 size on this PC, starting with {first:?}. Nothing has been \
                 written. Verify the library first, then try again."
            ),
            Refusal::OverwriteNotShown { dst } => format!(
                "{dst:?} already exists on the drive with different contents and \
                 was not shown to you, so nothing has been replaced. List every \
                 file the copy would overwrite before approving it."
            ),
        }
    }
}

/// A drive that passed the picker's rules. The only thing [`CopyPlan::build`]
/// accepts.
///
/// Constructible only by [`choose`], and that is the whole point: a caller
/// cannot hand a path to the copy engine and have it believed, so "did anybody
/// check this is a removable drive" becomes a question the compiler answers.
#[derive(Debug, Clone)]
pub struct Target {
    root: PathBuf,
    label: Option<String>,
    filesystem: Option<String>,
    free_bytes: u64,
    total_bytes: u64,
    identity: Option<String>,
}

impl Target {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
    pub fn filesystem(&self) -> Option<&str> {
        self.filesystem.as_deref()
    }
    pub fn free_bytes(&self) -> u64 {
        self.free_bytes
    }
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// The filename any resume state must be filed under.
    ///
    /// `None` when the platform reports no stable volume identity, and in that
    /// case resume is simply unavailable and the UI should say so. **It never
    /// falls back to the drive letter.** A letter is the one attribute
    /// guaranteed not to survive a replug, so keying on it means resuming onto
    /// whatever happened to come back as `E:`.
    pub fn journal_key(&self) -> Option<String> {
        self.identity
            .as_ref()
            .map(|id| format!("journal-{}.json", sanitize_key(id)))
    }

    pub fn filesystem_verdict(&self) -> FilesystemVerdict {
        filesystem_verdict(self.filesystem.as_deref())
    }

    /// What the wizard is allowed to promise about being interrupted.
    pub fn crash_safety(&self) -> CrashSafety {
        match self
            .filesystem
            .as_deref()
            .map(str::to_ascii_uppercase)
            .as_deref()
        {
            Some("NTFS") => CrashSafety::StagedRenameExpectedAtomic,
            _ => CrashSafety::NoClaim,
        }
    }

    /// Whether the drive still appears to be attached.
    ///
    /// Cheap, and checked between files. The failure it catches is the drive
    /// being pulled out, which is the normal way a 3 GB copy to a USB stick ends
    /// badly.
    pub fn is_attached(&self) -> bool {
        self.root.is_dir()
    }
}

/// Turn a survey plus a deliberate choice into a target.
///
/// `selected` is an `Option` so that "the user has not chosen yet" is a value
/// rather than a special case, and so the no-preselection rule lives in one
/// place: `None` is refused even when exactly one drive is selectable.
pub fn choose(
    survey: &DriveSurvey,
    selected: Option<&Path>,
) -> std::result::Result<Target, Refusal> {
    let Some(selected) = selected else {
        return Err(Refusal::NothingSelected);
    };
    let wanted = selected.to_string_lossy().to_lowercase();
    let wanted = wanted.trim_end_matches(['/', '\\']).to_string();

    let offer = survey.offers.iter().find(|o| {
        let mount = o.drive.mount_point.to_string_lossy().to_lowercase();
        mount.trim_end_matches(['/', '\\']) == wanted
    });

    let Some(offer) = offer else {
        return Err(Refusal::NotRemovable {
            path: selected.to_path_buf(),
        });
    };
    if let Some(reason) = &offer.refusal {
        return Err(Refusal::DriveRefused {
            path: selected.to_path_buf(),
            reason: reason.clone(),
        });
    }
    Ok(Target {
        root: offer.drive.mount_point.clone(),
        label: offer.drive.label.clone(),
        filesystem: offer.drive.filesystem.clone(),
        free_bytes: offer.drive.free_bytes,
        total_bytes: offer.drive.total_bytes,
        identity: offer.drive.volume_identity.clone(),
    })
}

/// What the wizard does with the filesystem it detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesystemVerdict {
    /// Write, with a note about what is and is not confirmed.
    Proceed { note: &'static str },
    /// Write, with a caution the user should read first. Still not a diagnosis.
    Caution { note: &'static str },
    /// Do not write.
    Refuse { note: &'static str },
}

impl FilesystemVerdict {
    pub fn allows_writing(&self) -> bool {
        !matches!(self, FilesystemVerdict::Refuse { .. })
    }
    pub fn note(&self) -> &'static str {
        match self {
            FilesystemVerdict::Proceed { note }
            | FilesystemVerdict::Caution { note }
            | FilesystemVerdict::Refuse { note } => note,
        }
    }
}

/// Section 8: the filesystem is read before the first byte is written.
///
/// Nobody knows which filesystem the console requires. Open item 3 is BLOCKING
/// and unanswered, so this does not refuse a drive on a guess about the console:
/// the three filesystems the design material actually shows are allowed with
/// honest notes, and anything the wizard cannot reason about is refused with the
/// reason, because copying 3.01 GB to find out costs the user an afternoon.
pub fn filesystem_verdict(filesystem: Option<&str>) -> FilesystemVerdict {
    match filesystem.map(str::to_ascii_uppercase).as_deref() {
        Some("EXFAT") => FilesystemVerdict::Proceed {
            note: "exFAT. This is what the sample drives in the design material \
                   use, but it is not confirmed against a console.",
        },
        Some("NTFS") => FilesystemVerdict::Caution {
            note: "NTFS. Some consoles and embedded readers do not mount NTFS. \
                   If the Xbox does not see the drive, exFAT is the next thing \
                   to try.",
        },
        Some("FAT32") => FilesystemVerdict::Caution {
            note: "FAT32. The payload fits, since the largest file is 1.36 GiB \
                   against a 4 GiB limit, but 2,782 files in one flat directory \
                   is heavy for FAT32 and console support is unconfirmed.",
        },
        _ => FilesystemVerdict::Refuse {
            note: "The wizard cannot tell whether an Xbox can read this \
                   filesystem, and copying about 3 GB to find out wastes your \
                   time. exFAT is what the design material uses.",
        },
    }
}

/// What the wizard may say about interrupting a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashSafety {
    /// A staged rename is expected to be all or nothing, so the UI may say the
    /// copy is safe to interrupt.
    StagedRenameExpectedAtomic,
    /// No claim is made. Staging, flushing, verifying and renaming all still
    /// happen, because they cost nothing extra and are correct where they help,
    /// but exFAT and FAT32 have no metadata journal and cheap USB controllers are
    /// known not to always honor cache flushes. The engine does not change per
    /// filesystem; only the promise does.
    NoClaim,
}

impl CrashSafety {
    pub fn note(self) -> &'static str {
        match self {
            CrashSafety::StagedRenameExpectedAtomic => {
                "Each file is written to a temporary name and renamed into place, \
                 which on this filesystem is expected to be all or nothing."
            }
            CrashSafety::NoClaim => {
                "Do not unplug the drive during the copy. On this filesystem the \
                 launcher cannot promise that an interrupted write leaves either \
                 the old file or the new one intact."
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// One file the copy will write, and whether something is already there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWrite {
    pub src: String,
    pub dst: String,
    pub size: u64,
    pub sha256: Option<Sha256Hex>,
    pub replaces_existing: bool,
}

/// Why a destination that already exists is going to be written anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteReason {
    /// The file on the drive is not the file this release ships.
    ContentDiffers,
    /// The profile carries no digest, so the file on the drive cannot be
    /// compared at all. Treated as an overwrite, because "the same length" is
    /// not evidence of being the same file.
    CannotBeChecked,
}

/// A file already on the drive that the copy would replace.
///
/// Every one of these has to be shown to the user before [`CopyPlan::approve`]
/// will hand back something [`copy`] accepts. The wizard cannot tell its own
/// previous release's file from one the user put there by hand, because no
/// manifest history ships yet, so it does not guess which of these matter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overwrite {
    pub dst: String,
    pub existing_bytes: u64,
    pub existing_sha256: Sha256Hex,
    pub reason: OverwriteReason,
}

/// Everything the copy will do, decided before anything is written.
#[derive(Debug, Clone)]
pub struct CopyPlan {
    target: Target,
    payload_root: PathBuf,
    profile_name: String,
    confirmed_on_hardware: bool,
    to_write: Vec<PlannedWrite>,
    already_current: Vec<String>,
    overwrites: Vec<Overwrite>,
    dirs_to_create: Vec<String>,
    missing_sources: Vec<String>,
    profile_total_bytes: u64,
    profile_largest_bytes: u64,
}

impl CopyPlan {
    /// Work out what would be written, reading both sides.
    ///
    /// Sources are checked for presence and length only. Hashing 1.5 GB of
    /// payload here would double the cost of the whole operation to answer a
    /// question the read-back verify after each write answers anyway.
    ///
    /// Destinations that already exist *are* hashed, because that is the only
    /// way to know whether a file needs writing and the only way to tell the
    /// user honestly that something is about to be replaced. Section 9.2 caches
    /// this in the PC-side journal to make repeat updates cheap; there is no
    /// journal yet, so this is slow and correct rather than fast and trusting.
    pub fn build(
        target: Target,
        profile: &DestinationProfile,
        payload_root: &Path,
    ) -> Result<Self> {
        let mut to_write = Vec::new();
        let mut already_current = Vec::new();
        let mut overwrites = Vec::new();
        let mut missing_sources = Vec::new();

        for entry in &profile.entries {
            let src = join_relative(payload_root, &entry.src);
            match std::fs::metadata(&src) {
                Ok(meta) if meta.is_file() && meta.len() == entry.size => {}
                _ => {
                    missing_sources.push(entry.src.clone());
                    continue;
                }
            }

            let dst = join_relative(target.root(), &entry.dst);
            let existing = std::fs::metadata(&dst).ok().filter(|m| m.is_file());

            let (needs_write, replaces, overwrite) = match (&existing, &entry.sha256) {
                (None, _) => (true, false, None),
                (Some(meta), Some(expected)) => {
                    let actual = hash_file(&dst)?;
                    if &actual == expected {
                        (false, true, None)
                    } else {
                        (
                            true,
                            true,
                            Some(Overwrite {
                                dst: entry.dst.clone(),
                                existing_bytes: meta.len(),
                                existing_sha256: actual,
                                reason: OverwriteReason::ContentDiffers,
                            }),
                        )
                    }
                }
                // Both empty. There is nothing a copy could improve, and an
                // empty file has no contents that could differ.
                (Some(meta), None) if entry.size == 0 && meta.len() == 0 => (false, true, None),
                (Some(meta), None) => (
                    true,
                    true,
                    Some(Overwrite {
                        dst: entry.dst.clone(),
                        existing_bytes: meta.len(),
                        existing_sha256: hash_file(&dst)?,
                        reason: OverwriteReason::CannotBeChecked,
                    }),
                ),
            };

            if let Some(o) = overwrite {
                overwrites.push(o);
            }
            if needs_write {
                to_write.push(PlannedWrite {
                    src: entry.src.clone(),
                    dst: entry.dst.clone(),
                    size: entry.size,
                    sha256: entry.sha256.clone(),
                    replaces_existing: replaces,
                });
            } else {
                already_current.push(entry.dst.clone());
            }
        }

        Ok(Self {
            target,
            payload_root: payload_root.to_path_buf(),
            profile_name: profile.profile.clone(),
            confirmed_on_hardware: profile.confirmed_on_hardware,
            to_write,
            already_current,
            overwrites,
            dirs_to_create: profile.empty_dirs.clone(),
            missing_sources,
            profile_total_bytes: profile.total_bytes(),
            profile_largest_bytes: profile.largest_entry_bytes(),
        })
    }

    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }
    pub fn confirmed_on_hardware(&self) -> bool {
        self.confirmed_on_hardware
    }
    pub fn to_write(&self) -> &[PlannedWrite] {
        &self.to_write
    }
    /// Destinations already holding exactly the right bytes. This is what makes
    /// an update cheap: a release that changes 40 textures copies 40 files, not
    /// 2,798.
    pub fn already_current(&self) -> &[String] {
        &self.already_current
    }
    pub fn overwrites(&self) -> &[Overwrite] {
        &self.overwrites
    }
    pub fn dirs_to_create(&self) -> &[String] {
        &self.dirs_to_create
    }
    /// Files the profile names that are missing or the wrong size on this PC.
    pub fn missing_sources(&self) -> &[String] {
        &self.missing_sources
    }

    pub fn bytes_to_write(&self) -> u64 {
        self.to_write.iter().map(|w| w.size).sum()
    }

    /// Free space the drive must have before the copy starts.
    ///
    /// This is [`CopyPlan::bytes_to_write`] plus a margin, and the staging peak
    /// is already inside it rather than added on top. The sum charges every
    /// replacement's *new* bytes against free space and never credits back the
    /// bytes of the old file it replaces, so the one moment where a `.rbn-part`
    /// and the file it will replace both exist is already paid for.
    ///
    /// Section 8's 4,473,573,242 figure is a different quantity. It is the
    /// *capacity* a drive needs to hold the finished install and stage the
    /// largest file, which is [`CopyPlan::required_capacity_bytes`]. Reading it
    /// as a free-space requirement rejects perfectly good sticks.
    pub fn required_free_bytes(&self) -> u64 {
        self.bytes_to_write().saturating_add(STAGING_MARGIN_BYTES)
    }

    /// Total drive size needed for this payload to be installable at all.
    pub fn required_capacity_bytes(&self) -> u64 {
        self.profile_total_bytes
            .saturating_add(self.profile_largest_bytes)
            .saturating_add(STAGING_MARGIN_BYTES)
    }

    /// Everything that has to be true before a byte is written.
    ///
    /// Returns the only value [`copy`] accepts. `shown_overwrites` is the list of
    /// destination paths the caller actually displayed: passing an empty slice
    /// while the plan replaces files is refused. That is what "never overwriting
    /// an unrelated file without saying so" looks like when it is enforced
    /// instead of documented.
    pub fn approve(
        &self,
        shown_overwrites: &[&str],
    ) -> std::result::Result<ApprovedPlan<'_>, Refusal> {
        if !self.target.filesystem_verdict().allows_writing() {
            return Err(Refusal::DriveRefused {
                path: self.target.root.clone(),
                reason: DriveRefusal::FilesystemUnsupported {
                    detected: self.target.filesystem.clone(),
                },
            });
        }
        if let Some(first) = self.missing_sources.first() {
            return Err(Refusal::PayloadIncomplete {
                missing: self.missing_sources.len(),
                first: first.clone(),
            });
        }
        if self.target.total_bytes < self.required_capacity_bytes() {
            return Err(Refusal::DriveTooSmall {
                required: self.required_capacity_bytes(),
                capacity: self.target.total_bytes,
            });
        }
        if self.target.free_bytes < self.required_free_bytes() {
            return Err(Refusal::NotEnoughSpace {
                required: self.required_free_bytes(),
                free: self.target.free_bytes,
            });
        }
        let shown: BTreeSet<String> = shown_overwrites.iter().map(|s| s.to_lowercase()).collect();
        for o in &self.overwrites {
            if !shown.contains(&o.dst.to_lowercase()) {
                return Err(Refusal::OverwriteNotShown { dst: o.dst.clone() });
            }
        }
        Ok(ApprovedPlan { plan: self })
    }
}

/// A plan whose filesystem, payload, space and overwrites have all been settled.
#[derive(Debug, Clone)]
pub struct ApprovedPlan<'p> {
    plan: &'p CopyPlan,
}

impl ApprovedPlan<'_> {
    pub fn plan(&self) -> &CopyPlan {
        self.plan
    }
}

// ---------------------------------------------------------------------------
// The copy
// ---------------------------------------------------------------------------

/// How far along a copy is, both ways.
///
/// Both fractions are reported because neither one alone tells the truth. The
/// payload is one 1.36 GB image and 2,782 texture files, so a bar driven by
/// bytes sits almost still for several minutes after the image lands, and a bar
/// driven by file count jumps to 99% and then stops dead on the image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl Progress {
    pub fn by_files(&self) -> f64 {
        if self.files_total == 0 {
            return 1.0;
        }
        self.files_done as f64 / self.files_total as f64
    }

    pub fn by_bytes(&self) -> f64 {
        if self.bytes_total == 0 {
            return 1.0;
        }
        self.bytes_done as f64 / self.bytes_total as f64
    }

    /// The number to put on a bar: the mean of the two, so it keeps moving
    /// through the one huge file and through the long tail of small ones.
    pub fn fraction(&self) -> f64 {
        (self.by_files() + self.by_bytes()) / 2.0
    }
}

/// What a finished or abandoned copy actually did.
#[derive(Debug, Clone, Default)]
pub struct CopyReport {
    pub dirs_created: Vec<String>,
    pub written: Vec<String>,
    pub skipped_already_current: Vec<String>,
    pub hash_verified: usize,
    pub presence_verified: usize,
    pub length_only: usize,
    pub bytes_written: u64,
}

impl CopyReport {
    /// A line that says what was checked and what was not, because five config
    /// files are zero bytes and a digest match on an empty file is a green check
    /// that cannot fail.
    pub fn summary(&self) -> String {
        format!(
            "{} files written, {} verified by SHA-256, {} zero-byte files \
             verified by presence and length, {} written with no digest to check \
             against, {} already current and skipped, {} empty directories \
             created",
            self.written.len(),
            self.hash_verified,
            self.presence_verified,
            self.length_only,
            self.skipped_already_current.len(),
            self.dirs_created.len()
        )
    }
}

/// Why a copy stopped early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Interruption {
    /// The drive is no longer there. The expected way this operation fails.
    DriveWentAway {
        root: PathBuf,
        remaining_files: usize,
        remaining_bytes: u64,
    },
    /// A write failed while the drive was still attached.
    WriteFailed {
        dst: String,
        detail: String,
        remaining_files: usize,
    },
    /// What landed did not match the digest, twice.
    VerifyFailedTwice {
        dst: String,
        expected: Sha256Hex,
        actual: Sha256Hex,
    },
}

impl Interruption {
    pub fn message(&self) -> String {
        match self {
            Interruption::DriveWentAway {
                root,
                remaining_files,
                remaining_bytes,
            } => format!(
                "The drive at {} stopped responding partway through, with \
                 {remaining_files} file(s) and {} still to copy. Plug it back in \
                 and run the copy again: files that are already correct are \
                 skipped, so it picks up rather than starting over.",
                root.display(),
                describe_bytes(*remaining_bytes)
            ),
            Interruption::WriteFailed {
                dst,
                detail,
                remaining_files,
            } => format!(
                "Writing {dst:?} failed and {remaining_files} file(s) were not \
                 copied: {detail}. The drive is still connected, so this is more \
                 likely a full or failing drive than a loose cable. Nothing that \
                 was already written was damaged."
            ),
            Interruption::VerifyFailedTwice {
                dst,
                expected,
                actual,
            } => format!(
                "{dst:?} did not match its checksum after two attempts (expected \
                 {expected}, got {actual}), so it was not put into place and the \
                 file that was already there is untouched. This usually means a \
                 failing USB drive: try another one."
            ),
        }
    }
}

/// The end of a copy.
#[derive(Debug, Clone)]
pub enum CopyOutcome {
    Complete(CopyReport),
    /// Stopped early. The report of what did land is deliberately kept: it is
    /// what a resume would be built from, and it is the only thing that can tell
    /// the user how far they got.
    Interrupted {
        report: CopyReport,
        cause: Interruption,
    },
}

impl CopyOutcome {
    pub fn report(&self) -> &CopyReport {
        match self {
            CopyOutcome::Complete(r) => r,
            CopyOutcome::Interrupted { report, .. } => report,
        }
    }
    pub fn is_complete(&self) -> bool {
        matches!(self, CopyOutcome::Complete(_))
    }
}

/// What a single successful write proved.
enum Landed {
    Hash,
    PresenceAndLength,
    LengthOnly,
}

enum WriteFailure {
    Io(String),
    Mismatch {
        expected: Sha256Hex,
        actual: Sha256Hex,
    },
}

/// Move the files.
///
/// Per file: stage under [`STAGING_SUFFIX`], flush, verify what landed by
/// reading it back, then rename into place. Reading it back is the point:
/// hashing the bytes handed to the operating system proves the source was read
/// correctly and says nothing about what reached the stick.
///
/// A drive pulled out mid-copy is a return value, not an error, and it is
/// checked after every file. Without that check the loop keeps going and writes
/// the remaining 2,700 files into a directory that is no longer a drive.
pub fn copy(
    platform: &Platform,
    approved: &ApprovedPlan<'_>,
    progress: &mut dyn FnMut(&Progress),
) -> Result<CopyOutcome> {
    let plan = approved.plan;
    let mut report = CopyReport::default();
    let files_total = plan.to_write.len();
    let bytes_total = plan.bytes_to_write();
    let mut files_done = 0usize;
    let mut bytes_done = 0u64;

    // Empty directories first. Four of them exist in the payload, a copy driven
    // by a file list omits all four, and doing them first means a drive that is
    // already gone is found before any bytes move.
    for dir in &plan.dirs_to_create {
        let path = join_relative(plan.target.root(), dir);
        if let Err(source) = std::fs::create_dir_all(&path) {
            let cause = if plan.target.is_attached() {
                Interruption::WriteFailed {
                    dst: dir.clone(),
                    detail: format!("{}: {source}", path.display()),
                    remaining_files: files_total,
                }
            } else {
                Interruption::DriveWentAway {
                    root: plan.target.root.clone(),
                    remaining_files: files_total,
                    remaining_bytes: bytes_total,
                }
            };
            return Ok(CopyOutcome::Interrupted { report, cause });
        }
        report.dirs_created.push(dir.clone());
    }

    for w in &plan.to_write {
        let src = join_relative(&plan.payload_root, &w.src);
        let dst = join_relative(plan.target.root(), &w.dst);

        // One retry, then give up on this file. A second failure on removable
        // media means a failing drive rather than bad luck, and hammering it
        // does not help.
        let landed = match attempt_write(platform, &src, &dst, w) {
            Ok(v) => Ok(v),
            Err(_first) => attempt_write(platform, &src, &dst, w),
        };

        match landed {
            Ok(Landed::Hash) => report.hash_verified += 1,
            Ok(Landed::PresenceAndLength) => report.presence_verified += 1,
            Ok(Landed::LengthOnly) => report.length_only += 1,
            Err(WriteFailure::Mismatch { expected, actual }) => {
                return Ok(CopyOutcome::Interrupted {
                    report,
                    cause: Interruption::VerifyFailedTwice {
                        dst: w.dst.clone(),
                        expected,
                        actual,
                    },
                });
            }
            Err(WriteFailure::Io(detail)) => {
                let remaining_files = files_total.saturating_sub(files_done);
                let cause = if plan.target.is_attached() {
                    Interruption::WriteFailed {
                        dst: w.dst.clone(),
                        detail,
                        remaining_files,
                    }
                } else {
                    Interruption::DriveWentAway {
                        root: plan.target.root.clone(),
                        remaining_files,
                        remaining_bytes: bytes_total.saturating_sub(bytes_done),
                    }
                };
                return Ok(CopyOutcome::Interrupted { report, cause });
            }
        }

        report.written.push(w.dst.clone());
        report.bytes_written = report.bytes_written.saturating_add(w.size);
        files_done += 1;
        bytes_done = bytes_done.saturating_add(w.size);

        let p = Progress {
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        };
        progress(&p);

        // Checked between files, not only when a write fails. A drive pulled out
        // after a successful write is otherwise noticed only when the next open
        // fails, and an open against a stale mount can succeed and write into
        // nothing.
        if files_done < files_total && !plan.target.is_attached() {
            return Ok(CopyOutcome::Interrupted {
                report,
                cause: Interruption::DriveWentAway {
                    root: plan.target.root.clone(),
                    remaining_files: files_total.saturating_sub(files_done),
                    remaining_bytes: bytes_total.saturating_sub(bytes_done),
                },
            });
        }
    }

    report.skipped_already_current = plan.already_current.clone();
    Ok(CopyOutcome::Complete(report))
}

fn attempt_write(
    platform: &Platform,
    src: &Path,
    dst: &Path,
    w: &PlannedWrite,
) -> std::result::Result<Landed, WriteFailure> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| WriteFailure::Io(format!("{}: {e}", parent.display())))?;
    }
    let part = staging_path(dst);

    {
        let mut reader = std::fs::File::open(src)
            .map_err(|e| WriteFailure::Io(format!("{}: {e}", src.display())))?;
        let mut writer = std::fs::File::create(&part)
            .map_err(|e| WriteFailure::Io(format!("{}: {e}", part.display())))?;
        std::io::copy(&mut reader, &mut writer)
            .map_err(|e| WriteFailure::Io(format!("{}: {e}", part.display())))?;
        // Worth the flush: the next thing that happens is a rename that makes
        // this file look finished.
        writer
            .sync_all()
            .map_err(|e| WriteFailure::Io(format!("{}: {e}", part.display())))?;
    }

    let landed_len = std::fs::metadata(&part)
        .map(|m| m.len())
        .map_err(|e| WriteFailure::Io(format!("{}: {e}", part.display())))?;
    if landed_len != w.size {
        let _ = std::fs::remove_file(&part);
        return Err(WriteFailure::Io(format!(
            "{landed_len} bytes landed on the drive, this release says {}",
            w.size
        )));
    }

    let verified = match (w.size, &w.sha256) {
        // A digest of an empty file matches trivially, so presence and length is
        // the whole of the honest check.
        (0, _) => Landed::PresenceAndLength,
        (_, Some(expected)) => {
            let actual = hash_file(&part).map_err(|e| WriteFailure::Io(e.to_string()))?;
            if &actual != expected {
                // The staged file is removed and the file already in place is
                // left alone. A verified-bad copy must not replace a good one.
                let _ = std::fs::remove_file(&part);
                return Err(WriteFailure::Mismatch {
                    expected: expected.clone(),
                    actual,
                });
            }
            Landed::Hash
        }
        (_, None) => Landed::LengthOnly,
    };

    platform
        .files
        .atomic_replace(&part, dst)
        .map_err(|e| WriteFailure::Io(e.to_string()))?;
    Ok(verified)
}

fn staging_path(dst: &Path) -> PathBuf {
    let mut name = dst
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(STAGING_SUFFIX);
    dst.with_file_name(name)
}

// ---------------------------------------------------------------------------
// Verification after the copy
// ---------------------------------------------------------------------------

/// A file on the drive that this release does not name.
///
/// Always left alone. Telling "content the launcher shipped and has since
/// dropped" from "content the user added" needs the manifest history section 9.5
/// requires, and nothing ships it yet, so the safe class is the only class:
/// foreign, reported, untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurplusFile {
    pub dst: String,
    pub bytes: u64,
}

/// What verification actually checked.
#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub profile: String,
    pub confirmed_on_hardware: bool,
    pub hash_verified: usize,
    pub presence_verified: usize,
    pub length_only: usize,
    pub mismatched: Vec<String>,
    pub missing: Vec<String>,
    pub empty_dirs_present: usize,
    pub empty_dirs_absent: Vec<String>,
    /// Files under directories this profile writes into that the profile does
    /// not name. Counted exactly; the list is capped.
    pub surplus: Vec<SurplusFile>,
    pub surplus_count: usize,
    pub surplus_truncated: bool,
    /// Leftover staged writes from an interrupted copy.
    pub leftover_staging: Vec<String>,
    pub crash_safety: CrashSafety,
}

impl VerifyReport {
    /// Whether every file this release names is on the drive with the right
    /// contents.
    ///
    /// Deliberately not called `is_healthy`. A drive can satisfy this and still
    /// be wrong, which is why [`VerifyReport::summary`] always prints the
    /// surplus count beside the mismatch count: "0 mismatches" over a manifest
    /// is not "this drive matches this release".
    pub fn matches_release(&self) -> bool {
        self.mismatched.is_empty() && self.missing.is_empty()
    }

    /// The report, in the shape section 9.4 specifies. Honest is more useful
    /// than green.
    pub fn summary(&self) -> String {
        let mut lines = vec![
            format!("{} files verified by SHA-256", self.hash_verified),
            format!(
                "{} zero-byte files verified by presence and length",
                self.presence_verified
            ),
        ];
        if self.length_only > 0 {
            lines.push(format!(
                "{} files this release ships no digest for, checked by length only",
                self.length_only
            ));
        }
        lines.push(format!("{} mismatches", self.mismatched.len()));
        lines.push(format!("{} missing", self.missing.len()));
        lines.push(format!(
            "{} empty directories present, {} absent",
            self.empty_dirs_present,
            self.empty_dirs_absent.len()
        ));
        lines.push(format!(
            "{} files on the drive are not in this release, left in place",
            self.surplus_count
        ));
        if self.surplus_truncated {
            lines.push(format!(
                "only the first {} of them are listed",
                self.surplus.len()
            ));
        }
        if !self.leftover_staging.is_empty() {
            lines.push(format!(
                "{} unfinished {} files from an interrupted copy",
                self.leftover_staging.len(),
                STAGING_SUFFIX
            ));
        }
        if !self.confirmed_on_hardware {
            lines.push(
                "this layout has not been tested on a console, so a clean report \
                 does not prove the Xbox will find these files"
                    .to_string(),
            );
        }
        lines.push(match self.crash_safety {
            CrashSafety::StagedRenameExpectedAtomic => {
                "staged renames are expected to be atomic on this filesystem".to_string()
            }
            CrashSafety::NoClaim => "no crash-safety claim is made for this filesystem".to_string(),
        });
        lines.join("\n")
    }
}

/// Check a drive against a profile.
///
/// Used after a copy, and on its own as the Repair pass. It reads; it never
/// writes, renames or deletes, including the leftover staged files it reports.
pub fn verify(target: &Target, profile: &DestinationProfile) -> Result<VerifyReport> {
    let mut report = VerifyReport {
        profile: profile.profile.clone(),
        confirmed_on_hardware: profile.confirmed_on_hardware,
        hash_verified: 0,
        presence_verified: 0,
        length_only: 0,
        mismatched: Vec::new(),
        missing: Vec::new(),
        empty_dirs_present: 0,
        empty_dirs_absent: Vec::new(),
        surplus: Vec::new(),
        surplus_count: 0,
        surplus_truncated: false,
        leftover_staging: Vec::new(),
        crash_safety: target.crash_safety(),
    };

    for entry in &profile.entries {
        let path = join_relative(target.root(), &entry.dst);
        let meta = match std::fs::metadata(&path) {
            Ok(m) if m.is_file() => m,
            _ => {
                report.missing.push(entry.dst.clone());
                continue;
            }
        };
        if meta.len() != entry.size {
            report.mismatched.push(entry.dst.clone());
            continue;
        }
        match entry.verified_by() {
            VerifiedBy::PresenceAndLength => report.presence_verified += 1,
            VerifiedBy::LengthOnly => report.length_only += 1,
            VerifiedBy::Sha256 => match &entry.sha256 {
                Some(expected) if &hash_file(&path)? == expected => report.hash_verified += 1,
                _ => report.mismatched.push(entry.dst.clone()),
            },
        }
    }

    for dir in &profile.empty_dirs {
        if join_relative(target.root(), dir).is_dir() {
            report.empty_dirs_present += 1;
        } else {
            report.empty_dirs_absent.push(dir.clone());
        }
    }

    let managed: BTreeSet<String> = profile
        .entries
        .iter()
        .map(|e| e.dst.to_lowercase())
        .collect();
    let mut found: BTreeMap<String, u64> = BTreeMap::new();
    for dir in owned_dirs(profile) {
        let root = join_relative(target.root(), &dir);
        if root.is_dir() {
            collect_files(&root, &dir, &mut found);
        }
    }
    for (path, bytes) in found {
        if path.ends_with(STAGING_SUFFIX) {
            report.leftover_staging.push(path);
            continue;
        }
        if managed.contains(&path.to_lowercase()) {
            continue;
        }
        report.surplus_count += 1;
        if report.surplus.len() < MAX_SURPLUS_LISTED {
            report.surplus.push(SurplusFile { dst: path, bytes });
        } else {
            report.surplus_truncated = true;
        }
    }

    Ok(report)
}

/// The directories this profile writes into, with nested ones folded into their
/// parents so nothing is walked twice.
fn owned_dirs(profile: &DestinationProfile) -> Vec<String> {
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    for entry in &profile.entries {
        dirs.insert(parent_of(&entry.dst));
    }
    if dirs.contains(".") {
        return vec![".".to_string()];
    }
    let all: Vec<String> = dirs.into_iter().collect();
    all.iter()
        .filter(|d| {
            let child = format!("{}/", d.to_lowercase());
            !all.iter().any(|other| {
                other.as_str() != d.as_str()
                    && child.starts_with(&format!("{}/", other.to_lowercase()))
            })
        })
        .cloned()
        .collect()
}

/// Files under `dir`, keyed by their drive-relative path.
///
/// `GC/` is skipped at any depth. It is outside the surplus classification
/// entirely, so a memory card can never appear on a list of files somebody might
/// later be offered the chance to remove.
///
/// A directory that cannot be read is skipped rather than failing the whole
/// report. This runs on a drive the user may be unplugging, and a partial report
/// that says what it found beats no report at all.
fn collect_files(dir: &Path, prefix: &str, out: &mut BTreeMap<String, u64>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        if name.to_lowercase() == GC_DIR {
            continue;
        }
        let relative = extend(prefix, &name);
        let path = item.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            collect_files(&path, &relative, out);
        } else if meta.is_file() {
            out.insert(relative, meta.len());
        }
    }
}

// ---------------------------------------------------------------------------
// Finishing
// ---------------------------------------------------------------------------

/// What happened when the wizard tried to make the drive safe to unplug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EjectOutcome {
    Ejected,
    /// Windows Search, an antivirus scanner, Explorer or a thumbnail service
    /// holds a handle. Common after a large copy.
    Refused {
        reason: String,
    },
}

impl EjectOutcome {
    /// The message for the last screen.
    ///
    /// **Eject failing is not the copy failing.** Every file was flushed and
    /// checked before this was attempted, so a refusal is reassurance plus a
    /// retry. After writing 2,798 files the user must not be told only "could
    /// not eject".
    pub fn message(&self) -> String {
        match self {
            EjectOutcome::Ejected => {
                "The drive has been ejected and is safe to unplug.".to_string()
            }
            EjectOutcome::Refused { reason } => format!(
                "Windows would not release the drive yet ({reason}). This is \
                 normal after a large copy, and every file is already written \
                 and checked. Wait a moment and try again, or use Windows' own \
                 Eject when it lets go."
            ),
        }
    }
}

/// Try to eject.
///
/// Never returns an error: by the time this runs the copy is finished and
/// verified, so there is nothing left that failing to eject could invalidate.
pub fn eject(platform: &Platform, target: &Target) -> EjectOutcome {
    match platform.drives.eject(target.root()) {
        Ok(()) => EjectOutcome::Ejected,
        Err(e) => EjectOutcome::Refused {
            reason: e.to_string(),
        },
    }
}

// ---------------------------------------------------------------------------
// Path handling
// ---------------------------------------------------------------------------

/// Join a forward-slash relative path onto a root, one component at a time.
///
/// Never `root.join(relative)`: a `relative` that turned out to be absolute
/// would silently replace the root, and on Windows so would one beginning with a
/// drive letter. [`check_relative_file`] rejects both on load, and this keeps the
/// guarantee even if a path reaches here some other way.
fn join_relative(root: &Path, relative: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        path.push(part);
    }
    path
}

fn extend(prefix: &str, name: &str) -> String {
    if prefix == "." || prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn parent_of(relative: &str) -> String {
    match relative.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => ".".to_string(),
    }
}

fn has_component(relative: &str, lowercase_name: &str) -> bool {
    relative
        .split('/')
        .any(|part| part.to_lowercase() == lowercase_name)
}

fn check_relative_file(field: &str, value: &str) -> std::result::Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} is empty."));
    }
    if value.contains('\\') {
        return Err(format!(
            "{field} {value:?} uses a backslash. Profiles are written with \
             forward slashes so one profile file works on every platform."
        ));
    }
    if value.contains(':') || value.starts_with('/') {
        return Err(format!(
            "{field} {value:?} is not relative. Every path in a profile is \
             relative to the payload root or the drive root, and an absolute one \
             would write outside both."
        ));
    }
    for part in value.split('/') {
        if part.is_empty() {
            return Err(format!("{field} {value:?} has an empty path component."));
        }
        if part == ".." || part == "." {
            return Err(format!(
                "{field} {value:?} contains {part:?}, which could climb out of \
                 the drive root."
            ));
        }
        if part != part.trim() {
            return Err(format!(
                "{field} {value:?} has a component padded with whitespace, which \
                 does not survive every filesystem."
            ));
        }
    }
    Ok(())
}

fn check_relative_dir(field: &str, value: &str) -> std::result::Result<(), String> {
    if value == "." {
        return Ok(());
    }
    check_relative_file(field, value)
}

/// Make a volume identity safe to use inside a filename without losing it.
fn sanitize_key(identity: &str) -> String {
    identity
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Bytes as decimal GB, with the exact byte count kept.
///
/// Both, always. Three different figures for this payload are circulating in the
/// design documents because somebody rounded once, so the exact number travels
/// with the friendly one.
pub fn describe_bytes(bytes: u64) -> String {
    if bytes < 1_000_000 {
        format!("{bytes} bytes")
    } else {
        format!("{:.2} GB ({bytes} bytes)", bytes as f64 / 1e9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIG: u64 = 64_000_000_000;

    fn drive(root: &Path, free: u64, total: u64, filesystem: &str) -> RemovableDrive {
        RemovableDrive {
            mount_point: root.to_path_buf(),
            label: Some("XBOX-USB".to_string()),
            total_bytes: total,
            free_bytes: free,
            filesystem: Some(filesystem.to_string()),
            volume_identity: Some("{2f8a-9c11}".to_string()),
        }
    }

    fn target_at(root: &Path, free: u64, total: u64, filesystem: &str) -> Target {
        let survey = survey_of(
            vec![drive(root, free, total, filesystem)],
            &Guards::protecting(Vec::new()),
        );
        choose(&survey, Some(root)).expect("a plain removable drive must be selectable")
    }

    /// Lay down a payload on the PC side.
    fn write_payload(root: &Path, files: &[(&str, &[u8])]) {
        for (relative, bytes) in files {
            let path = join_relative(root, relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, bytes).unwrap();
        }
    }

    fn authored(payload_root: &Path) -> DestinationProfile {
        DestinationProfile::author(
            "test-layout",
            "Games",
            ".",
            payload_root,
            &[SourceTree {
                relative_root: ".".to_string(),
                dst_prefix: ".".to_string(),
            }],
        )
        .expect("a payload on disk can always be measured")
    }

    fn files_on(root: &Path) -> BTreeMap<String, u64> {
        let mut out = BTreeMap::new();
        collect_files(root, ".", &mut out);
        out
    }

    fn json_profile(entries: &str, empty_dirs: &str) -> String {
        format!(
            "{{\"schema\":\"{PROFILE_SCHEMA}\",\"profile\":\"t\",\
             \"confirmed_on_hardware\":false,\"game_dir\":\"Games\",\
             \"user_dir\":\".\",\"entries\":[{entries}],\
             \"empty_dirs\":[{empty_dirs}]}}"
        )
    }

    fn entry_json(dst: &str, size: u64) -> String {
        format!("{{\"src\":\"payload/x\",\"dst\":\"{dst}\",\"size\":{size}}}")
    }

    // -----------------------------------------------------------------------
    // Space
    // -----------------------------------------------------------------------

    /// The failure the whole check exists for: a drive that fills at file 2,600
    /// of 2,798. Refusal has to happen before the first byte, and the drive has
    /// to be untouched afterwards.
    #[test]
    fn a_drive_without_room_is_refused_before_any_byte_is_written() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(
            &payload,
            &[
                ("Config/Dolphin.ini", b"ISOPath0 = E:\\Games"),
                ("Games/NSR.iso", &[7u8; 8192]),
            ],
        );
        let profile = authored(&payload);
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();

        let target = target_at(&usb, 4096, BIG, "exFAT");
        let plan = CopyPlan::build(target, &profile, &payload).unwrap();

        let refusal = plan
            .approve(&[])
            .expect_err("a drive with 4 KB free must not be handed to the copier");
        assert!(
            matches!(refusal, Refusal::NotEnoughSpace { .. }),
            "got {refusal:?}"
        );
        assert!(
            files_on(&usb).is_empty(),
            "the drive must be untouched when the copy was refused"
        );
        let message = refusal.message();
        assert!(
            message.contains("Nothing has been written"),
            "the user has to be told the drive is untouched: {message}"
        );
    }

    /// Free space and total capacity are different questions, and the design
    /// document's 4.47 GB figure answers the second one. Conflating them refuses
    /// good drives.
    #[test]
    fn required_free_space_is_not_the_capacity_figure() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, &[("Games/NSR.iso", &[1u8; 4096])]);
        let profile = authored(&payload);
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();
        let plan = CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();

        assert_eq!(plan.required_free_bytes(), 4096 + STAGING_MARGIN_BYTES);
        assert_eq!(
            plan.required_capacity_bytes(),
            4096 + 4096 + STAGING_MARGIN_BYTES,
            "capacity has to allow for the largest file existing twice while it \
             is staged"
        );
        assert!(plan.required_capacity_bytes() > plan.required_free_bytes());
    }

    /// A stick that could never hold the payload gets a different sentence from
    /// one that is merely full, because the actions differ.
    #[test]
    fn a_drive_too_small_to_ever_fit_is_refused_with_its_own_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, &[("Games/NSR.iso", &[1u8; 4096])]);
        let profile = authored(&payload);
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();

        let target = target_at(&usb, 1024, 2048, "exFAT");
        let plan = CopyPlan::build(target, &profile, &payload).unwrap();
        let refusal = plan.approve(&[]).expect_err("2 KB is not an Xbox drive");
        assert!(
            matches!(refusal, Refusal::DriveTooSmall { .. }),
            "got {refusal:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Choosing a drive
    // -----------------------------------------------------------------------

    /// A path the drive seam never reported must never become a target. This is
    /// the guard against writing 3 GB into somebody's internal disk because a
    /// caller passed the wrong path.
    #[test]
    fn a_path_that_is_not_a_removable_drive_can_never_become_a_target() {
        let tmp = tempfile::tempdir().unwrap();
        let usb = tmp.path().join("usbroot");
        let internal = tmp.path().join("system-disk");
        std::fs::create_dir_all(&usb).unwrap();
        std::fs::create_dir_all(&internal).unwrap();

        let survey = survey_of(
            vec![drive(&usb, BIG, BIG, "exFAT")],
            &Guards::protecting(Vec::new()),
        );
        let refusal = choose(&survey, Some(&internal))
            .expect_err("only enumerated removable drives may be chosen");
        assert!(
            matches!(refusal, Refusal::NotRemovable { .. }),
            "got {refusal:?}"
        );
        assert!(
            refusal.message().contains("removable"),
            "{}",
            refusal.message()
        );
    }

    /// Section 7, tightened: preselect nothing, ever. One drive present is still
    /// one deliberate selection, because the alternative is a 3 GB write to
    /// whatever happened to be plugged in.
    #[test]
    fn nothing_is_preselected_even_when_exactly_one_drive_is_present() {
        let tmp = tempfile::tempdir().unwrap();
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();
        let survey = survey_of(
            vec![drive(&usb, BIG, BIG, "exFAT")],
            &Guards::protecting(Vec::new()),
        );
        assert_eq!(survey.selectable().len(), 1);
        assert!(
            matches!(choose(&survey, None), Err(Refusal::NothingSelected)),
            "one selectable drive is not the same as a chosen drive"
        );
    }

    /// A portable launcher install on the stick the user is about to overwrite.
    /// The drive is still listed, with the reason, so the user knows it was seen.
    #[test]
    fn a_drive_holding_the_launchers_own_files_is_listed_but_never_written_to() {
        let tmp = tempfile::tempdir().unwrap();
        let usb = tmp.path().join("usbroot");
        let data_root = usb.join("NFL Street Reborn Launcher");
        std::fs::create_dir_all(&data_root).unwrap();

        let survey = survey_of(
            vec![drive(&usb, BIG, BIG, "exFAT")],
            &Guards::protecting(vec![data_root]),
        );
        assert_eq!(survey.selectable().len(), 0);
        assert_eq!(survey.refused().len(), 1, "it must still be shown");
        let why = survey.offers[0].why_not().expect("a reason is required");
        assert!(why.contains("NFL Street Reborn"), "{why}");

        let refusal = choose(&survey, Some(&usb)).expect_err("must be refused");
        assert!(
            matches!(
                refusal,
                Refusal::DriveRefused {
                    reason: DriveRefusal::HoldsLauncherFiles { .. },
                    ..
                }
            ),
            "got {refusal:?}"
        );
    }

    /// Open item 3 is unanswered: nobody knows what filesystem the console
    /// needs. So the three the design material shows are allowed with honest
    /// notes and everything else is refused, rather than copying 3 GB to find
    /// out.
    #[test]
    fn a_filesystem_the_wizard_cannot_reason_about_is_refused() {
        assert!(filesystem_verdict(Some("exFAT")).allows_writing());
        assert!(filesystem_verdict(Some("NTFS")).allows_writing());
        assert!(filesystem_verdict(Some("fat32")).allows_writing());
        assert!(!filesystem_verdict(Some("ext4")).allows_writing());
        assert!(
            !filesystem_verdict(None).allows_writing(),
            "an undetectable filesystem is not a filesystem we can reason about"
        );

        let tmp = tempfile::tempdir().unwrap();
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();
        let survey = survey_of(
            vec![drive(&usb, BIG, BIG, "ext4")],
            &Guards::protecting(Vec::new()),
        );
        assert!(survey.selectable().is_empty());
        let why = survey.offers[0].why_not().unwrap();
        assert!(
            why.contains("ext4"),
            "the detected name must be named: {why}"
        );
    }

    /// The engine never changes per filesystem, only the promise does. exFAT and
    /// FAT32 have no metadata journal, so no crash-safety claim may be made.
    #[test]
    fn only_ntfs_gets_a_crash_safety_promise() {
        let tmp = tempfile::tempdir().unwrap();
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();
        assert_eq!(
            target_at(&usb, BIG, BIG, "exFAT").crash_safety(),
            CrashSafety::NoClaim
        );
        assert_eq!(
            target_at(&usb, BIG, BIG, "FAT32").crash_safety(),
            CrashSafety::NoClaim
        );
        assert_eq!(
            target_at(&usb, BIG, BIG, "NTFS").crash_safety(),
            CrashSafety::StagedRenameExpectedAtomic
        );
        assert!(CrashSafety::NoClaim.note().contains("Do not unplug"));
    }

    /// Section 11's corollary. A drive letter is the one attribute guaranteed
    /// not to survive a replug, so resume state must never be filed under it.
    #[test]
    fn resume_state_is_keyed_on_volume_identity_and_never_on_the_mount_point() {
        let tmp = tempfile::tempdir().unwrap();
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();

        let key = target_at(&usb, BIG, BIG, "exFAT")
            .journal_key()
            .expect("a drive with an identity has a key");
        assert!(key.contains("2f8a"), "the identity has to survive: {key}");
        assert!(
            !key.contains("usbroot"),
            "the mount point must not appear in the key: {key}"
        );

        let mut anonymous = drive(&usb, BIG, BIG, "exFAT");
        anonymous.volume_identity = None;
        let survey = survey_of(vec![anonymous], &Guards::protecting(Vec::new()));
        let target = choose(&survey, Some(&usb)).unwrap();
        assert!(
            target.journal_key().is_none(),
            "with no volume identity, resume is unavailable rather than keyed on \
             the letter"
        );
    }

    // -----------------------------------------------------------------------
    // Overwrites
    // -----------------------------------------------------------------------

    /// The rule with the most ways to get it wrong: a file already on the drive
    /// is never replaced unless it was listed to the user first. Approval takes
    /// the list that was displayed, so a caller that shows nothing cannot copy.
    #[test]
    fn a_file_already_on_the_drive_is_never_replaced_unless_it_was_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(
            &payload,
            &[("Config/Dolphin.ini", b"ours: ISOPath0 = E:\\Games")],
        );
        let profile = authored(&payload);

        let usb = tmp.path().join("usbroot");
        let victim = usb.join("Config").join("Dolphin.ini");
        std::fs::create_dir_all(victim.parent().unwrap()).unwrap();
        std::fs::write(&victim, b"the user's own hand-edited config").unwrap();

        let target = target_at(&usb, BIG, BIG, "exFAT");
        let plan = CopyPlan::build(target, &profile, &payload).unwrap();

        assert_eq!(plan.overwrites().len(), 1);
        let overwrite = &plan.overwrites()[0];
        assert_eq!(overwrite.dst, "Config/Dolphin.ini");
        assert_eq!(
            overwrite.existing_bytes,
            "the user's own hand-edited config".len() as u64,
            "the size on the drive has to be reported, not the size we ship"
        );
        assert_eq!(overwrite.reason, OverwriteReason::ContentDiffers);

        let refusal = plan
            .approve(&[])
            .expect_err("an unlisted overwrite must stop the copy");
        assert!(
            matches!(refusal, Refusal::OverwriteNotShown { .. }),
            "got {refusal:?}"
        );
        assert_eq!(
            std::fs::read(&victim).unwrap(),
            b"the user's own hand-edited config",
            "the file must be untouched while approval is outstanding"
        );

        let approved = plan
            .approve(&["Config/Dolphin.ini"])
            .expect("a listed overwrite is allowed");
        let platform = Platform::detect();
        let outcome = copy(&platform, &approved, &mut |_| {}).unwrap();
        assert!(outcome.is_complete(), "{outcome:?}");
        assert_eq!(
            std::fs::read(&victim).unwrap(),
            b"ours: ISOPath0 = E:\\Games"
        );
    }

    /// A destination the profile cannot prove is still an overwrite. "Probably
    /// the same length" is not evidence, and treating it as a match would leave
    /// a wrong file in place forever.
    #[test]
    fn a_destination_with_no_digest_to_compare_is_treated_as_an_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, &[("Config/GFX.ini", b"1234")]);
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(usb.join("Config")).unwrap();
        std::fs::write(usb.join("Config").join("GFX.ini"), b"abcd").unwrap();

        // Hand-written profile with no sha256 for a non-empty file.
        let profile = DestinationProfile::from_json(&json_profile(
            "{\"src\":\"Config/GFX.ini\",\"dst\":\"Config/GFX.ini\",\"size\":4}",
            "",
        ))
        .unwrap();
        assert_eq!(profile.entries()[0].verified_by(), VerifiedBy::LengthOnly);

        let plan = CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();
        assert_eq!(plan.overwrites().len(), 1);
        assert_eq!(
            plan.overwrites()[0].reason,
            OverwriteReason::CannotBeChecked
        );
    }

    // -----------------------------------------------------------------------
    // The copy
    // -----------------------------------------------------------------------

    /// The normal way a 3 GB copy to a USB stick ends badly. It has to stop, say
    /// how far it got, and tell the user what to do, not raise an io error per
    /// remaining file.
    #[test]
    fn a_drive_pulled_out_mid_copy_stops_and_reports_what_landed() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(
            &payload,
            &[
                ("Load/a.png", &[1u8; 512]),
                ("Load/b.png", &[2u8; 512]),
                ("Load/c.png", &[3u8; 512]),
            ],
        );
        let profile = authored(&payload);
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();

        let plan = CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();
        assert_eq!(plan.to_write().len(), 3);
        let approved = plan
            .approve(&[])
            .expect("a clean drive needs no approval list");

        let platform = Platform::detect();
        let mut seen = 0;
        let usb_for_closure = usb.clone();
        let outcome = copy(&platform, &approved, &mut |_p| {
            seen += 1;
            if seen == 1 {
                // The user pulls the stick out.
                let _ = std::fs::remove_dir_all(&usb_for_closure);
            }
        })
        .expect("a yanked drive is a return value, not an error");

        match outcome {
            CopyOutcome::Interrupted {
                report,
                cause:
                    Interruption::DriveWentAway {
                        remaining_files,
                        remaining_bytes,
                        ..
                    },
            } => {
                assert_eq!(report.written.len(), 1, "one file landed before the pull");
                assert_eq!(remaining_files, 2);
                assert_eq!(remaining_bytes, 1024);
                let message = Interruption::DriveWentAway {
                    root: usb.clone(),
                    remaining_files,
                    remaining_bytes,
                }
                .message();
                assert!(
                    message.contains("Plug it back in"),
                    "the user needs a next step: {message}"
                );
            }
            other => panic!("expected a drive-went-away interruption, got {other:?}"),
        }
    }

    /// Section 11, the highest-value single rule: the wizard writes Xbox-side
    /// paths verbatim and never substitutes the letter the drive has on the PC.
    /// Byte equality is the mechanical form of that promise.
    #[test]
    fn a_config_file_lands_byte_for_byte_so_the_xbox_side_drive_letter_survives() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        let ini: &[u8] = b"[Main.General]\nISOPaths = 1\nISOPath0 = E:\\Games\n";
        write_payload(&payload, &[("Config/Dolphin.ini", ini)]);
        let profile = authored(&payload);
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();

        let plan = CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();
        let approved = plan.approve(&[]).unwrap();
        let platform = Platform::detect();
        assert!(copy(&platform, &approved, &mut |_| {})
            .unwrap()
            .is_complete());

        assert_eq!(
            std::fs::read(usb.join("Config").join("Dolphin.ini")).unwrap(),
            ini,
            "the Xbox-side path E:\\Games must survive untouched"
        );
    }

    /// Five payload config files are zero bytes. A digest match on an empty file
    /// is a check that cannot fail, so it is counted separately and never
    /// reported as content verification.
    #[test]
    fn a_zero_byte_config_is_verified_by_presence_and_length_not_a_vacuous_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(
            &payload,
            &[("Config/FreeLook.ini", b""), ("Config/GFX.ini", b"x")],
        );
        let profile = authored(&payload);
        let empty = profile
            .entries()
            .iter()
            .find(|e| e.dst == "Config/FreeLook.ini")
            .unwrap();
        assert_eq!(empty.verified_by(), VerifiedBy::PresenceAndLength);
        assert!(
            empty.sha256.is_none(),
            "authoring must not record a digest that proves nothing"
        );
        assert!(!VerifiedBy::PresenceAndLength.proves_content());

        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();
        let plan = CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();
        let platform = Platform::detect();
        let outcome = copy(&platform, &plan.approve(&[]).unwrap(), &mut |_| {}).unwrap();
        let report = outcome.report();
        assert_eq!(report.presence_verified, 1);
        assert_eq!(report.hash_verified, 1);
        assert!(
            report.summary().contains("zero-byte"),
            "the report must not hide which check was used: {}",
            report.summary()
        );
    }

    /// What makes an update cheap. A second run over an unchanged drive must
    /// write nothing at all, and a single damaged file must be the only rewrite.
    #[test]
    fn a_second_run_rewrites_only_what_no_longer_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(
            &payload,
            &[("Config/A.ini", b"aaaa"), ("Config/B.ini", b"bbbb")],
        );
        let profile = authored(&payload);
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();
        let platform = Platform::detect();

        let first =
            CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();
        assert!(copy(&platform, &first.approve(&[]).unwrap(), &mut |_| {})
            .unwrap()
            .is_complete());

        let second =
            CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();
        assert!(
            second.to_write().is_empty(),
            "an unchanged drive must cost zero writes, not 2,798"
        );
        assert_eq!(second.already_current().len(), 2);

        std::fs::write(usb.join("Config").join("B.ini"), b"cccc").unwrap();
        let third =
            CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();
        assert_eq!(third.to_write().len(), 1);
        assert_eq!(third.to_write()[0].dst, "Config/B.ini");
        assert!(third.to_write()[0].replaces_existing);
    }

    /// A payload that does not match its own profile would produce a partial
    /// install. Caught before the drive is touched.
    #[test]
    fn a_payload_file_missing_on_the_pc_is_refused_before_writing_anything() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(
            &payload,
            &[("Config/A.ini", b"aaaa"), ("Games/NSR.iso", &[9u8; 2048])],
        );
        let profile = authored(&payload);
        std::fs::remove_file(payload.join("Games").join("NSR.iso")).unwrap();

        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();
        let plan = CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();
        assert_eq!(plan.missing_sources(), ["Games/NSR.iso"]);

        let refusal = plan
            .approve(&[])
            .expect_err("a partial payload must be refused");
        assert!(
            matches!(refusal, Refusal::PayloadIncomplete { missing: 1, .. }),
            "got {refusal:?}"
        );
        assert!(files_on(&usb).is_empty(), "nothing may have been written");
    }

    /// Neither fraction alone tells the truth: bytes alone sits still through
    /// 2,782 small files, files alone jumps to 99% and hangs on the disc image.
    #[test]
    fn progress_is_weighted_by_file_count_as_well_as_by_bytes() {
        let after_the_image = Progress {
            files_done: 1,
            files_total: 2783,
            bytes_done: 1_459_978_240,
            bytes_total: 2_987_903_696,
        };
        assert!(after_the_image.by_bytes() > 0.48);
        assert!(after_the_image.by_files() < 0.01);
        assert!(
            after_the_image.fraction() < after_the_image.by_bytes(),
            "a bytes-only bar would claim half the work is done after one file"
        );

        let near_the_end = Progress {
            files_done: 2782,
            files_total: 2783,
            bytes_done: 1_527_925_456,
            bytes_total: 2_987_903_696,
        };
        assert!(
            near_the_end.fraction() > near_the_end.by_bytes(),
            "a files-only bar and a bytes-only bar disagree, so the reported \
             number has to move for both"
        );

        let nothing_to_do = Progress {
            files_done: 0,
            files_total: 0,
            bytes_done: 0,
            bytes_total: 0,
        };
        assert_eq!(nothing_to_do.fraction(), 1.0, "no work is finished work");
    }

    // -----------------------------------------------------------------------
    // Verification
    // -----------------------------------------------------------------------

    /// "0 mismatches" over a manifest is not "this drive matches this release".
    /// Files the release does not name are counted, reported, and left exactly
    /// where they are.
    #[test]
    fn verification_names_files_the_release_does_not_ship_and_never_removes_them() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, &[("Config/A.ini", b"aaaa")]);
        let profile = authored(&payload);
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();
        let platform = Platform::detect();
        let plan = CopyPlan::build(target_at(&usb, BIG, BIG, "exFAT"), &profile, &payload).unwrap();
        assert!(copy(&platform, &plan.approve(&[]).unwrap(), &mut |_| {})
            .unwrap()
            .is_complete());

        let stranger = usb.join("Config").join("someone-elses.ini");
        std::fs::write(&stranger, b"not ours").unwrap();

        let target = target_at(&usb, BIG, BIG, "exFAT");
        let report = verify(&target, &profile).unwrap();
        assert!(report.matches_release(), "{:?}", report.mismatched);
        assert_eq!(report.surplus_count, 1);
        assert_eq!(report.surplus[0].dst, "Config/someone-elses.ini");
        assert!(
            report.summary().contains("not in this release"),
            "a clean report must still admit what it found: {}",
            report.summary()
        );
        assert!(
            stranger.exists(),
            "verification reads. It must never remove a foreign file"
        );
    }

    /// A damaged file has to be reported as a mismatch, not as a pass, or Repair
    /// can never converge.
    #[test]
    fn verification_reports_a_damaged_file_rather_than_passing_it() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, &[("Config/A.ini", b"aaaa")]);
        let profile = authored(&payload);
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(usb.join("Config")).unwrap();
        // Same length, different bytes: only the digest can tell.
        std::fs::write(usb.join("Config").join("A.ini"), b"zzzz").unwrap();

        let report = verify(&target_at(&usb, BIG, BIG, "exFAT"), &profile).unwrap();
        assert_eq!(report.mismatched, ["Config/A.ini"]);
        assert!(!report.matches_release());
    }

    // -----------------------------------------------------------------------
    // Memory cards
    // -----------------------------------------------------------------------

    /// The card is the only file on the drive with no recovery path. A profile
    /// that writes one cannot even be loaded.
    #[test]
    fn a_profile_that_writes_a_file_under_gc_is_refused_at_load() {
        let error = DestinationProfile::from_json(&json_profile(
            &entry_json("GC/MemoryCardA.USA.raw", 16_777_216),
            "",
        ))
        .expect_err("the wizard never writes a file under GC/");
        assert!(error.contains("GC/"), "{error}");
        assert!(
            error.contains("irreplaceable"),
            "the reason has to be stated: {error}"
        );
    }

    /// The shipped payload contains the author's real 16 MB card. Authoring must
    /// not sweep it in, and must still record the empty directories under `GC/`,
    /// because creating a directory cannot destroy a card.
    #[test]
    fn authoring_skips_a_memory_card_but_still_records_empty_gc_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(
            &payload,
            &[
                ("Config/A.ini", b"aaaa"),
                ("GC/MemoryCardA.USA.raw", &[0xAA; 64]),
                ("GC/SRAM.raw", &[0xBB; 8]),
            ],
        );
        std::fs::create_dir_all(payload.join("GC").join("EUR")).unwrap();
        std::fs::create_dir_all(payload.join("Config").join("GraphicMods")).unwrap();

        let profile = authored(&payload);
        assert!(
            profile
                .entries()
                .iter()
                .all(|e| !has_component(&e.dst, GC_DIR)),
            "a memory card must never enter a payload: {:?}",
            profile.entries()
        );
        assert!(profile.empty_dirs().iter().any(|d| d == "GC/EUR"));
        assert!(profile
            .empty_dirs()
            .iter()
            .any(|d| d == "Config/GraphicMods"));
        assert!(
            !profile.empty_dirs().iter().any(|d| d == "GC"),
            "GC/ is not empty, it just holds nothing we may copy"
        );
    }

    /// A card sitting on the drive is outside the surplus classification
    /// entirely, so it can never end up on a list of files somebody might be
    /// offered the chance to remove.
    #[test]
    fn a_memory_card_on_the_drive_is_never_classed_as_a_surplus_file() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, &[("Config/A.ini", b"aaaa")]);
        let profile = DestinationProfile::from_json(&json_profile(
            "{\"src\":\"Config/A.ini\",\"dst\":\"A.ini\",\"size\":4}",
            "",
        ))
        .unwrap();

        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(usb.join("GC")).unwrap();
        std::fs::write(usb.join("GC").join("MemoryCardA.USA.raw"), [0xAA; 64]).unwrap();
        std::fs::write(usb.join("A.ini"), b"aaaa").unwrap();

        let report = verify(&target_at(&usb, BIG, BIG, "exFAT"), &profile).unwrap();
        assert_eq!(
            report.surplus_count, 0,
            "GC/ is outside the classification: {:?}",
            report.surplus
        );
        assert!(usb.join("GC").join("MemoryCardA.USA.raw").exists());
    }

    // -----------------------------------------------------------------------
    // Profile validation
    // -----------------------------------------------------------------------

    /// Two entries that are one file on the drive would silently let the second
    /// copy win, and the byte total would count the file twice.
    #[test]
    fn two_entries_that_differ_only_by_case_are_refused() {
        let error = DestinationProfile::from_json(&json_profile(
            &format!(
                "{},{}",
                entry_json("Config/Dolphin.ini", 4),
                entry_json("Config/dolphin.INI", 4)
            ),
            "",
        ))
        .expect_err("the drive is case insensitive, so these are one file");
        assert!(error.contains("case-insensitive"), "{error}");
    }

    /// A profile has to state whether its layout was tested on hardware. An
    /// absent field defaulting to false is indistinguishable from a deliberate
    /// false, and the UI has to be able to say which.
    #[test]
    fn a_profile_must_state_whether_the_layout_was_confirmed_on_hardware() {
        let without = format!(
            "{{\"schema\":\"{PROFILE_SCHEMA}\",\"profile\":\"t\",\
             \"game_dir\":\"Games\",\"user_dir\":\".\",\"entries\":[{}]}}",
            entry_json("A.ini", 4)
        );
        let error = DestinationProfile::from_json(&without)
            .expect_err("confirmed_on_hardware has no default");
        assert!(error.contains("confirmed_on_hardware"), "{error}");

        let profile =
            DestinationProfile::from_json(&json_profile(&entry_json("A.ini", 4), "")).unwrap();
        assert!(!profile.confirmed_on_hardware());
        assert!(
            profile.layout_caveat().is_some(),
            "an unconfirmed layout must carry its caveat into the UI"
        );
    }

    /// A digest that is not a digest must fail on load, not at file 2,600 of a
    /// copy that has already replaced 2,599 files.
    #[test]
    fn a_digest_that_is_not_a_sha256_is_refused_at_load() {
        let error = DestinationProfile::from_json(&json_profile(
            "{\"src\":\"a\",\"dst\":\"A.ini\",\"size\":4,\"sha256\":\"nope\"}",
            "",
        ))
        .expect_err("a 4 character digest is not a digest");
        assert!(error.contains("sha-256"), "{error}");
    }

    /// A destination that climbs out of the drive root would write into the
    /// user's own filesystem. Every form of it is refused on load.
    #[test]
    fn a_destination_that_escapes_the_drive_root_is_refused() {
        for bad in [
            "../Windows/System32/evil.dll",
            "C:/Windows/evil.dll",
            "/etc/passwd",
            "Config\\Dolphin.ini",
            "Config//Dolphin.ini",
        ] {
            let error =
                DestinationProfile::from_json(&json_profile(&entry_json(bad, 4), "")).unwrap_err();
            assert!(!error.is_empty(), "{bad} must be refused with a reason");
        }
    }

    /// A payload file that ends in the staging suffix would be deleted by the
    /// wizard's own cleanup, which is the one delete that happens without a
    /// per-file confirmation.
    #[test]
    fn a_payload_file_using_the_staging_suffix_is_refused() {
        let error = DestinationProfile::from_json(&json_profile(
            &entry_json(&format!("Config/Dolphin.ini{STAGING_SUFFIX}"), 4),
            "",
        ))
        .expect_err("the staging suffix is reserved");
        assert!(error.contains(STAGING_SUFFIX), "{error}");
    }

    /// A profile that copies nothing must not be loadable, because a wizard that
    /// copies nothing and reports success is the worst possible outcome.
    #[test]
    fn a_profile_that_names_no_files_is_refused() {
        let error = DestinationProfile::from_json(&json_profile("", "")).unwrap_err();
        assert!(error.contains("names no files"), "{error}");
    }

    /// The profile must round-trip through disk unchanged, or a saved profile
    /// stops describing the drive it wrote.
    #[test]
    fn a_profile_survives_a_round_trip_through_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(
            &payload,
            &[("Config/A.ini", b"aaaa"), ("Config/Empty.ini", b"")],
        );
        let profile = authored(&payload);
        let path = tmp.path().join("profiles").join("xbox.json");
        profile.save(&path).unwrap();

        let reloaded = DestinationProfile::load(&path).unwrap();
        assert_eq!(reloaded.file_count(), profile.file_count());
        assert_eq!(reloaded.total_bytes(), profile.total_bytes());
        assert_eq!(reloaded.entries(), profile.entries());
        assert!(!reloaded.confirmed_on_hardware());
    }

    /// A missing profile is not an empty profile. The error has to say what to
    /// do, because the layout genuinely is not known.
    #[test]
    fn a_missing_profile_says_what_to_do_instead_of_copying_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let error = DestinationProfile::load(&tmp.path().join("absent.json"))
            .expect_err("there is no default layout to fall back on");
        let message = error.to_string();
        assert!(message.contains("not documented"), "{message}");
        assert!(message.contains("author"), "{message}");
    }

    // -----------------------------------------------------------------------
    // Finishing, and the state of the seam
    // -----------------------------------------------------------------------

    /// Eject failing is not the copy failing. After 2,798 verified files the
    /// user must not be told only "could not eject".
    #[test]
    fn eject_being_refused_is_not_the_copy_failing() {
        let tmp = tempfile::tempdir().unwrap();
        let usb = tmp.path().join("usbroot");
        std::fs::create_dir_all(&usb).unwrap();
        let target = target_at(&usb, BIG, BIG, "exFAT");
        let platform = Platform::detect();

        let outcome = eject(&platform, &target);
        let message = outcome.message();
        match outcome {
            EjectOutcome::Refused { .. } => {
                assert!(
                    message.contains("already written and checked"),
                    "a refusal must reassure, not read as a failure: {message}"
                );
            }
            EjectOutcome::Ejected => {
                assert!(message.contains("safe to unplug"), "{message}");
            }
        }
    }

    /// Every refusal has to be a sentence somebody can act on. The same rule the
    /// launch sequence holds itself to.
    #[test]
    fn every_refusal_says_something_actionable() {
        let refusals = [
            Refusal::NothingSelected,
            Refusal::NotRemovable {
                path: PathBuf::from("Z:\\"),
            },
            Refusal::DriveRefused {
                path: PathBuf::from("Z:\\"),
                reason: DriveRefusal::CapacityUnknown,
            },
            Refusal::DriveRefused {
                path: PathBuf::from("Z:\\"),
                reason: DriveRefusal::FilesystemUnsupported {
                    detected: Some("ext4".to_string()),
                },
            },
            Refusal::DriveRefused {
                path: PathBuf::from("Z:\\"),
                reason: DriveRefusal::HoldsLauncherFiles {
                    protected: PathBuf::from("Z:\\launcher"),
                },
            },
            Refusal::NotEnoughSpace {
                required: 3_013_595_002,
                free: 1_000_000_000,
            },
            Refusal::DriveTooSmall {
                required: 4_473_573_242,
                capacity: 2_000_000_000,
            },
            Refusal::PayloadIncomplete {
                missing: 3,
                first: "Load/Textures/GN7E69/a.png".to_string(),
            },
            Refusal::OverwriteNotShown {
                dst: "Config/Dolphin.ini".to_string(),
            },
        ];
        for refusal in refusals {
            let message = refusal.message();
            assert!(message.len() > 30, "too terse to act on: {message}");
            assert!(message.ends_with('.'), "not a sentence: {message}");
        }
    }

    /// The byte figure always travels with the friendly one, because three
    /// different rounded numbers for this payload are already circulating in the
    /// design documents.
    #[test]
    fn byte_counts_are_never_reported_only_as_rounded_gigabytes() {
        let described = describe_bytes(3_013_595_002);
        assert!(described.contains("3.01 GB"), "{described}");
        assert!(described.contains("3013595002"), "{described}");
    }

    /// The drive seam is a stub on every platform today, and the wizard reports
    /// that rather than showing an empty picker that implies nothing is plugged
    /// in. Written to keep passing on the day the seam is implemented.
    #[test]
    fn the_drive_seam_reports_honestly_whether_or_not_it_is_implemented() {
        let platform = Platform::detect();
        match survey(&platform) {
            Err(Error::NotImplemented { operation, .. }) => {
                assert_eq!(operation, "removable drive enumeration");
            }
            Err(other) => panic!("an unimplemented seam must say so, got {other:?}"),
            Ok(found) => {
                for offer in &found.offers {
                    assert!(
                        offer.drive.mount_point.is_absolute(),
                        "a mount point must be absolute: {:?}",
                        offer.drive.mount_point
                    );
                }
            }
        }
    }
}
