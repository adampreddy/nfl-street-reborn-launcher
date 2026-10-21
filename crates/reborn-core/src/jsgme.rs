//! Reading the player's real JSGME mod library, and describing it in the terms
//! [`crate::mods`] composes in.
//!
//! # Why a bridge is needed at all
//!
//! [`crate::mods`] composes a [`ModPackage`]: a validated manifest plus a
//! payload under `mods/packages/<id>/<version>/files/`. The mods on this
//! machine are not that. A JSGME mod is a folder under `MODS/` holding a mirror
//! of the game tree and nothing else: no manifest, no id, no version, no
//! hashes, and no declaration of what it overwrites. Measured on the shipped
//! 2.1 install, `MODS/` holds 11 of them.
//!
//! So this module reads that library and produces what composition needs. It
//! derives only what is genuinely derivable and refuses to fill in the rest.
//!
//! | Field composition needs | Where it comes from here |
//! |---|---|
//! | `name` | the folder name, verbatim |
//! | `id` | a slug of the folder name, namespaced [`IMPORTED_ID_PREFIX`] |
//! | `files` | walked, with byte length and SHA-256 read off each file |
//! | `targets` | the build directories the payload actually contains |
//! | `version` | **nowhere.** [`JsgmeMod::to_package`] takes it as an argument |
//! | `conflict_group` | **nowhere.** Always absent |
//!
//! Those last two are the fields that would have become placeholders. JSGME has
//! no notion of either, so inventing `1.0.0` and `"ball"` would put a made-up
//! value into a manifest that later reads as authoritative. Absent costs
//! nothing: a contested path is computed from the file lists by
//! [`crate::mods::detect_conflicts`], so the seven football mods still refuse
//! each other with no group declared.
//!
//! # This module never writes
//!
//! Every function here reads. That is not tidiness, it is the safety property:
//! `MODS/` is the only copy of the player's mod payloads, and `MODS/!BACKUP` is
//! the only copy of any original those mods displaced. Conversion, which does
//! copy bytes, is returned as a [`ConversionPlan`] for the caller to execute,
//! and the plan refuses outright if its destination is inside `MODS/`.
//!
//! # What was measured on the real library
//!
//! Facts this module was written against, read off
//! `%APPDATA%\NFL Street Reborn\MODS` rather than assumed:
//!
//! - **20 contested paths.** The seven `* Football` mods all write the single
//!   path `User/Load/Textures/GN7E69/tex1_128x128_m_ffc5151a141db330_14.dds`,
//!   and `PS Controls` and `Xbox Controls` write the same 19 button prompts.
//!   JSGME resolves that by copying in order and letting the last one win, with
//!   nothing said to the user, which is one of the reasons it is being
//!   replaced. [`PathOverlap`] names every mod on a path rather than pairing
//!   them off, because "7 mods write this file" is the useful sentence.
//! - **A folder under `Textures/` is not necessarily a build.** `Xbox Controls`
//!   carries `User/Load/Textures/v2/`, 20 working files its own install log
//!   never mentions. `v2` passes [`GameId::parse`] on shape alone, so a scan
//!   that trusted the directory name would publish a package claiming to target
//!   a build that does not exist. That is why [`JsgmeLibrary::scan`] takes the
//!   builds the caller actually has and reports the rest as
//!   [`SkipReason::UnknownBuild`].
//! - **An applied mod with no backup entry is not proof of a lost original.**
//!   JSGME only stashes a file it displaced, so a mod that only adds files
//!   leaves no backup at all. `Xbox Controls` is exactly that: 19 paths in its
//!   install log, 0 in `!BACKUP`, and nothing lost. The state that is genuinely
//!   dangerous is the whole of `!BACKUP` being empty or missing while
//!   `!INSTLOGS` is not, which is the blocker [`crate::migration`] already
//!   refuses to migrate through. [`AppliedState::blocker`] is that check and
//!   nothing looser.
//! - **Both directories can legitimately be empty.** That is what nothing
//!   applied looks like, and it is the normal case rather than an edge case.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::library::hash_file;
use crate::manifest::Sha256Hex;
use crate::mods::{
    GameId, ModFile, ModId, ModPackage, PackageVersion, PayloadPath, MOD_MAGIC, MOD_SCHEMA,
    PACKAGE_MANIFEST,
};
use crate::platform::{Error, Result};

/// JSGME's backup folder, holding files it displaced.
pub const BACKUP_DIRNAME: &str = "!BACKUP";
/// JSGME's install logs, one per applied mod.
pub const INSTLOGS_DIRNAME: &str = "!INSTLOGS";
/// JSGME's own settings file, which records the applied order.
pub const JSGME_INI: &str = "JSGME.ini";
/// The section of [`JSGME_INI`] holding the applied order.
pub const JSGME_INI_SECTION: &str = "MODS";
/// The suffix JSGME puts on an install log file name.
pub const INSTALL_LOG_SUFFIX: &str = " install.log";
/// Where the texture mirror sits inside a mod folder, relative to the JSGME
/// root.
///
/// The same three segments [`crate::migration`] copies the pristine tree from.
/// Spelled again here rather than shared because that module owns migration and
/// this one owns import; if a third place needs it, it should become one
/// constant in one of them.
pub const TEXTURE_MIRROR_PREFIX: &str = "User/Load/Textures";
/// The namespace every imported package id carries.
///
/// An imported id says where it came from, and it can never collide with a
/// curated `reborn.*` package that happens to share a name. Two packages with
/// one id would be two versions of one mod as far as
/// [`crate::mods::Activation`] is concerned, so turning one on would silently
/// turn the other off.
pub const IMPORTED_ID_PREFIX: &str = "jsgme.";

fn invalid(message: String) -> Error {
    Error::PlainIo(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

// ---------------------------------------------------------------------------
// Files the launcher will not import
// ---------------------------------------------------------------------------

/// Why one file in a mod folder cannot become a payload entry.
///
/// Reported per file rather than counted, because a mod that silently imports
/// 19 of its 39 files looks like it worked. Each of these is a real shape found
/// in the shipped library, or a shape [`PayloadPath`] exists to refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Not under `User/Load/Textures/`, so it is not a texture overlay.
    ///
    /// JSGME can mirror any part of the game tree. Version 1 of the package
    /// format carries textures only, so anything else is described and left
    /// where it is rather than imported as something it is not.
    OutsideTextureMirror,
    /// Directly inside `Textures/`, with no build directory naming what it is
    /// for.
    NoBuildDirectory,
    /// The directory under `Textures/` cannot be a build id at all.
    NotABuildDirectory { name: String },
    /// The directory under `Textures/` could be a build id by shape, but the
    /// caller does not have that build.
    ///
    /// This is the `Xbox Controls/User/Load/Textures/v2/` case. Importing it
    /// would put a target in a manifest naming a build nothing can compose for.
    UnknownBuild { name: String },
    /// [`PayloadPath::parse`] refused it, with its own reason.
    RejectedPayloadPath { detail: String },
}

impl SkipReason {
    /// A sentence saying what was skipped and why it was not guessed at.
    pub fn message(&self) -> String {
        match self {
            SkipReason::OutsideTextureMirror => format!(
                "it is not under {TEXTURE_MIRROR_PREFIX}, so it is not a texture \
                 overlay and this package format carries nothing else"
            ),
            SkipReason::NoBuildDirectory => format!(
                "it sits directly in {TEXTURE_MIRROR_PREFIX} with no build \
                 directory, so nothing says which game it is for"
            ),
            SkipReason::NotABuildDirectory { name } => format!(
                "{name:?} is not a usable build directory name, so the file \
                 cannot be placed in a composed tree"
            ),
            SkipReason::UnknownBuild { name } => format!(
                "{name:?} is not one of the builds installed here. It may be a \
                 working folder the mod author left in place. Importing it would \
                 claim the mod targets a build that does not exist"
            ),
            SkipReason::RejectedPayloadPath { detail } => detail.clone(),
        }
    }
}

/// One file that will not be imported, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unimportable {
    /// Path relative to the mod folder, forward slashes. Also the path JSGME
    /// would write it to, relative to the JSGME root.
    pub mirror_relative: String,
    pub bytes: u64,
    pub why: SkipReason,
}

// ---------------------------------------------------------------------------
// A mod
// ---------------------------------------------------------------------------

/// One file a JSGME mod would write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsgmeFile {
    /// Path relative to the mod folder, forward slashes.
    ///
    /// This is also the path JSGME overwrites, relative to the JSGME root, and
    /// it is the form the install logs record. Carried alongside
    /// [`JsgmeFile::payload`] because they answer different questions: this one
    /// says what JSGME would have clobbered, the other says where composition
    /// would put it.
    pub mirror_relative: String,
    /// Where the bytes are now, inside the mod folder.
    pub source: PathBuf,
    /// The byte length, read from the file.
    ///
    /// Read here rather than taken from a directory listing because
    /// [`crate::mods::Composer::plan`] refuses a package whose declared size
    /// does not match its payload, and an allocated size rounded up to the
    /// cluster would fail that check on every correct import.
    pub bytes: u64,
    pub sha256: Sha256Hex,
    /// Where composition would place it, `<gameid>/<name>`.
    pub payload: PayloadPath,
}

/// One JSGME mod folder, described.
#[derive(Debug, Clone)]
pub struct JsgmeMod {
    /// The folder name, verbatim. This is what the player calls the mod, and it
    /// is the only human name that exists.
    pub name: String,
    /// The derived package id. See [`slug_id`].
    pub id: ModId,
    pub folder: PathBuf,
    /// Importable files, sorted by mirror path.
    pub files: Vec<JsgmeFile>,
    /// Files present in the folder that will not be imported.
    ///
    /// Not hashed, because hashing would imply a plan to carry them.
    pub skipped: Vec<Unimportable>,
    /// Entries that are neither a plain file nor a directory, and entries whose
    /// name is not valid UTF-8.
    ///
    /// Reported rather than ignored, for the reason
    /// [`crate::mods::PristineListing`] gives about its own unsupported list: a
    /// symlinked directory would make a walk circle, and dropping one quietly
    /// loses a texture with nothing said.
    pub unsupported: Vec<PathBuf>,
}

impl JsgmeMod {
    /// The builds this mod actually has payload for, sorted.
    ///
    /// Computed from the files rather than from the directory names, so a build
    /// directory that turned out to hold nothing importable never becomes a
    /// declared target.
    pub fn targets(&self) -> Vec<GameId> {
        let unique: BTreeSet<&GameId> = self.files.iter().map(|f| f.payload.game_id()).collect();
        unique.into_iter().cloned().collect()
    }

    /// Bytes a conversion would copy.
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }

    /// Every path this mod would write, relative to the JSGME root, importable
    /// or not.
    ///
    /// Overlap detection uses all of them. JSGME copies the whole mirror, so a
    /// file this module declines to import is still a file JSGME would have
    /// overwritten, and leaving it out of the overlap report would understate
    /// what the old system was doing.
    pub fn written_paths(&self) -> impl Iterator<Item = &str> {
        self.files
            .iter()
            .map(|f| f.mirror_relative.as_str())
            .chain(self.skipped.iter().map(|s| s.mirror_relative.as_str()))
    }

    /// Describe this mod as a package composition can use.
    ///
    /// `version` is an argument because JSGME records no version anywhere: not
    /// in the folder, not in the install log, not in `JSGME.ini`. Returning a
    /// package stamped `1.0.0` would be this module inventing the one field
    /// that later decides which payload directory is read. The caller picks it,
    /// visibly, at the call site.
    ///
    /// `conflict_group` is always absent for the same reason. Nothing in a
    /// JSGME mod says a football is a football.
    pub fn to_package(&self, version: &PackageVersion) -> Result<ModPackage> {
        if self.files.is_empty() {
            return Err(invalid(format!(
                "{:?} has no importable file. {} of its files were skipped, the \
                 first because {}. No package is produced rather than an empty \
                 one, which would activate and change nothing",
                self.name,
                self.skipped.len(),
                self.skipped
                    .first()
                    .map(|s| s.why.message())
                    .unwrap_or_else(|| "the folder holds no files".to_string())
            )));
        }

        let package = ModPackage {
            schema: MOD_SCHEMA.to_string(),
            magic: MOD_MAGIC.to_string(),
            id: self.id.clone(),
            name: self.name.clone(),
            version: version.clone(),
            targets: self.targets(),
            conflict_group: None,
            files: self
                .files
                .iter()
                .map(|f| ModFile {
                    path: f.payload.clone(),
                    sha256: f.sha256.clone(),
                    bytes: f.bytes,
                })
                .collect(),
        };
        // Validated here so a caller is never handed a package that composition
        // will refuse later, with the refusal reading as a fault in the package
        // rather than in this import.
        package.validate()?;
        Ok(package)
    }
}

/// Derive a package id from a JSGME folder name.
///
/// A slug, not an invention: lowercased, runs of anything outside ASCII letters
/// and digits folded to a single hyphen, and namespaced with
/// [`IMPORTED_ID_PREFIX`]. `Patriots Throwback` becomes
/// `jsgme.patriots-throwback`.
///
/// Refuses rather than truncating or numbering. Truncating a long name can make
/// two folders share an id, and one id shared by two packages is two versions
/// of one mod to [`crate::mods::Activation`], so turning one on would turn the
/// other off with nothing said. A name that cannot be slugged is a name the
/// player can change.
pub fn slug_id(folder_name: &str) -> Result<ModId> {
    let mut slug = String::new();
    let mut pending_hyphen = false;
    for ch in folder_name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_hyphen && !slug.is_empty() {
                slug.push('-');
            }
            pending_hyphen = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            pending_hyphen = true;
        }
    }

    if slug.is_empty() {
        return Err(invalid(format!(
            "the mod folder {folder_name:?} has no ASCII letters or digits in its \
             name, so no package id can be derived from it. Rename the folder"
        )));
    }
    let full = format!("{IMPORTED_ID_PREFIX}{slug}");
    ModId::parse(&full).map_err(|e| {
        invalid(format!(
            "the mod folder {folder_name:?} does not yield a usable package id: \
             {e}. Rename the folder to something shorter or simpler"
        ))
    })
}

// ---------------------------------------------------------------------------
// Applied state, and the state that cannot be undone
// ---------------------------------------------------------------------------

/// One file sitting in `!BACKUP`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupEntry {
    /// Path relative to `!BACKUP`, forward slashes, exactly as found.
    pub relative: String,
    /// Which mod displaced it, when the file name says so.
    ///
    /// The shipped library names backups `<original path>.<Mod Name>`. Absent
    /// means the name carries no attribution, and it is left absent rather than
    /// assumed: an original restored to the wrong path is worse than one the
    /// launcher admits it cannot place.
    pub displaced_by: Option<String>,
    /// The path the file came from, relative to the JSGME root, when
    /// [`BackupEntry::displaced_by`] made that recoverable.
    pub original_relative: Option<String>,
    pub bytes: u64,
}

/// One mod JSGME records as applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMod {
    /// The mod name as taken from the install log file name.
    pub name: String,
    pub log: PathBuf,
    /// The paths the log says were written, relative to the JSGME root,
    /// backslashes normalized to forward slashes.
    pub recorded_paths: Vec<String>,
    /// The backup entries attributed to this mod, as original paths.
    pub backup_paths: Vec<String>,
    /// Whether the mod folder is still in `MODS/`.
    ///
    /// False means JSGME itself can no longer un-apply it: the payload it would
    /// need in order to recognize and remove its own files is gone.
    pub folder_present: bool,
    /// Present when the log could not be read, with the reason.
    ///
    /// An unreadable log still proves something was applied, so it is carried
    /// rather than dropped. Dropping it would move the library toward "nothing
    /// applied", which is the dangerous direction.
    pub unreadable: Option<String>,
}

impl AppliedMod {
    /// Recorded paths with no backup entry behind them.
    ///
    /// **Not a list of lost originals.** JSGME only stashes a file it
    /// displaced, so a mod that adds new files leaves no backup for them and
    /// nothing was lost. On the shipped library `Xbox Controls` has 19 recorded
    /// paths and 0 backups for exactly that reason. Use
    /// [`AppliedState::blocker`] for the question that can actually be
    /// answered.
    pub fn recorded_without_backup(&self) -> Vec<&str> {
        let held: BTreeSet<String> = self.backup_paths.iter().map(|p| p.to_lowercase()).collect();
        self.recorded_paths
            .iter()
            .filter(|p| !held.contains(&p.to_lowercase()))
            .map(String::as_str)
            .collect()
    }
}

/// What JSGME says is currently applied, and whether it could be undone.
#[derive(Debug, Clone, Default)]
pub struct AppliedState {
    /// One per install log. Empty means nothing is applied, which is the normal
    /// case and what a freshly shipped `MODS/` folder looks like.
    pub applied: Vec<AppliedMod>,
    pub instlogs_root: PathBuf,
    pub instlogs_exists: bool,
    pub backup_root: PathBuf,
    pub backup_exists: bool,
    /// Everything in `!BACKUP`, whether or not it could be attributed.
    pub backup_entries: Vec<BackupEntry>,
    /// Files in `!INSTLOGS` that are not install logs, reported rather than
    /// ignored.
    pub unrecognized_logs: Vec<PathBuf>,
}

impl AppliedState {
    /// Whether JSGME considers anything applied.
    /// Read the applied state on its own, without scanning payloads.
    ///
    /// [`JsgmeLibrary::scan`] hashes every mod file, which is the right cost
    /// when the caller wants to import. A caller that only needs to know
    /// whether migration is safe should not pay it, and migration asks this on
    /// every plan.
    pub fn read(mods_root: &Path) -> Result<Self> {
        // Which mod folders are still present. A mod whose folder is gone
        // cannot be un-applied by JSGME itself, and read_applied_state records
        // that, so it has to be passed in rather than assumed.
        let names: Vec<String> = std::fs::read_dir(mods_root)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|n| !n.starts_with('!'))
                    .collect()
            })
            .unwrap_or_default();
        let present: std::collections::BTreeSet<&str> = names.iter().map(String::as_str).collect();
        read_applied_state(mods_root, &present)
    }

    pub fn nothing_applied(&self) -> bool {
        self.applied.is_empty()
    }

    /// The dangerous state, and only that state.
    ///
    /// Mods recorded as applied while the whole of `!BACKUP` is empty or gone.
    /// Then any file those mods displaced exists nowhere: JSGME **moved** the
    /// original into `!BACKUP` rather than copying it, and the 2.1 updater runs
    /// `DelTree` over `MODS`, `!BACKUP` included. The live texture tree is left
    /// holding modded files that look pristine.
    ///
    /// Deliberately not "this mod has fewer backups than log lines". That is
    /// also exactly what a mod which only adds files looks like, and flagging
    /// `Xbox Controls` as unrecoverable when nothing is wrong teaches the
    /// player to ignore the warning that matters. See
    /// [`AppliedMod::recorded_without_backup`].
    ///
    /// The same condition [`crate::migration::MigrationPlan`] blocks on, so the
    /// two cannot disagree about whether an install is safe to migrate.
    pub fn blocker(&self) -> Option<String> {
        if self.applied.is_empty() {
            return None;
        }

        // Deliberately NOT blocking on per-mod coverage.
        //
        // I tried that and it was wrong. JSGME writes a backup only when it
        // DISPLACES an existing file, so an add-only mod has recorded paths and
        // zero backups and that is entirely normal. On a real install
        // `Xbox Controls` shows 19 recorded paths and 0 backups, and the install
        // logs cannot say whether those 19 files existed beforehand.
        //
        // An empty !BACKUP alongside applied mods is the one unambiguous signal,
        // so that is what blocks. The ambiguous case is reported by
        // `unbacked_paths` and surfaced as a warning, because a false alarm that
        // stops a migration is its own kind of damage.
        if !self.backup_entries.is_empty() {
            return None;
        }
        let mut names: Vec<&str> = self.applied.iter().map(|m| m.name.as_str()).collect();
        names.sort_unstable();
        Some(format!(
            "{} mod(s) are recorded as applied ({}) but {BACKUP_DIRNAME} is {}, \
             so any original texture they displaced no longer exists anywhere. \
             The live texture tree holds modded files as if they were pristine. \
             Do not import it as a pristine tree: let the launcher fetch a clean \
             copy of the textures component first",
            self.applied.len(),
            names.join(", "),
            if self.backup_exists {
                "empty"
            } else {
                "missing"
            }
        ))
    }

    /// Whether the originals could still be put back.
    /// Applied mods with recorded paths that have no backup entry, as name and
    /// count.
    ///
    /// This is NOT proof of lost originals and must never be presented as such.
    /// JSGME backs up only a file it displaces, so an add-only mod legitimately
    /// has none. What it means is that these paths cannot be un-applied by
    /// restoring, and whether that matters depends on whether the file existed
    /// before the mod was applied, which the install logs do not record.
    ///
    /// Useful anyway: a caller about to treat the live tree as pristine should
    /// say which mods it cannot reason about, rather than implying it checked.
    pub fn unbacked_paths(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = self
            .applied
            .iter()
            .filter_map(|m| {
                let n = m.recorded_without_backup().len();
                if n > 0 {
                    Some((m.name.clone(), n))
                } else {
                    None
                }
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub fn originals_are_restorable(&self) -> bool {
        self.blocker().is_none()
    }
}

// ---------------------------------------------------------------------------
// Contested paths
// ---------------------------------------------------------------------------

/// Which mod's copy of a contested file the game is loading right now.
///
/// Three genuinely different answers, so three variants rather than an
/// `Option<String>`. "Nothing overwrote it" and "JSGME kept no record, so this
/// cannot be told" look the same as an absent name and are not the same fact:
/// the first means the pristine texture is in place, the second means nobody
/// knows what is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDisk {
    /// This mod was applied last, so its file is the one in the live tree.
    Applied(String),
    /// None of the mods on this path is applied, so the pristine file is there.
    NoneApplied,
    /// `JSGME.ini` is missing or unreadable, so which copy is live cannot be
    /// recovered.
    Unknown,
}

/// One path more than one mod would write.
///
/// Grouped by path and listing every mod, rather than reported pairwise. Seven
/// mods on one file is one fact to show the player; as pairs it is 21 rows
/// saying the same thing. [`crate::mods::detect_conflicts`] is pairwise because
/// it gates one specific activation; this is a description of a library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathOverlap {
    /// The path, relative to the JSGME root, as the first mod spelled it.
    pub relative: String,
    /// Every mod that writes it, by folder name, sorted.
    pub mods: Vec<String>,
    /// True when the mods do not agree on letter case.
    ///
    /// On Windows that is one file and the overlap is real. On ext4 it is two
    /// files and the overlap is not, so the same library behaves differently on
    /// the two, and that is flagged rather than resolved here.
    pub differing_case: bool,
    /// Which of these mods has its file in the live tree right now.
    ///
    /// `JSGME.ini` lists the applied mods and the order they were copied in,
    /// and the last copy survives, so the highest-numbered mod on a contested
    /// path is the one the game loads. A mod on the path with no number is not
    /// applied and wrote nothing, which is why a partial order still answers
    /// the question.
    pub on_disk: OnDisk,
}

impl PathOverlap {
    /// A sentence naming every mod and what the old system did about it.
    pub fn message(&self) -> String {
        let state = match &self.on_disk {
            OnDisk::Applied(name) => {
                format!(" {name} was applied last, so that is the file on disk now.")
            }
            OnDisk::NoneApplied => {
                " None of them is applied, so the original file is in place.".to_string()
            }
            OnDisk::Unknown => format!(
                " {JSGME_INI} left no usable order, so which copy is on disk \
                 cannot be told from it."
            ),
        };
        format!(
            "{} mods write {}: {}. JSGME copies them in order and keeps \
             whichever landed last, without saying so.{state}",
            self.mods.len(),
            self.relative,
            self.mods.join(", ")
        )
    }
}

// ---------------------------------------------------------------------------
// The library
// ---------------------------------------------------------------------------

/// A JSGME `MODS` directory, read and described.
#[derive(Debug, Clone)]
pub struct JsgmeLibrary {
    pub root: PathBuf,
    /// One per mod folder, sorted by folder name.
    pub mods: Vec<JsgmeMod>,
    /// Paths more than one mod writes.
    pub overlaps: Vec<PathOverlap>,
    pub applied: AppliedState,
    /// The applied order from `JSGME.ini`, lowest number first.
    ///
    /// JSGME's own record of the sequence it copied in, which is the only thing
    /// that says which mod won a contested path.
    pub applied_order: Vec<(String, u32)>,
    /// Present when `JSGME.ini` exists but could not be read, so a missing
    /// order is never mistaken for an absent file.
    pub order_unreadable: Option<String>,
    /// Folders that slug to one id, so none of them can be imported until they
    /// are renamed.
    pub duplicate_ids: Vec<(ModId, Vec<String>)>,
    /// Folders whose name could not be turned into a package id, with the
    /// reason. Described rather than skipped silently.
    pub unusable_names: Vec<(String, String)>,
    /// Entries at the top of `MODS/` that are not mod folders: `JSGME.ini`, the
    /// executable, the two `!` control directories, anything else.
    pub ignored: Vec<PathBuf>,
}

impl JsgmeLibrary {
    /// Read a `MODS` directory.
    ///
    /// `recognized` is the set of builds the caller actually has, normally the
    /// directory names under `library/textures/`. It is a parameter and not a
    /// list baked in here because this module cannot know which components are
    /// installed, and because a directory under `Textures/` is not proof of a
    /// build: `Xbox Controls` ships a `v2` working folder that passes
    /// [`GameId::parse`] on shape alone. Anything under an unrecognized
    /// directory is reported as [`SkipReason::UnknownBuild`] and never imported.
    ///
    /// An absent directory is an error rather than an empty library. Returning
    /// "0 mods" for a path that does not exist would read as the player's 11
    /// mods having vanished.
    pub fn scan(mods_root: &Path, recognized: &[GameId]) -> Result<Self> {
        if !mods_root.is_dir() {
            return Err(Error::Io {
                path: mods_root.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no JSGME MODS directory here. It sits next to the User \
                     folder inside a 2.1 install, so check the install root \
                     before treating this as an empty mod library",
                ),
            });
        }

        let mut mod_dirs: Vec<(String, PathBuf)> = Vec::new();
        let mut ignored = Vec::new();

        let reader = std::fs::read_dir(mods_root).map_err(|source| Error::Io {
            path: mods_root.to_path_buf(),
            source,
        })?;
        for entry in reader {
            let entry = entry.map_err(|source| Error::Io {
                path: mods_root.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            let raw = entry.file_name();
            let Some(name) = raw.to_str() else {
                ignored.push(path);
                continue;
            };
            // symlink_metadata, so a symlinked directory is classified rather
            // than descended into.
            let meta = std::fs::symlink_metadata(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            // JSGME reserves a leading `!` for its own bookkeeping, so a folder
            // starting with one is never a mod, even if it is not one of the two
            // this module knows by name.
            if !meta.is_dir() || name.starts_with('!') {
                ignored.push(path);
                continue;
            }
            mod_dirs.push((name.to_string(), path));
        }
        mod_dirs.sort_by(|a, b| a.0.cmp(&b.0));

        let mut mods = Vec::new();
        let mut unusable_names = Vec::new();
        for (name, folder) in &mod_dirs {
            let id = match slug_id(name) {
                Ok(id) => id,
                Err(e) => {
                    unusable_names.push((name.clone(), e.to_string()));
                    continue;
                }
            };
            mods.push(read_mod(name, id, folder, recognized)?);
        }

        let duplicate_ids = duplicate_ids(&mods);
        let order = read_applied_order(&mods_root.join(JSGME_INI));
        let present: BTreeSet<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        let applied = read_applied_state(mods_root, &present)?;
        let overlaps = detect_overlaps(&mods, &order);

        Ok(Self {
            root: mods_root.to_path_buf(),
            mods,
            overlaps,
            applied,
            applied_order: order.entries,
            order_unreadable: order.unreadable,
            duplicate_ids,
            unusable_names,
            ignored,
        })
    }

    /// A mod by folder name.
    pub fn find(&self, name: &str) -> Option<&JsgmeMod> {
        self.mods.iter().find(|m| m.name == name)
    }

    /// Bytes a conversion of the whole library would copy.
    pub fn total_bytes(&self) -> u64 {
        self.mods.iter().map(JsgmeMod::total_bytes).sum()
    }

    /// Work out what converting this library into `mods/packages/` would do,
    /// without doing any of it.
    ///
    /// Nothing is written anywhere. The destination is read only to the extent
    /// of asking whether each file is already there, and `MODS/` is not written
    /// to at all.
    ///
    /// Overlapping paths do **not** refuse the plan. Every mod converts, and
    /// [`crate::mods::Composer::plan`] refuses the combination if the player
    /// later turns two contested mods on together. Refusing to convert would
    /// leave the player with 20 contested paths in the system that cannot warn
    /// about them, which is the state being escaped.
    pub fn conversion_plan(
        &self,
        packages_root: &Path,
        version: &PackageVersion,
    ) -> Result<Conversion> {
        // The read-only rule, checked first. Everything else is irrelevant if
        // the destination is inside the only copy of the player's mods.
        if contains_or_equals(&self.root, packages_root) {
            return Ok(Conversion::Refused(
                ConversionRefusal::DestinationInsideMods {
                    mods_root: self.root.clone(),
                    packages_root: packages_root.to_path_buf(),
                },
            ));
        }
        if contains_or_equals(packages_root, &self.root) {
            return Ok(Conversion::Refused(
                ConversionRefusal::ModsInsideDestination {
                    mods_root: self.root.clone(),
                    packages_root: packages_root.to_path_buf(),
                },
            ));
        }
        if let Some((id, folders)) = self.duplicate_ids.first() {
            return Ok(Conversion::Refused(ConversionRefusal::DuplicateIds {
                id: id.clone(),
                folders: folders.clone(),
            }));
        }

        let mut packages = Vec::new();
        let mut skipped = Vec::new();
        for m in &self.mods {
            let package = match m.to_package(version) {
                Ok(p) => p,
                Err(e) => {
                    skipped.push((m.name.clone(), e.to_string()));
                    continue;
                }
            };
            // Zipped rather than re-derived: `to_package` builds one ModFile
            // per JsgmeFile in order, so this pairs each manifest entry with
            // the file it was built from and cannot drift.
            let mut files = Vec::new();
            for (file, source) in package.files.iter().zip(m.files.iter()) {
                let to = package.payload_file(packages_root, file);
                files.push(PlannedCopy {
                    from: source.source.clone(),
                    to: to.clone(),
                    bytes: file.bytes,
                    sha256: file.sha256.clone(),
                    already_present: to.exists(),
                });
            }
            let manifest_path = packages_root
                .join(package.id.as_str())
                .join(package.version.as_str())
                .join(PACKAGE_MANIFEST);
            packages.push(PlannedPackage {
                package,
                manifest_path,
                files,
            });
        }

        if packages.is_empty() {
            return Ok(Conversion::Refused(ConversionRefusal::NothingImportable {
                mods_root: self.root.clone(),
                skipped,
            }));
        }

        Ok(Conversion::Planned(ConversionPlan {
            mods_root: self.root.clone(),
            packages_root: packages_root.to_path_buf(),
            version: version.clone(),
            packages,
            skipped,
        }))
    }
}

// ---------------------------------------------------------------------------
// The conversion plan
// ---------------------------------------------------------------------------

/// One file the caller would copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCopy {
    /// Inside the mod folder. Read only.
    pub from: PathBuf,
    /// Inside `mods/packages/<id>/<version>/files/`.
    pub to: PathBuf,
    pub bytes: u64,
    /// The hash of `from` as read during the scan.
    ///
    /// Carried so the caller can check what it wrote without hashing the source
    /// again, and so a source that changed between planning and copying is
    /// detectable rather than imported under a hash that no longer describes it.
    pub sha256: Sha256Hex,
    /// Whether the destination already holds a file.
    ///
    /// Reported, not resolved. Overwriting would silently replace an already
    /// imported package, and skipping would leave a payload that does not match
    /// the manifest about to be written next to it. The caller decides which of
    /// those it means.
    pub already_present: bool,
}

/// One package the caller would create.
#[derive(Debug, Clone)]
pub struct PlannedPackage {
    /// Write this to [`PlannedPackage::manifest_path`] once every file has
    /// landed.
    ///
    /// Manifest last, for the reason [`crate::mods`] writes its composed marker
    /// in two stages: a manifest sitting next to a half-copied payload is a
    /// package that passes a load and fails a compose.
    pub package: ModPackage,
    pub manifest_path: PathBuf,
    pub files: Vec<PlannedCopy>,
}

impl PlannedPackage {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }

    /// Destinations that already hold a file.
    pub fn occupied(&self) -> Vec<&PlannedCopy> {
        self.files.iter().filter(|f| f.already_present).collect()
    }
}

/// Everything conversion would do, before any of it is done.
#[derive(Debug, Clone)]
pub struct ConversionPlan {
    pub mods_root: PathBuf,
    pub packages_root: PathBuf,
    /// The version the caller chose. Recorded so the plan states which one it
    /// used rather than leaving it implied.
    pub version: PackageVersion,
    pub packages: Vec<PlannedPackage>,
    /// Mods that will not convert, with the reason, so a library of 11 that
    /// produces 10 packages says which one it dropped.
    pub skipped: Vec<(String, String)>,
}

impl ConversionPlan {
    pub fn file_count(&self) -> usize {
        self.packages.iter().map(|p| p.files.len()).sum()
    }

    pub fn total_bytes(&self) -> u64 {
        self.packages.iter().map(PlannedPackage::total_bytes).sum()
    }

    /// Destinations that already hold a file, across every package.
    pub fn occupied(&self) -> Vec<&PlannedCopy> {
        self.packages
            .iter()
            .flat_map(PlannedPackage::occupied)
            .collect()
    }
}
// ---------------------------------------------------------------------------
// Reconstructing a pristine tree
// ---------------------------------------------------------------------------

/// What it would take to produce an unmodded copy of a texture tree.
///
/// Composition layers mods over a PRISTINE tree, and a live 2.1 install does not
/// have one: JSGME applies by overwriting in place, so the tree on disk is the
/// base with whatever is currently applied baked into it. Copying it as-is
/// would make those mods permanent and unremovable, and every composition on
/// top would be layering over a tree that already contains a mod.
///
/// So the copy is un-applied. For each path an install log records:
///
///   a backup exists  -> the backup IS the original, and is written over the
///                       copy
///   no backup exists -> JSGME backs up only what it DISPLACES, so no backup
///                       means the file did not exist before the mod added it,
///                       and it is removed from the copy
///
/// The second rule is the one worth stating twice, because "no backup" reads
/// like missing data and is not. On this library nineteen of the prompt
/// textures have no backup, and all nineteen are files the mod introduced.
#[derive(Debug, Clone)]
pub struct PristinePlan {
    pub build: GameId,
    /// The live tree, read only.
    pub from: PathBuf,
    /// Where the pristine copy goes.
    pub to: PathBuf,
    pub files_to_copy: usize,
    pub bytes_to_copy: u64,
    /// Backups to write over the copy, as (backup file, path within the tree).
    pub restore: Vec<(PathBuf, String)>,
    /// Paths to delete from the copy, because the mod added them.
    pub remove: Vec<String>,
    /// Why this cannot be done, when it cannot.
    ///
    /// An unreadable install log blocks it: the log is the only record of what a
    /// mod wrote, so without it there is no way to know which files to reverse,
    /// and a tree that MIGHT still contain a mod is not a pristine tree.
    pub blocked: Option<String>,
}

impl PristinePlan {
    /// How many paths are being reversed in total.
    pub fn reversing(&self) -> usize {
        self.restore.len() + self.remove.len()
    }
}

/// What the reconstruction did.
#[derive(Debug, Clone)]
pub struct PristineReport {
    pub files_copied: usize,
    pub bytes_copied: u64,
    pub restored: usize,
    /// Originals restored to a path the copied tree did not have.
    ///
    /// Worth its own count, because it means the pristine tree is NOT a subset
    /// of the live one. It happens when a mod-written file was later deleted or
    /// moved out of the live textures: the install log still names the path, the
    /// backup still holds the original, and the original belongs in a pristine
    /// tree whether or not the live tree kept it. On the real install this was
    /// 9 of 11, which showed up as an unexplained file count until it was
    /// counted separately.
    pub restored_absent: Vec<String>,
    pub removed: usize,
    /// Paths that were supposed to be reversed and were not there to reverse.
    ///
    /// Reported rather than ignored. It means the install log and the tree
    /// disagree, and a caller should know the result is not exactly what was
    /// planned even though nothing failed.
    pub missing: Vec<String>,
}

impl PristineReport {
    pub fn is_exact(&self) -> bool {
        self.missing.is_empty()
    }
}

impl JsgmeLibrary {
    /// Work out how to produce a pristine copy of one build's texture tree.
    ///
    /// Reads only. `install_root` is the folder holding `User` and `MODS`, which
    /// is the root the install logs are written relative to.
    pub fn pristine_plan(
        &self,
        install_root: &Path,
        build: &GameId,
        to: &Path,
    ) -> Result<PristinePlan> {
        let prefix = format!("User/Load/Textures/{}/", build.as_str());
        let from = install_root
            .join("User")
            .join("Load")
            .join("Textures")
            .join(build.as_str());

        let mut plan = PristinePlan {
            build: build.clone(),
            from: from.clone(),
            to: to.to_path_buf(),
            files_to_copy: 0,
            bytes_to_copy: 0,
            restore: Vec::new(),
            remove: Vec::new(),
            blocked: None,
        };

        if !from.is_dir() {
            plan.blocked = Some(format!(
                "there is no texture tree at {}, so there is nothing to copy",
                from.display()
            ));
            return Ok(plan);
        }
        // The destination must not already hold a tree. Overwriting one would
        // replace a pristine tree that compositions are already layered over,
        // and there is no way to tell a stale one from a good one.
        if to.is_dir() && std::fs::read_dir(to)?.next().is_some() {
            plan.blocked = Some(format!(
                "{} already holds files. A pristine tree is what every \
                 composition is layered over, so it is never overwritten. Move \
                 it aside first if it is wrong.",
                to.display()
            ));
            return Ok(plan);
        }
        if contains_or_equals(&from, to) || contains_or_equals(to, &from) {
            plan.blocked = Some(format!(
                "{} and {} contain one another, so copying would read what it \
                 is writing",
                from.display(),
                to.display()
            ));
            return Ok(plan);
        }

        // An unreadable log means an unknown set of writes.
        for applied in &self.applied.applied {
            if let Some(why) = &applied.unreadable {
                plan.blocked = Some(format!(
                    "the install log for {} could not be read ({why}), so there \
                     is no way to know which files it wrote. A tree that might \
                     still contain a mod is not a pristine tree.",
                    applied.name
                ));
                return Ok(plan);
            }
        }

        // Every backup, keyed by the path it came from.
        let mut backups: BTreeMap<&str, &BackupEntry> = BTreeMap::new();
        for entry in &self.applied.backup_entries {
            if let Some(original) = entry.original_relative.as_deref() {
                backups.insert(original, entry);
            }
        }

        for applied in &self.applied.applied {
            for path in &applied.recorded_paths {
                let Some(tail) = path.strip_prefix(&prefix) else {
                    // A path for a different build, or outside the texture tree
                    // entirely. Not this plan's business.
                    continue;
                };
                match backups.get(path.as_str()) {
                    Some(entry) => plan
                        .restore
                        .push((self.applied.backup_root.join(&entry.relative), tail.to_string())),
                    None => plan.remove.push(tail.to_string()),
                }
            }
        }

        plan.restore.sort();
        plan.remove.sort();

        let (files, bytes) = count_tree(&from)?;
        plan.files_to_copy = files;
        plan.bytes_to_copy = bytes;
        Ok(plan)
    }
}

/// Files and bytes under a directory.
fn count_tree(root: &Path) -> Result<(usize, u64)> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|source| Error::Io {
            path: dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| Error::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(meta) = entry.metadata() {
                files += 1;
                bytes += meta.len();
            }
        }
    }
    Ok((files, bytes))
}

impl PristinePlan {
    /// Copy the tree, then un-apply every mod from the copy.
    ///
    /// The source is never written to. Everything happens in the destination,
    /// so a failure part way leaves the player's install exactly as it was and
    /// costs nothing but the copy.
    ///
    /// Order matters: copy everything first, then reverse. Reversing during the
    /// copy would mean deciding file by file whether a path is a mod's, and a
    /// single missed decision produces a tree that looks pristine and is not.
    pub fn execute(&self) -> Result<PristineReport> {
        if let Some(why) = &self.blocked {
            return Err(invalid(why.clone()));
        }

        std::fs::create_dir_all(&self.to).map_err(|source| Error::Io {
            path: self.to.clone(),
            source,
        })?;
        let bytes_copied = crate::migration::copy_tree_no_clobber(&self.from, &self.to)?;
        let (files_copied, _) = count_tree(&self.to)?;

        let mut restored = 0usize;
        let mut restored_absent = Vec::new();
        let mut removed = 0usize;
        let mut missing = Vec::new();

        for (backup, tail) in &self.restore {
            let dest = join_relative(&self.to, tail);
            if !backup.is_file() {
                missing.push(tail.clone());
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            // Recorded BEFORE the copy, which is the only moment the answer
            // is knowable: fs::copy creates the destination either way.
            if !dest.exists() {
                restored_absent.push(tail.clone());
            }
            std::fs::copy(backup, &dest).map_err(|source| Error::Io {
                path: dest.clone(),
                source,
            })?;
            restored += 1;
        }

        for tail in &self.remove {
            let dest = join_relative(&self.to, tail);
            if !dest.exists() {
                // The log says a mod wrote it and the tree does not have it.
                // Nothing to undo, and worth reporting: the two disagree.
                missing.push(tail.clone());
                continue;
            }
            std::fs::remove_file(&dest).map_err(|source| Error::Io {
                path: dest.clone(),
                source,
            })?;
            removed += 1;
        }

        missing.sort();
        restored_absent.sort();
        Ok(PristineReport {
            files_copied,
            bytes_copied,
            restored,
            restored_absent,
            removed,
            missing,
        })
    }
}

/// Join a forward-slash relative path onto a root, one segment at a time.
///
/// Segment by segment rather than as one string, so a path written with forward
/// slashes lands correctly on a platform that uses backslashes.
fn join_relative(root: &Path, relative: &str) -> PathBuf {
    let mut out = root.to_path_buf();
    for segment in relative.split('/').filter(|s| !s.is_empty()) {
        out = out.join(segment);
    }
    out
}

// ---------------------------------------------------------------------------
// Executing a plan
// ---------------------------------------------------------------------------

/// What to do about a destination that already holds a file.
///
/// The plan reports these and does not resolve them, so this is where the
/// caller says what it meant. There is no default: the two answers do opposite
/// things to an already imported package, and picking one here would be this
/// module deciding on the player's behalf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occupied {
    /// Refuse the whole import, writing nothing.
    ///
    /// The safe answer, and the one to offer first. A destination that is
    /// already occupied usually means the import has already been run, and a
    /// second run would replace packages the player may have composed from.
    Refuse,
    /// Replace what is there.
    ///
    /// For re-importing after the source mods changed. The manifest is rewritten
    /// with the payload, so the pair stays consistent.
    Replace,
}

/// What executing a plan did.
#[derive(Debug, Clone)]
pub struct ImportReport {
    /// Packages whose payload and manifest are both on disk.
    pub imported: Vec<ModId>,
    pub files_copied: usize,
    pub bytes_copied: u64,
    /// Packages that were not attempted, carried through from the plan.
    pub skipped: Vec<(String, String)>,
    /// Packages that were attempted and abandoned, with the reason.
    ///
    /// Each of these left nothing behind: the package directory is removed
    /// before this is reported, so a failure cannot leave a payload that no
    /// manifest describes.
    pub failed: Vec<(ModId, String)>,
}

impl ImportReport {
    /// Whether every package that could be imported was.
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }
}

impl ConversionPlan {
    /// Copy every payload and write every manifest.
    ///
    /// The only function in this module that writes anything. `MODS/` is not
    /// written to at all: every source path is read, and the JSGME library is
    /// left exactly as it was, which is what makes a failed import cost nothing.
    ///
    /// Order matters, per package: create the directory, copy the files, verify
    /// each one against the hash the scan already took, and write the manifest
    /// LAST. A manifest sitting next to a half-copied payload is a package that
    /// passes [`ModPackage::load`] and fails a compose, which is a fault that
    /// surfaces at the moment the player tries to play.
    ///
    /// A package that fails part way is removed rather than left. Only
    /// directories this call created are removed, so a pre-existing package
    /// cannot be deleted by another package's failure.
    ///
    /// One package failing does not abandon the rest. The report names both, and
    /// ten of eleven packages is a better outcome than none, provided nobody is
    /// told it was eleven.
    pub fn execute(&self, occupied: Occupied) -> Result<ImportReport> {
        // Checked before anything is written, so a refusal costs nothing.
        if occupied == Occupied::Refuse {
            let taken = self.occupied();
            if !taken.is_empty() {
                return Err(invalid(format!(
                    "{} destination(s) already hold a file, the first being {}. Nothing was written. Overwriting would replace an already imported package, so pass Occupied::Replace to mean that, or import into an empty packages root.",
                    taken.len(),
                    taken[0].to.display()
                )));
            }
        }

        let mut report = ImportReport {
            imported: Vec::new(),
            files_copied: 0,
            bytes_copied: 0,
            skipped: self.skipped.clone(),
            failed: Vec::new(),
        };

        for planned in &self.packages {
            // The version directory, which is what gets removed if this package
            // does not finish. Taken from the manifest path rather than rebuilt,
            // so it cannot disagree with where the manifest is going.
            let Some(package_dir) = planned.manifest_path.parent() else {
                report.failed.push((
                    planned.package.id.clone(),
                    "its manifest path has no parent directory".to_string(),
                ));
                continue;
            };

            let existed = package_dir.exists();
            match write_package(planned, package_dir) {
                Ok((files, bytes)) => {
                    report.imported.push(planned.package.id.clone());
                    report.files_copied += files;
                    report.bytes_copied += bytes;
                }
                Err(e) => {
                    // Remove only what this call created. A package directory
                    // that was already there belongs to a previous import, and
                    // deleting it would turn one failure into two.
                    if !existed {
                        let _ = std::fs::remove_dir_all(package_dir);
                    }
                    report.failed.push((planned.package.id.clone(), e.to_string()));
                }
            }
        }

        Ok(report)
    }
}

/// Copy one package's payload and write its manifest. Returns files and bytes.
fn write_package(planned: &PlannedPackage, package_dir: &Path) -> Result<(usize, u64)> {
    let mut files = 0usize;
    let mut bytes = 0u64;

    for copy in &planned.files {
        if let Some(parent) = copy.to.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let payload = std::fs::read(&copy.from).map_err(|source| Error::Io {
            path: copy.from.clone(),
            source,
        })?;

        // Against the hash the scan already took. A source that changed between
        // planning and copying would otherwise be imported under a hash that no
        // longer describes it, and the manifest would then be wrong about its
        // own payload for the life of the package.
        let actual = Sha256Hex::from_bytes(&payload);
        if actual != copy.sha256 {
            return Err(invalid(format!(
                "{} changed between being read and being copied. Expected {}, found {}. Nothing about this package was kept. Re-run the import to read it again.",
                copy.from.display(),
                copy.sha256,
                actual
            )));
        }

        std::fs::write(&copy.to, &payload).map_err(|source| Error::Io {
            path: copy.to.clone(),
            source,
        })?;
        files += 1;
        bytes += payload.len() as u64;
    }

    // Last, and only now.
    //
    // Written through serde rather than assembled as text: the manifest has to
    // be readable by ModPackage::load, and a hand built one that drifts from the
    // struct would be a package that imports and will not compose.
    let manifest = serde_json::to_vec_pretty(&planned.package).map_err(|e| {
        invalid(format!(
            "could not encode the manifest for {}: {e}",
            planned.package.id.as_str()
        ))
    })?;
    std::fs::write(&planned.manifest_path, manifest).map_err(|source| Error::Io {
        path: planned.manifest_path.clone(),
        source,
    })?;

    // Read it back through the same path a composition will. A manifest that
    // serializes and does not load is a package that only fails later, when
    // someone is trying to start a game.
    ModPackage::load(package_dir)?;

    Ok((files, bytes))
}


/// A state that stops conversion, with something the caller can do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversionRefusal {
    /// The destination is inside `MODS/`, so converting would write into the
    /// only copy of the player's mods.
    DestinationInsideMods {
        mods_root: PathBuf,
        packages_root: PathBuf,
    },
    /// `MODS/` is inside the destination, so anything allowed to clear the
    /// package directory would take the mod library with it.
    ModsInsideDestination {
        mods_root: PathBuf,
        packages_root: PathBuf,
    },
    /// Two or more folders slug to one package id.
    DuplicateIds { id: ModId, folders: Vec<String> },
    /// No mod in the library has an importable file.
    NothingImportable {
        mods_root: PathBuf,
        skipped: Vec<(String, String)>,
    },
}

impl ConversionRefusal {
    pub fn message(&self) -> String {
        match self {
            ConversionRefusal::DestinationInsideMods {
                mods_root,
                packages_root,
            } => format!(
                "Refusing to convert into {} because it is inside the JSGME mod \
                 folder {}. That folder is the only copy of the player's mod \
                 payloads and importing must not write into it. Point the \
                 destination at mods/packages under the launcher's data root.",
                packages_root.display(),
                mods_root.display()
            ),
            ConversionRefusal::ModsInsideDestination {
                mods_root,
                packages_root,
            } => format!(
                "Refusing to convert into {} because the JSGME mod folder {} is \
                 inside it. Anything allowed to clear the package directory \
                 would delete the mod library with it. Point the destination at \
                 mods/packages under the launcher's data root.",
                packages_root.display(),
                mods_root.display()
            ),
            ConversionRefusal::DuplicateIds { id, folders } => format!(
                "The mod folders {} all produce the package id {id}. One id is \
                 one mod to the activation list, so turning one on would turn \
                 the others off with nothing said. Rename all but one and scan \
                 again.",
                folders.join(", ")
            ),
            ConversionRefusal::NothingImportable { mods_root, skipped } => format!(
                "Nothing in {} can be imported. {} mod(s) were examined and each \
                 one was skipped, the first because: {}",
                mods_root.display(),
                skipped.len(),
                skipped
                    .first()
                    .map(|(name, why)| format!("{name}: {why}"))
                    .unwrap_or_else(|| "there are no mod folders".to_string())
            ),
        }
    }
}

/// The result of planning a conversion.
#[derive(Debug, Clone)]
pub enum Conversion {
    Planned(ConversionPlan),
    Refused(ConversionRefusal),
}

impl Conversion {
    pub fn plan(&self) -> Option<&ConversionPlan> {
        match self {
            Conversion::Planned(p) => Some(p),
            Conversion::Refused(_) => None,
        }
    }

    pub fn refusal(&self) -> Option<&ConversionRefusal> {
        match self {
            Conversion::Refused(r) => Some(r),
            Conversion::Planned(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

fn read_mod(name: &str, id: ModId, folder: &Path, recognized: &[GameId]) -> Result<JsgmeMod> {
    let walked = walk_tree(folder)?;
    let mut files = Vec::new();
    let mut skipped = Vec::new();

    for file in walked.files {
        match texture_target(&file.relative, recognized) {
            Ok(payload) => files.push(JsgmeFile {
                mirror_relative: file.relative,
                // Hashed from the file rather than assumed, because this hash
                // is what composition later verifies the composed tree against.
                sha256: hash_file(&file.path)?,
                source: file.path,
                bytes: file.bytes,
                payload,
            }),
            Err(why) => skipped.push(Unimportable {
                mirror_relative: file.relative,
                bytes: file.bytes,
                why,
            }),
        }
    }

    Ok(JsgmeMod {
        name: name.to_string(),
        id,
        folder: folder.to_path_buf(),
        files,
        skipped,
        unsupported: walked.unsupported,
    })
}

/// Where composition would put a mirrored file, or why it will not be imported.
fn texture_target(
    mirror_relative: &str,
    recognized: &[GameId],
) -> std::result::Result<PayloadPath, SkipReason> {
    let mut segments = mirror_relative.split('/');
    // Compared segment by segment and case insensitively rather than by
    // stripping a lowercased prefix: `to_lowercase` can change a string's byte
    // length, so slicing by the lowercased length would cut a non-ASCII path in
    // the wrong place and produce a payload path for a different file.
    for want in TEXTURE_MIRROR_PREFIX.split('/') {
        match segments.next() {
            Some(got) if got.eq_ignore_ascii_case(want) => {}
            _ => return Err(SkipReason::OutsideTextureMirror),
        }
    }

    let rest: Vec<&str> = segments.collect();
    let Some((build, tail)) = rest.split_first() else {
        return Err(SkipReason::OutsideTextureMirror);
    };
    if tail.is_empty() {
        return Err(SkipReason::NoBuildDirectory);
    }
    let game = GameId::parse(build).map_err(|_| SkipReason::NotABuildDirectory {
        name: (*build).to_string(),
    })?;
    if !recognized.contains(&game) {
        return Err(SkipReason::UnknownBuild {
            name: (*build).to_string(),
        });
    }
    PayloadPath::parse(&rest.join("/")).map_err(|e| SkipReason::RejectedPayloadPath {
        detail: e.to_string(),
    })
}

fn duplicate_ids(mods: &[JsgmeMod]) -> Vec<(ModId, Vec<String>)> {
    let mut by_id: BTreeMap<&ModId, Vec<String>> = BTreeMap::new();
    for m in mods {
        by_id.entry(&m.id).or_default().push(m.name.clone());
    }
    by_id
        .into_iter()
        .filter(|(_, names)| names.len() > 1)
        .map(|(id, names)| (id.clone(), names))
        .collect()
}

fn detect_overlaps(mods: &[JsgmeMod], order: &AppliedOrder) -> Vec<PathOverlap> {
    // Folded, because Windows treats two spellings as one file. Folding can
    // only report an overlap a case-sensitive filesystem would not have had,
    // and that direction costs a message while the other costs a texture.
    let mut by_path: BTreeMap<String, (String, Vec<String>, bool)> = BTreeMap::new();
    for m in mods {
        for relative in m.written_paths() {
            let entry = by_path
                .entry(relative.to_lowercase())
                .or_insert_with(|| (relative.to_string(), Vec::new(), false));
            if entry.0 != relative {
                entry.2 = true;
            }
            if !entry.1.contains(&m.name) {
                entry.1.push(m.name.clone());
            }
        }
    }

    let rank: BTreeMap<&str, u32> = order
        .entries
        .iter()
        .map(|(n, i)| (n.as_str(), *i))
        .collect();
    by_path
        .into_values()
        .filter(|(_, names, _)| names.len() > 1)
        .map(|(relative, mut names, differing_case)| {
            names.sort();
            // JSGME copies in ascending order and the last copy survives, so
            // among the mods it recorded as applied, the highest number is the
            // file actually on disk. A mod with no number is not applied and
            // wrote nothing, so a partial order still answers this. With no
            // usable order at all, nothing is claimed.
            let on_disk = if order.usable() {
                let mut ranked: Vec<(u32, &String)> = names
                    .iter()
                    .filter_map(|n| rank.get(n.as_str()).map(|i| (*i, n)))
                    .collect();
                ranked.sort();
                match ranked.pop() {
                    Some((_, name)) => OnDisk::Applied(name.clone()),
                    None => OnDisk::NoneApplied,
                }
            } else {
                OnDisk::Unknown
            };
            PathOverlap {
                relative,
                mods: names,
                differing_case,
                on_disk,
            }
        })
        .collect()
}

/// What `JSGME.ini` said about the applied order.
#[derive(Debug, Default)]
struct AppliedOrder {
    entries: Vec<(String, u32)>,
    /// Whether the file was there at all.
    ///
    /// Tracked separately from `entries` being empty, because a readable file
    /// with an empty `[MODS]` section means nothing is applied, while an absent
    /// file means the order is simply unknown. Collapsing the two would make the
    /// launcher answer "which mod is on disk" with a guess in one of them.
    present: bool,
    /// Present only when the file exists and could not be read.
    unreadable: Option<String>,
}

impl AppliedOrder {
    fn unreadable(reason: String) -> Self {
        Self {
            entries: Vec::new(),
            present: true,
            unreadable: Some(reason),
        }
    }

    /// Whether this order can be treated as the authority on what is applied.
    fn usable(&self) -> bool {
        self.present && self.unreadable.is_none()
    }
}

/// Read `JSGME.ini`'s `[MODS]` section.
///
/// Parsed here rather than through [`crate::ini::IniDocument`] because that type
/// reads a key it is given and has no way to enumerate the keys in a section,
/// and here the mod names are the keys.
fn read_applied_order(path: &Path) -> AppliedOrder {
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        // No file means JSGME never recorded an order. That is a different fact
        // from a file that could not be read, so only the second is reported as
        // a problem.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return AppliedOrder::default(),
        Err(source) => {
            return AppliedOrder::unreadable(format!(
                "{} could not be read: {source}",
                path.display()
            ))
        }
    };
    let Ok(text) = String::from_utf8(raw) else {
        return AppliedOrder::unreadable(format!(
            "{} is not UTF-8, so the applied order could not be read. Which mod \
             won a contested path cannot be recovered from it",
            path.display()
        ));
    };

    let mut in_section = false;
    let mut order = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
            in_section = trimmed[1..trimmed.len() - 1]
                .trim()
                .eq_ignore_ascii_case(JSGME_INI_SECTION);
            continue;
        }
        if !in_section || trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#')
        {
            continue;
        }
        // rsplit_once, because a mod name may contain an `=` and the number
        // never does.
        if let Some((name, index)) = trimmed.rsplit_once('=') {
            if let Ok(index) = index.trim().parse::<u32>() {
                order.push((name.trim().to_string(), index));
            }
        }
    }
    order.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    AppliedOrder {
        entries: order,
        present: true,
        unreadable: None,
    }
}

fn read_applied_state(mods_root: &Path, present: &BTreeSet<&str>) -> Result<AppliedState> {
    let instlogs_root = mods_root.join(INSTLOGS_DIRNAME);
    let backup_root = mods_root.join(BACKUP_DIRNAME);
    let mut state = AppliedState {
        instlogs_exists: instlogs_root.is_dir(),
        backup_exists: backup_root.is_dir(),
        instlogs_root,
        backup_root,
        ..AppliedState::default()
    };

    let mut logs: Vec<(String, PathBuf)> = Vec::new();
    if state.instlogs_exists {
        let walked = walk_tree(&state.instlogs_root)?;
        for file in walked.files {
            match log_mod_name(&file.relative) {
                Some(name) => logs.push((name, file.path)),
                None => state.unrecognized_logs.push(file.path),
            }
        }
        state.unrecognized_logs.extend(walked.unsupported);
    }
    logs.sort_by(|a, b| a.0.cmp(&b.0));

    if state.backup_exists {
        let walked = walk_tree(&state.backup_root)?;
        let names: Vec<&str> = logs.iter().map(|(n, _)| n.as_str()).collect();
        for file in walked.files {
            let (displaced_by, original_relative) = attribute_backup(&file.relative, &names);
            state.backup_entries.push(BackupEntry {
                relative: file.relative,
                displaced_by,
                original_relative,
                bytes: file.bytes,
            });
        }
    }

    for (name, log) in logs {
        let (recorded_paths, unreadable) = read_install_log(&log);
        let backup_paths = state
            .backup_entries
            .iter()
            .filter(|e| e.displaced_by.as_deref() == Some(name.as_str()))
            .filter_map(|e| e.original_relative.clone())
            .collect();
        state.applied.push(AppliedMod {
            folder_present: present.contains(name.as_str()),
            name,
            log,
            recorded_paths,
            backup_paths,
            unreadable,
        });
    }

    Ok(state)
}

/// The mod name an install log file belongs to.
///
/// Any `.log` file at the top of `!INSTLOGS` counts as evidence something was
/// applied, so the full suffix is stripped when it is there and the extension
/// alone when it is not. Under-counting applied mods is the direction that
/// matters: it is what would let [`AppliedState::blocker`] stay quiet about a
/// tree whose originals are gone.
fn log_mod_name(relative: &str) -> Option<String> {
    // Only files at the top level. JSGME does not nest install logs, so a
    // nested file is reported as unrecognized rather than read as a mod name.
    if relative.contains('/') {
        return None;
    }
    if let Some(stem) = relative.strip_suffix(INSTALL_LOG_SUFFIX) {
        if !stem.is_empty() {
            return Some(stem.to_string());
        }
    }
    let (stem, extension) = relative.rsplit_once('.')?;
    if extension.eq_ignore_ascii_case("log") && !stem.is_empty() {
        return Some(stem.to_string());
    }
    None
}

/// Read the paths an install log records, normalizing separators.
fn read_install_log(path: &Path) -> (Vec<String>, Option<String>) {
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(source) => {
            return (
                Vec::new(),
                Some(format!(
                    "{} could not be read: {source}. The mod is still applied; \
                     which files it wrote is now unknown",
                    path.display()
                )),
            )
        }
    };
    let Ok(text) = String::from_utf8(raw) else {
        return (
            Vec::new(),
            Some(format!(
                "{} is not UTF-8, so which files this mod wrote could not be \
                 read. The mod is still applied",
                path.display()
            )),
        );
    };
    let paths = text
        .lines()
        // JSGME writes Windows separators. Normalized so these compare equal to
        // the paths walked out of a mod folder, which use forward slashes. Left
        // unnormalized, every comparison fails and the launcher reports that
        // none of the applied files was ever applied.
        .map(|l| l.trim().replace('\\', "/"))
        .filter(|l| !l.is_empty())
        .collect();
    (paths, None)
}

/// Which mod displaced a backup file, and where it came from.
///
/// The shipped library names a backup `<original path>.<Mod Name>`. The longest
/// matching name wins, so a mod called `Controls` cannot claim a file belonging
/// to `Xbox Controls`. No match leaves both answers absent rather than assuming
/// the unsuffixed convention, because an original restored to the wrong path is
/// worse than one the launcher admits it cannot place.
fn attribute_backup(relative: &str, mod_names: &[&str]) -> (Option<String>, Option<String>) {
    let mut best: Option<&str> = None;
    for name in mod_names {
        let suffix = format!(".{name}");
        if relative.ends_with(&suffix) && best.map(|b| name.len() > b.len()).unwrap_or(true) {
            best = Some(name);
        }
    }
    match best {
        // `relative` ends with `.{name}`, so this many bytes are the original
        // path and the subtraction cannot underflow.
        Some(name) => {
            let cut = relative.len() - name.len() - 1;
            (Some(name.to_string()), Some(relative[..cut].to_string()))
        }
        None => (None, None),
    }
}

// ---------------------------------------------------------------------------
// Walking
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct WalkedFile {
    relative: String,
    path: PathBuf,
    bytes: u64,
}

#[derive(Debug, Default)]
struct Walked {
    files: Vec<WalkedFile>,
    unsupported: Vec<PathBuf>,
}

/// Walk a tree, classifying by `symlink_metadata` so a symlinked directory is
/// never descended into and cannot make the walk circle.
fn walk_tree(root: &Path) -> Result<Walked> {
    let mut out = Walked::default();
    let mut stack = vec![(root.to_path_buf(), String::new())];

    while let Some((dir, prefix)) = stack.pop() {
        let reader = std::fs::read_dir(&dir).map_err(|source| Error::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in reader {
            let entry = entry.map_err(|source| Error::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            let raw = entry.file_name();
            let Some(name) = raw.to_str() else {
                out.unsupported.push(path);
                continue;
            };
            let relative = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            let meta = std::fs::symlink_metadata(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            let kind = meta.file_type();
            if kind.is_dir() {
                stack.push((path, relative));
            } else if kind.is_file() {
                out.files.push(WalkedFile {
                    relative,
                    path,
                    bytes: meta.len(),
                });
            } else {
                out.unsupported.push(path);
            }
        }
    }

    out.files.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(out)
}

/// Whether `inner` is `outer` or is inside it.
///
/// Component by component and case folded, not a string prefix, so `MODS2` is
/// not treated as being inside `MODS`. Folded on every platform because the
/// only cost of folding is refusing a conversion that would have been fine,
/// while the cost of missing a containment is writing into the player's mod
/// library.
fn contains_or_equals(outer: &Path, inner: &Path) -> bool {
    let segments = |p: &Path| -> Vec<String> {
        std::path::absolute(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect()
    };
    let (o, i) = (segments(outer), segments(inner));
    i.len() >= o.len() && i[..o.len()] == o[..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::detect_conflicts;

    /// The single path all seven football mods write, from the real library.
    const BALL: &str = "User/Load/Textures/GN7E69/tex1_128x128_m_ffc5151a141db330_14.dds";

    fn gn7e69() -> GameId {
        GameId::parse("GN7E69").expect("GN7E69 is a valid game id")
    }

    fn recognized() -> Vec<GameId> {
        vec![gn7e69()]
    }

    fn version() -> PackageVersion {
        PackageVersion::parse("imported").expect("`imported` is a valid version string")
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, bytes).expect("write file");
    }

    /// Lay out a mod folder holding a mirror of the game tree, JSGME style.
    fn lay_mod(mods_root: &Path, name: &str, files: &[(&str, &[u8])]) {
        for (relative, bytes) in files {
            let mut path = mods_root.join(name);
            for segment in relative.split('/') {
                path = path.join(segment);
            }
            write(&path, bytes);
        }
    }

    /// A `MODS` folder with both control directories present and empty, which
    /// is what the shipped 2.1 install looks like with nothing applied.
    fn lay_mods_root(dir: &Path) -> PathBuf {
        let root = dir.join("MODS");
        std::fs::create_dir_all(root.join(BACKUP_DIRNAME)).expect("create !BACKUP");
        std::fs::create_dir_all(root.join(INSTLOGS_DIRNAME)).expect("create !INSTLOGS");
        root
    }

    /// Record a mod as applied: an install log, and one backup per displaced
    /// original.
    fn lay_applied(mods_root: &Path, name: &str, recorded: &[&str], displaced: &[&str]) {
        let log = mods_root
            .join(INSTLOGS_DIRNAME)
            .join(format!("{name}{INSTALL_LOG_SUFFIX}"));
        let body: String = recorded
            .iter()
            .map(|p| format!("{}\r\n", p.replace('/', "\\")))
            .collect();
        write(&log, body.as_bytes());

        for original in displaced {
            let mut path = mods_root.join(BACKUP_DIRNAME);
            let segments: Vec<&str> = original.split('/').collect();
            for (i, segment) in segments.iter().enumerate() {
                if i + 1 == segments.len() {
                    path = path.join(format!("{segment}.{name}"));
                } else {
                    path = path.join(segment);
                }
            }
            write(&path, b"the original texture");
        }
    }

    /// Every file under a root, as (relative path, byte length), sorted.
    fn snapshot(root: &Path) -> Vec<(String, u64)> {
        let walked = walk_tree(root).expect("walk");
        let mut out: Vec<(String, u64)> = walked
            .files
            .into_iter()
            .map(|f| (f.relative, f.bytes))
            .collect();
        out.sort();
        out
    }

    // -----------------------------------------------------------------------
    // The hard rule: this module reads
    // -----------------------------------------------------------------------

    /// `MODS/` is the only copy of the player's mod payloads and `!BACKUP` the
    /// only copy of any original they displaced. Nothing in this module may add,
    /// remove or resize a single file in there.
    #[test]
    fn scanning_and_planning_never_touch_the_mods_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);
        lay_mod(
            &mods_root,
            "Xbox Controls",
            &[
                ("User/Load/Textures/GN7E69/btn_a.png", b"a"),
                ("User/Load/Textures/v2/btn_a.png", b"working copy"),
            ],
        );
        lay_applied(&mods_root, "Purple Football", &[BALL], &[BALL]);

        let before = snapshot(&mods_root);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let plan = library
            .conversion_plan(&dir.path().join("packages"), &version())
            .expect("plan");
        assert!(plan.plan().is_some(), "the plan should not have refused");

        assert_eq!(
            before,
            snapshot(&mods_root),
            "the MODS folder changed. It is the only copy of the player's mods \
             and of any original they displaced"
        );
    }

    /// The destination must never land inside `MODS/`. That would be this
    /// module writing into the folder it exists to read.
    #[test]
    fn converting_into_the_mods_folder_is_refused_both_ways() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);
        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        for inside in [mods_root.clone(), mods_root.join("packages")] {
            let refusal = library
                .conversion_plan(&inside, &version())
                .expect("planning is not fallible here")
                .refusal()
                .cloned()
                .unwrap_or_else(|| panic!("{} must be refused", inside.display()));
            assert!(
                matches!(refusal, ConversionRefusal::DestinationInsideMods { .. }),
                "got {refusal:?}"
            );
            assert!(
                refusal.message().contains("mods/packages"),
                "the refusal must say where to point it: {}",
                refusal.message()
            );
        }

        // And the other direction, where clearing the destination would delete
        // the mod library.
        let refusal = library
            .conversion_plan(dir.path(), &version())
            .expect("planning is not fallible here")
            .refusal()
            .cloned()
            .expect("a parent of MODS must be refused");
        assert!(
            matches!(refusal, ConversionRefusal::ModsInsideDestination { .. }),
            "got {refusal:?}"
        );
    }

    /// A sibling directory whose name merely starts with the same characters is
    /// not inside `MODS`, and refusing it would block a legitimate import.
    #[test]
    fn a_sibling_named_like_the_mods_folder_is_not_inside_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);
        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        let sibling = dir.path().join("MODS2");
        assert!(
            library
                .conversion_plan(&sibling, &version())
                .expect("plan")
                .plan()
                .is_some(),
            "MODS2 is not inside MODS"
        );
    }

    // -----------------------------------------------------------------------
    // Applied state, with nothing applied as the normal case
    // -----------------------------------------------------------------------

    /// The state the shipped machine is in most of the time: both control
    /// directories present and empty. Nothing applied, nothing to warn about,
    /// and the mods are still fully described.
    #[test]
    fn empty_backup_and_instlogs_means_nothing_applied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);
        lay_mod(&mods_root, "Pink Football", &[(BALL, b"pink ball!!")]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert!(library.applied.instlogs_exists);
        assert!(library.applied.backup_exists);
        assert!(
            library.applied.nothing_applied(),
            "an empty !INSTLOGS is nothing applied"
        );
        assert!(library.applied.backup_entries.is_empty());
        assert_eq!(
            library.applied.blocker(),
            None,
            "an empty !BACKUP with nothing applied is not a problem, it is the \
             ordinary state"
        );
        assert!(library.applied.originals_are_restorable());
        assert_eq!(library.mods.len(), 2, "the mods are still described");
    }

    /// Both control directories missing entirely is also nothing applied, and
    /// must not be read as an emptied backup.
    #[test]
    fn missing_backup_and_instlogs_means_nothing_applied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = dir.path().join("MODS");
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert!(!library.applied.instlogs_exists);
        assert!(!library.applied.backup_exists);
        assert!(library.applied.nothing_applied());
        assert_eq!(
            library.applied.blocker(),
            None,
            "no install logs means nothing was applied, whatever !BACKUP looks like"
        );
    }

    /// The state that must stop the caller. Mods applied, `!BACKUP` emptied, so
    /// the originals those mods moved aside exist nowhere and the live texture
    /// tree is modded while looking pristine.
    #[test]
    fn applied_with_an_empty_backup_is_reported_as_unrestorable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Aqua Football", &[(BALL, b"aqua ball!!")]);
        // Applied, and the backup that should hold the original is not there.
        lay_applied(&mods_root, "Aqua Football", &[BALL], &[]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(library.applied.applied.len(), 1);
        let blocker = library
            .applied
            .blocker()
            .expect("applied with no backup at all must block");
        assert!(blocker.contains(BACKUP_DIRNAME), "{blocker}");
        assert!(blocker.contains("Aqua Football"), "{blocker}");
        assert!(
            blocker.contains("as if they were pristine"),
            "the reason must say what goes wrong, not just that something did: \
             {blocker}"
        );
        assert!(!library.applied.originals_are_restorable());
    }

    /// A missing `!BACKUP` is as bad as an empty one, and the message must say
    /// which it was so the user can tell a deleted folder from an empty one.
    #[test]
    fn applied_with_a_missing_backup_says_missing_not_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = dir.path().join("MODS");
        lay_mod(&mods_root, "Aqua Football", &[(BALL, b"aqua ball!!")]);
        lay_applied(&mods_root, "Aqua Football", &[BALL], &[]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let blocker = library.applied.blocker().expect("must block");
        assert!(blocker.contains("missing"), "{blocker}");
        assert!(!library.applied.backup_exists);
    }

    /// With the originals in `!BACKUP` the state is recoverable, and each
    /// applied mod is matched to the files it displaced.
    #[test]
    fn applied_with_a_backup_is_restorable_and_attributed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Aqua Football", &[(BALL, b"aqua ball!!")]);
        lay_applied(&mods_root, "Aqua Football", &[BALL], &[BALL]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(library.applied.blocker(), None);
        assert!(library.applied.originals_are_restorable());
        let applied = &library.applied.applied[0];
        assert_eq!(applied.name, "Aqua Football");
        assert!(applied.folder_present);
        assert_eq!(applied.recorded_paths, vec![BALL.to_string()]);
        assert_eq!(
            applied.backup_paths,
            vec![BALL.to_string()],
            "the backup must be traced back to the path it came from, or it \
             cannot be put back"
        );
        assert!(applied.recorded_without_backup().is_empty());
    }

    /// The false alarm that would train the player to ignore the real one.
    /// `Xbox Controls` writes 19 files, displaces none of them because the
    /// pristine tree does not contain them, and so has no backup entries at
    /// all. Nothing is lost and nothing may claim otherwise.
    #[test]
    fn an_add_only_mod_with_no_backup_entry_is_not_a_lost_original() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        let prompt = "User/Load/Textures/GN7E69/btn_a.png";
        lay_mod(&mods_root, "Aqua Football", &[(BALL, b"aqua ball!!")]);
        lay_mod(&mods_root, "Xbox Controls", &[(prompt, b"prompt")]);
        // One mod displaced an original, the other only added a file.
        lay_applied(&mods_root, "Aqua Football", &[BALL], &[BALL]);
        lay_applied(&mods_root, "Xbox Controls", &[prompt], &[]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(
            library.applied.blocker(),
            None,
            "!BACKUP holds entries, so nothing is unrecoverable. An add-only mod \
             having none of its own is normal and must not read as data loss"
        );
        let xbox = library
            .applied
            .applied
            .iter()
            .find(|m| m.name == "Xbox Controls")
            .expect("Xbox Controls is applied");
        assert!(xbox.backup_paths.is_empty());
        assert_eq!(
            xbox.recorded_without_backup(),
            vec![prompt],
            "the fact is still reported, it is just not called a lost original"
        );
    }

    /// An applied mod whose folder was deleted cannot be un-applied by JSGME
    /// either, so the state is worth naming rather than silently dropping.
    #[test]
    fn an_applied_mod_with_no_folder_is_still_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_applied(&mods_root, "Deleted Mod", &[BALL], &[BALL]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert!(library.mods.is_empty());
        assert_eq!(library.applied.applied.len(), 1);
        assert!(
            !library.applied.applied[0].folder_present,
            "the payload JSGME would need in order to un-apply this is gone"
        );
    }

    /// Install logs record Windows separators. Unnormalized, no recorded path
    /// would ever match a walked one and the launcher would report that none of
    /// the applied files was applied.
    #[test]
    fn install_log_separators_are_normalized_to_match_walked_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Aqua Football", &[(BALL, b"aqua ball!!")]);
        lay_applied(&mods_root, "Aqua Football", &[BALL], &[BALL]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let recorded = &library.applied.applied[0].recorded_paths;
        assert_eq!(recorded, &vec![BALL.to_string()]);
        assert!(
            !recorded[0].contains('\\'),
            "a backslash here never matches a walked path: {recorded:?}"
        );

        let walked: Vec<&str> = library
            .find("Aqua Football")
            .expect("the mod is there")
            .written_paths()
            .collect();
        assert_eq!(walked, vec![BALL]);
    }

    // -----------------------------------------------------------------------
    // Contested paths
    // -----------------------------------------------------------------------

    /// The real shape of the library: seven mods on one file. JSGME copies in
    /// order and keeps the last, so the report must name every mod rather than
    /// pick one.
    #[test]
    fn every_mod_on_a_contested_path_is_named() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        let colors = [
            "Aqua Football",
            "Blue Football",
            "Camo Football",
            "Green Football",
            "Panda Football",
            "Pink Football",
            "Purple Football",
        ];
        for name in colors {
            lay_mod(&mods_root, name, &[(BALL, name.as_bytes())]);
        }

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(library.overlaps.len(), 1, "one path, one finding");
        let overlap = &library.overlaps[0];
        assert_eq!(overlap.relative, BALL);
        assert_eq!(overlap.mods.len(), 7);
        for name in colors {
            assert!(
                overlap.mods.contains(&name.to_string()),
                "{name} is missing from {:?}",
                overlap.mods
            );
        }
        assert!(
            overlap.message().contains("keeps"),
            "the message must say what JSGME did: {}",
            overlap.message()
        );
    }

    /// `JSGME.ini` is the only record of the order, and the order is the only
    /// thing that says which file is actually on disk.
    #[test]
    fn the_recorded_order_says_which_mod_won() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Aqua Football", &[(BALL, b"aqua ball!!")]);
        lay_mod(&mods_root, "Pink Football", &[(BALL, b"pink ball!!")]);
        write(
            &mods_root.join(JSGME_INI),
            b"[MODS]\r\nAqua Football=1\r\nPink Football=2\r\n",
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(
            library.applied_order,
            vec![
                ("Aqua Football".to_string(), 1),
                ("Pink Football".to_string(), 2)
            ]
        );
        assert_eq!(library.order_unreadable, None);
        assert_eq!(
            library.overlaps[0].on_disk,
            OnDisk::Applied("Pink Football".to_string()),
            "JSGME copies in ascending order and the last write survives"
        );
    }

    /// The real machine's shape: one mod on a contested path is applied and the
    /// others are not. `JSGME.ini` lists only applied mods, so a partial order
    /// still says which file the game is loading, and answering "unknown" here
    /// would throw away a fact the launcher has.
    #[test]
    fn a_partial_order_still_says_which_file_is_live() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        for name in ["Aqua Football", "Pink Football", "Purple Football"] {
            lay_mod(&mods_root, name, &[(BALL, name.as_bytes())]);
        }
        // Only Aqua is applied, so only Aqua is in the order.
        write(&mods_root.join(JSGME_INI), b"[MODS]\r\nAqua Football=1\r\n");
        lay_applied(&mods_root, "Aqua Football", &[BALL], &[BALL]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(library.overlaps[0].mods.len(), 3);
        assert_eq!(
            library.overlaps[0].on_disk,
            OnDisk::Applied("Aqua Football".to_string()),
            "the two unapplied mods wrote nothing, so the applied one is live"
        );
    }

    /// A readable order that names none of the contesting mods means none of
    /// them is applied, which is a different claim from not knowing.
    #[test]
    fn a_readable_order_naming_none_of_them_means_none_is_applied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Aqua Football", &[(BALL, b"aqua ball!!")]);
        lay_mod(&mods_root, "Pink Football", &[(BALL, b"pink ball!!")]);
        write(&mods_root.join(JSGME_INI), b"[MODS]\r\n");

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(library.overlaps[0].on_disk, OnDisk::NoneApplied);
        assert!(
            library.overlaps[0].message().contains("original file"),
            "{}",
            library.overlaps[0].message()
        );
    }

    /// With no `JSGME.ini` at all, which mod won cannot be recovered, and the
    /// type says unknown rather than guessing the first or the alphabetically
    /// last, and rather than claiming nothing is applied.
    #[test]
    fn without_a_recorded_order_the_winner_is_unknown_not_guessed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Aqua Football", &[(BALL, b"aqua ball!!")]);
        lay_mod(&mods_root, "Pink Football", &[(BALL, b"pink ball!!")]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert!(library.applied_order.is_empty());
        assert_eq!(
            library.order_unreadable, None,
            "an absent file is not an unreadable one"
        );
        assert_eq!(library.overlaps[0].on_disk, OnDisk::Unknown);
        let text = library.overlaps[0].message();
        assert!(
            !text.contains("that is the file on disk now") && !text.contains("original file"),
            "unknown must not be reported as either answer: {text}"
        );
    }

    /// Two spellings of one name are one file on Windows and two on ext4, so
    /// the overlap is reported and flagged rather than resolved.
    #[test]
    fn contested_paths_are_compared_case_insensitively() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(
            &mods_root,
            "Alpha",
            &[("User/Load/Textures/GN7E69/ball.dds", b"alpha")],
        );
        lay_mod(
            &mods_root,
            "Beta",
            &[("User/Load/Textures/GN7E69/BALL.DDS", b"beta!")],
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(
            library.overlaps.len(),
            1,
            "on Windows these are one file, so this is a real overlap"
        );
        assert!(library.overlaps[0].differing_case);
        assert_eq!(library.overlaps[0].mods, vec!["Alpha", "Beta"]);
    }

    /// A file this module will not import is still a file JSGME would have
    /// overwritten, so it belongs in the overlap report.
    #[test]
    fn a_contested_path_counts_even_when_neither_side_is_importable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        let work = "User/Load/Textures/v2/btn_a.png";
        lay_mod(&mods_root, "Alpha", &[(work, b"alpha")]);
        lay_mod(&mods_root, "Beta", &[(work, b"beta!")]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(library.overlaps.len(), 1);
        assert_eq!(library.overlaps[0].relative, work);
    }

    // -----------------------------------------------------------------------
    // What is derived, and what is refused
    // -----------------------------------------------------------------------

    /// Sizes and hashes must come off the file. A size taken from a directory
    /// listing is rounded to the allocation unit, and
    /// `Composer::plan` refuses a package whose declared size does not match
    /// its payload, so a rounded size fails every correct import.
    #[test]
    fn sizes_and_hashes_are_read_from_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        let content: &[u8] = b"the purple football, 27 bytes";
        lay_mod(&mods_root, "Purple Football", &[(BALL, content)]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let file = &library
            .find("Purple Football")
            .expect("the mod is there")
            .files[0];

        assert_eq!(file.bytes, content.len() as u64);
        assert_eq!(file.sha256, Sha256Hex::from_bytes(content));
        assert_eq!(
            file.payload.as_str(),
            "GN7E69/tex1_128x128_m_ffc5151a141db330_14.dds"
        );
        assert_eq!(file.mirror_relative, BALL);
    }

    /// The `Xbox Controls/User/Load/Textures/v2/` case. `v2` passes
    /// `GameId::parse` on shape, so a scan that trusted the directory name would
    /// publish a package claiming to target a build that does not exist.
    #[test]
    fn an_unrecognized_build_directory_is_never_imported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(
            &mods_root,
            "Xbox Controls",
            &[
                ("User/Load/Textures/GN7E69/btn_a.png", b"real"),
                ("User/Load/Textures/v2/btn_a.png", b"working copy"),
                ("User/Load/Textures/v2/btn_a - Copy.png", b"working copy"),
            ],
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let m = library.find("Xbox Controls").expect("the mod is there");

        assert_eq!(m.files.len(), 1, "only the recognized build is imported");
        assert_eq!(m.targets(), vec![gn7e69()], "no invented target");
        assert_eq!(m.skipped.len(), 2);
        for skipped in &m.skipped {
            assert_eq!(
                skipped.why,
                SkipReason::UnknownBuild {
                    name: "v2".to_string()
                }
            );
            assert!(
                skipped.why.message().contains("v2"),
                "{}",
                skipped.why.message()
            );
        }

        let package = m.to_package(&version()).expect("one importable file");
        assert_eq!(package.targets, vec![gn7e69()]);
    }

    /// Files outside the texture mirror are described and left alone, not
    /// imported as textures they are not.
    #[test]
    fn files_outside_the_texture_mirror_are_described_not_imported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(
            &mods_root,
            "PS Controls",
            &[
                ("User/Load/Textures/GN7E69/btn_a.png", b"prompt"),
                ("User/Config/GCPadNew.ini", b"[GCPad1]"),
                ("readme.txt", b"by somebody"),
                ("User/Load/Textures/loose.dds", b"no build directory"),
            ],
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let m = library.find("PS Controls").expect("the mod is there");

        assert_eq!(m.files.len(), 1);
        let reasons: Vec<&SkipReason> = m.skipped.iter().map(|s| &s.why).collect();
        assert!(reasons.contains(&&SkipReason::OutsideTextureMirror));
        assert!(reasons.contains(&&SkipReason::NoBuildDirectory));
        assert_eq!(
            m.skipped.len(),
            3,
            "every skipped file is named, not counted: {:?}",
            m.skipped
        );
    }

    /// A texture the package format cannot carry must be refused by name rather
    /// than imported under an extension composition will not accept.
    #[test]
    fn a_payload_path_the_package_format_refuses_is_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(
            &mods_root,
            "Odd Mod",
            &[("User/Load/Textures/GN7E69/notes.txt", b"not a texture")],
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let m = library.find("Odd Mod").expect("the mod is there");

        assert!(m.files.is_empty());
        assert!(matches!(
            m.skipped[0].why,
            SkipReason::RejectedPayloadPath { .. }
        ));
    }

    /// A mod with nothing importable produces no package at all. An empty
    /// package would activate, change nothing, and look like a broken mod.
    #[test]
    fn a_mod_with_nothing_importable_produces_no_package() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(
            &mods_root,
            "Work In Progress",
            &[("User/Load/Textures/v2/btn_a.png", b"working copy")],
        );
        lay_mod(
            &mods_root,
            "Purple Football",
            &[(BALL, b"the purple football")],
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let wip = library.find("Work In Progress").expect("the mod is there");
        let err = wip
            .to_package(&version())
            .expect_err("nothing importable means no package");
        assert!(err.to_string().contains("Work In Progress"), "{err}");

        // And the rest of the library still converts, naming what it dropped.
        let conversion = library
            .conversion_plan(&dir.path().join("packages"), &version())
            .expect("plan");
        let plan = conversion.plan().expect("one mod is importable");
        assert_eq!(plan.packages.len(), 1);
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].0, "Work In Progress");
    }

    /// A library where nothing can be imported refuses rather than returning an
    /// empty plan that would read as success.
    #[test]
    fn a_library_with_nothing_importable_refuses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(
            &mods_root,
            "Work In Progress",
            &[("User/Load/Textures/v2/btn_a.png", b"working copy")],
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let refusal = library
            .conversion_plan(&dir.path().join("packages"), &version())
            .expect("planning is not fallible here")
            .refusal()
            .cloned()
            .expect("nothing importable must refuse");
        assert!(matches!(
            refusal,
            ConversionRefusal::NothingImportable { .. }
        ));
        assert!(
            refusal.message().contains("Work In Progress"),
            "{}",
            refusal.message()
        );
    }

    /// One id shared by two packages is two versions of one mod to the
    /// activation list, so turning one on would turn the other off with nothing
    /// said. Refused, never disambiguated by numbering.
    #[test]
    fn two_folders_that_slug_to_one_id_are_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "PS Controls", &[(BALL, b"one")]);
        lay_mod(&mods_root, "ps-controls", &[(BALL, b"two")]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        assert_eq!(library.duplicate_ids.len(), 1);

        let refusal = library
            .conversion_plan(&dir.path().join("packages"), &version())
            .expect("planning is not fallible here")
            .refusal()
            .cloned()
            .expect("a duplicate id must refuse");
        let ConversionRefusal::DuplicateIds { id, folders } = &refusal else {
            panic!("got {refusal:?}");
        };
        assert_eq!(id.as_str(), "jsgme.ps-controls");
        assert_eq!(folders.len(), 2);
        assert!(
            refusal.message().contains("Rename"),
            "{}",
            refusal.message()
        );
    }

    /// The id is derived mechanically and namespaced, so an imported mod can
    /// never collide with a curated `reborn.*` package of the same name.
    #[test]
    fn ids_are_slugged_and_namespaced() {
        for (folder, expected) in [
            ("Patriots Throwback", "jsgme.patriots-throwback"),
            ("PS Controls", "jsgme.ps-controls"),
            ("Aqua Football", "jsgme.aqua-football"),
            ("  spaced  out  ", "jsgme.spaced-out"),
            ("Mod (v2)!", "jsgme.mod-v2"),
        ] {
            let id = slug_id(folder).unwrap_or_else(|e| panic!("{folder}: {e}"));
            assert_eq!(id.as_str(), expected);
            assert!(id.as_str().starts_with(IMPORTED_ID_PREFIX));
        }
    }

    /// A name that cannot be slugged is refused with something to do about it.
    /// Numbering or truncating would produce an id that means nothing and can
    /// collide.
    #[test]
    fn a_name_that_cannot_be_slugged_is_refused_with_a_remedy() {
        let err = slug_id("!!!").expect_err("no letters or digits");
        assert!(err.to_string().contains("Rename the folder"), "{err}");

        let long = "x".repeat(200);
        let err = slug_id(&long).expect_err("too long for a package id");
        assert!(err.to_string().contains("Rename the folder"), "{err}");
    }

    /// A folder with an unusable name must not take the whole scan down with
    /// it. The other ten mods are still described.
    #[test]
    fn an_unusable_folder_name_does_not_stop_the_scan() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "!!!", &[(BALL, b"nameless")]);
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        // A leading `!` is JSGME's own namespace, so this folder is ignored
        // rather than reported as an unusable name.
        assert_eq!(library.mods.len(), 1);
        assert_eq!(library.mods[0].name, "Purple Football");
        assert!(
            library.ignored.iter().any(|p| p.ends_with("!!!")),
            "nothing may be dropped without being reported: {:?}",
            library.ignored
        );
    }

    // -----------------------------------------------------------------------
    // The bridge actually fits mods.rs
    // -----------------------------------------------------------------------

    /// The point of the module. An imported package must satisfy the same
    /// validation a curated one does, and two imported mods on one path must
    /// hard-conflict from their file lists alone, with no `conflict_group` to
    /// help. If that did not hold, importing would produce packages that look
    /// fine and refuse to compose.
    #[test]
    fn imported_packages_validate_and_still_conflict_without_a_group() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);
        lay_mod(&mods_root, "Pink Football", &[(BALL, b"pink ball!!")]);
        lay_mod(
            &mods_root,
            "Saints Alternate",
            &[("User/Load/Textures/GN7E69/saints.dds", b"saints")],
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let packages: Vec<ModPackage> = library
            .mods
            .iter()
            .map(|m| {
                m.to_package(&version())
                    .unwrap_or_else(|e| panic!("{}: {e}", m.name))
            })
            .collect();

        for package in &packages {
            package
                .validate()
                .unwrap_or_else(|e| panic!("{} does not validate: {e}", package.id));
            assert_eq!(
                package.conflict_group, None,
                "nothing in a JSGME mod says which group it is in"
            );
        }

        let conflicts = detect_conflicts(&packages, &gn7e69(), None);
        assert_eq!(
            conflicts.len(),
            1,
            "the two footballs must conflict and the jersey must not: {conflicts:?}"
        );
        assert!(
            conflicts[0].is_hard(),
            "a shared path is a hard conflict even with no group declared"
        );
    }

    /// The version is the caller's, visibly, because JSGME records none. A
    /// stamped `1.0.0` would be this module inventing the field that decides
    /// which payload directory composition reads.
    #[test]
    fn the_version_comes_from_the_caller_and_lands_in_the_payload_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);
        let packages_root = dir.path().join("packages");

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let chosen = PackageVersion::parse("2026-09-01").expect("a valid version string");
        let conversion = library
            .conversion_plan(&packages_root, &chosen)
            .expect("plan");
        let plan = conversion.plan().expect("one importable mod");

        assert_eq!(plan.version, chosen);
        let package = &plan.packages[0];
        assert_eq!(package.package.version, chosen);
        assert_eq!(
            package.manifest_path,
            packages_root
                .join("jsgme.purple-football")
                .join("2026-09-01")
                .join(PACKAGE_MANIFEST)
        );
        assert!(
            package.files[0].to.starts_with(&packages_root),
            "every destination must be under the packages root: {}",
            package.files[0].to.display()
        );
    }

    /// The plan describes work, it does not do it, and it must be honest about
    /// bytes so the caller can say what a conversion costs.
    #[test]
    fn the_plan_reports_the_cost_and_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"1234567890")]);
        lay_mod(
            &mods_root,
            "Saints Alternate",
            &[("User/Load/Textures/GN7E69/saints.dds", b"12345")],
        );
        let packages_root = dir.path().join("packages");

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let conversion = library
            .conversion_plan(&packages_root, &version())
            .expect("plan");
        let plan = conversion.plan().expect("both mods import");

        assert_eq!(plan.file_count(), 2);
        assert_eq!(plan.total_bytes(), 15);
        assert_eq!(library.total_bytes(), 15);
        assert!(plan.occupied().is_empty());
        assert!(
            !packages_root.exists(),
            "planning must not create the destination"
        );
    }

    /// A destination that already holds a file is reported, not resolved.
    /// Overwriting would silently replace an imported package and skipping
    /// would leave a payload that disagrees with its manifest.
    #[test]
    fn an_occupied_destination_is_reported_rather_than_resolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);
        let packages_root = dir.path().join("packages");

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let first = library
            .conversion_plan(&packages_root, &version())
            .expect("plan");
        let destination = first.plan().expect("one importable mod").packages[0].files[0]
            .to
            .clone();
        write(&destination, b"an earlier import");

        let second = library
            .conversion_plan(&packages_root, &version())
            .expect("plan");
        let occupied = second.plan().expect("still planned").occupied();
        assert_eq!(occupied.len(), 1);
        assert_eq!(occupied[0].to, destination);
        assert_eq!(
            std::fs::read(&destination).expect("read back"),
            b"an earlier import",
            "planning must not have overwritten it"
        );
    }

    /// Contested paths do not stop conversion. Every mod is imported, and
    /// composition refuses the combination later. Refusing to convert would
    /// leave the player inside the system that cannot warn them.
    #[test]
    fn contested_paths_do_not_stop_conversion() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);
        lay_mod(&mods_root, "Pink Football", &[(BALL, b"pink ball!!")]);

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        assert_eq!(library.overlaps.len(), 1, "the overlap is still reported");

        let conversion = library
            .conversion_plan(&dir.path().join("packages"), &version())
            .expect("plan");
        assert_eq!(
            conversion
                .plan()
                .expect("both mods must convert")
                .packages
                .len(),
            2
        );
    }

    // -----------------------------------------------------------------------
    // Scanning nothing, and scanning nowhere
    // -----------------------------------------------------------------------

    /// A path that is not there must not report an empty library. "0 mods"
    /// would read as the player's 11 mods having vanished.
    #[test]
    fn a_missing_mods_directory_is_an_error_that_says_where_to_look() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("MODS");
        let err = JsgmeLibrary::scan(&missing, &recognized())
            .expect_err("an absent directory is not an empty library");
        let text = err.to_string();
        assert!(text.contains("MODS"), "{text}");
        assert!(text.contains("install root"), "{text}");
    }

    /// A `MODS` folder holding only JSGME's own files is a real state: the
    /// player has no mods. Zero is the answer, and it is not an error.
    #[test]
    fn a_mods_folder_with_no_mods_scans_to_zero() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        write(&mods_root.join(JSGME_INI), b"[MODS]\r\n");

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert!(library.mods.is_empty());
        assert!(library.overlaps.is_empty());
        assert!(library.applied.nothing_applied());
        assert_eq!(library.total_bytes(), 0);
        assert_eq!(
            library.ignored.len(),
            3,
            "!BACKUP, !INSTLOGS and JSGME.ini are accounted for, not dropped: \
             {:?}",
            library.ignored
        );
    }

    /// An empty `recognized` list imports nothing, and says why per file rather
    /// than returning a library of empty mods.
    #[test]
    fn with_no_recognized_builds_nothing_is_imported_and_the_reason_is_given() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Purple Football", &[(BALL, b"purple ball")]);

        let library = JsgmeLibrary::scan(&mods_root, &[]).expect("scan");
        let m = library.find("Purple Football").expect("the mod is there");

        assert!(m.files.is_empty());
        assert_eq!(
            m.skipped[0].why,
            SkipReason::UnknownBuild {
                name: "GN7E69".to_string()
            }
        );
    }

    /// A file in `!INSTLOGS` that is not an install log still needs saying,
    /// because it may be the evidence that something was applied.
    #[test]
    fn an_unrecognized_file_in_instlogs_is_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        write(
            &mods_root.join(INSTLOGS_DIRNAME).join("notes.txt"),
            b"not a log",
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert!(library.applied.nothing_applied());
        assert_eq!(library.applied.unrecognized_logs.len(), 1);
    }

    /// A backup whose name carries no mod suffix cannot be traced back to a
    /// mod, and guessing would restore a file to the wrong path.
    #[test]
    fn an_unattributable_backup_is_left_unattributed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(&mods_root, "Aqua Football", &[(BALL, b"aqua ball!!")]);
        lay_applied(&mods_root, "Aqua Football", &[BALL], &[]);
        // A backup in the unsuffixed convention.
        write(
            &mods_root
                .join(BACKUP_DIRNAME)
                .join("User")
                .join("Load")
                .join("Textures")
                .join("GN7E69")
                .join("tex1_128x128_m_ffc5151a141db330_14.dds"),
            b"the original football",
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");

        assert_eq!(library.applied.backup_entries.len(), 1);
        let entry = &library.applied.backup_entries[0];
        assert_eq!(entry.displaced_by, None);
        assert_eq!(entry.original_relative, None);
        assert_eq!(
            library.applied.blocker(),
            None,
            "!BACKUP is not empty, so the originals are still on the machine \
             even though this one cannot be attributed"
        );
        assert!(library.applied.applied[0].backup_paths.is_empty());
    }

    /// A mod name that is a suffix of another must not steal its backups.
    #[test]
    fn backup_attribution_prefers_the_longest_matching_mod_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mods_root = lay_mods_root(dir.path());
        lay_mod(
            &mods_root,
            "Controls",
            &[("User/Load/Textures/GN7E69/a.png", b"short")],
        );
        lay_mod(
            &mods_root,
            "Xbox Controls",
            &[("User/Load/Textures/GN7E69/b.png", b"long!")],
        );
        lay_applied(
            &mods_root,
            "Controls",
            &["User/Load/Textures/GN7E69/a.png"],
            &[],
        );
        lay_applied(
            &mods_root,
            "Xbox Controls",
            &["User/Load/Textures/GN7E69/b.png"],
            &["User/Load/Textures/GN7E69/b.png"],
        );

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).expect("scan");
        let entry = library
            .applied
            .backup_entries
            .iter()
            .find(|e| e.relative.ends_with("Xbox Controls"))
            .expect("the backup is there");
        assert_eq!(entry.displaced_by.as_deref(), Some("Xbox Controls"));
        assert_eq!(
            entry.original_relative.as_deref(),
            Some("User/Load/Textures/GN7E69/b.png")
        );
    }
    // ---- executing a plan --------------------------------------------------

    /// A JSGME library on disk: one mod, one texture, under the build given.
    fn a_library(root: &Path, mod_name: &str, build: &str) {
        let dir = root
            .join("MODS")
            .join(mod_name)
            .join("User")
            .join("Load")
            .join("Textures")
            .join(build);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tex_1234.png"), b"a texture").unwrap();
    }

    fn planned(root: &Path, dest: &Path) -> ConversionPlan {
        let build = GameId::parse("GN7E69").unwrap();
        let library = JsgmeLibrary::scan(&root.join("MODS"), &[build]).unwrap();
        let version = PackageVersion::parse("1.0.0").unwrap();
        match library.conversion_plan(dest, &version).unwrap() {
            Conversion::Planned(plan) => plan,
            Conversion::Refused(r) => panic!("planning refused: {}", r.message()),
        }
    }

    #[test]
    fn executing_a_plan_leaves_a_package_that_loads() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        a_library(src.path(), "Aqua Football", "GN7E69");

        let plan = planned(src.path(), dest.path());
        let report = plan.execute(Occupied::Refuse).unwrap();

        assert!(report.is_complete(), "failed: {:?}", report.failed);
        assert_eq!(report.imported.len(), 1);
        assert_eq!(report.files_copied, 1);

        // Through the same door a composition uses. Anything less proves only
        // that bytes were written somewhere.
        let dir = dest.path().join("jsgme.aqua-football").join("1.0.0");
        let package = ModPackage::load(&dir).unwrap();
        assert_eq!(package.name, "Aqua Football");
        assert_eq!(package.files.len(), 1);
    }

    #[test]
    fn the_source_library_is_never_written_to() {
        // The whole reason a failed import costs nothing. If MODS were touched,
        // a half finished import would damage the only copy of the player's
        // mods, which is the state being escaped.
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        a_library(src.path(), "Aqua Football", "GN7E69");

        let before = tree(&src.path().join("MODS"));
        planned(src.path(), dest.path())
            .execute(Occupied::Refuse)
            .unwrap();
        assert_eq!(before, tree(&src.path().join("MODS")), "MODS changed");
    }

    #[test]
    fn an_occupied_destination_is_refused_before_anything_is_written() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        a_library(src.path(), "Aqua Football", "GN7E69");

        // First import succeeds.
        planned(src.path(), dest.path())
            .execute(Occupied::Refuse)
            .unwrap();

        // Second refuses rather than silently replacing a package the player
        // may already have composed from.
        let err = planned(src.path(), dest.path())
            .execute(Occupied::Refuse)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("already hold a file"),
            "the refusal must say why: {err}"
        );
        assert!(
            err.contains("Replace"),
            "and must name the way to mean it: {err}"
        );

        // Saying so explicitly works.
        let report = planned(src.path(), dest.path())
            .execute(Occupied::Replace)
            .unwrap();
        assert!(report.is_complete(), "failed: {:?}", report.failed);
    }

    #[test]
    fn a_source_that_changed_after_planning_is_refused_and_leaves_nothing() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        a_library(src.path(), "Aqua Football", "GN7E69");

        let plan = planned(src.path(), dest.path());

        // Someone edits the mod between the scan and the copy. Importing it
        // anyway would write a manifest whose hash does not describe its own
        // payload, and nothing would notice until a verify.
        let file = src
            .path()
            .join("MODS")
            .join("Aqua Football")
            .join("User")
            .join("Load")
            .join("Textures")
            .join("GN7E69")
            .join("tex_1234.png");
        std::fs::write(&file, b"a DIFFERENT texture").unwrap();

        let report = plan.execute(Occupied::Refuse).unwrap();
        assert!(report.imported.is_empty(), "nothing may be imported");
        assert_eq!(report.failed.len(), 1);
        assert!(
            report.failed[0].1.contains("changed between"),
            "the reason must name the cause: {}",
            report.failed[0].1
        );

        // And no payload left behind for a later run to trust.
        let dir = dest.path().join("jsgme.aqua-football").join("1.0.0");
        assert!(
            !dir.exists(),
            "a failed package must not leave a directory: {}",
            dir.display()
        );
    }

    #[test]
    fn one_package_failing_does_not_abandon_the_others() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        a_library(src.path(), "Aqua Football", "GN7E69");
        a_library(src.path(), "Blue Football", "GN7E69");

        let plan = planned(src.path(), dest.path());
        assert_eq!(plan.packages.len(), 2, "both must be planned");

        // Break exactly one source.
        let broken = src
            .path()
            .join("MODS")
            .join("Aqua Football")
            .join("User")
            .join("Load")
            .join("Textures")
            .join("GN7E69")
            .join("tex_1234.png");
        std::fs::write(&broken, b"changed").unwrap();

        let report = plan.execute(Occupied::Refuse).unwrap();
        assert_eq!(report.imported.len(), 1, "the good one must still import");
        assert_eq!(report.failed.len(), 1, "and the broken one must be named");
        assert!(!report.is_complete(), "and the report must not claim success");

        // The survivor is whole.
        ModPackage::load(&dest.path().join("jsgme.blue-football").join("1.0.0")).unwrap();
    }

    /// Every path under a root with its bytes, for proving nothing changed.
    fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(bytes) = std::fs::read(&path) {
                    out.push((path, bytes));
                }
            }
        }
        out.sort();
        out
    }

    // ---- reconstructing a pristine tree ------------------------------------
    //
    // Composition layers mods over a pristine tree, and a live install has
    // none: JSGME applies by overwriting in place. Copying that tree as-is
    // would bake the applied mods in permanently, so the copy is un-applied.

    /// A live install: an unmodded base plus two mods applied over it, one that
    /// displaced a file and one that only added one.
    fn lay_live_install(root: &Path) -> PathBuf {
        let mods_root = lay_mods_root(root);
        let textures = root
            .join("User")
            .join("Load")
            .join("Textures")
            .join("GN7E69");
        std::fs::create_dir_all(&textures).unwrap();

        // Untouched base content, which must survive exactly.
        write(&textures.join("crowd.dds"), b"base crowd");
        // Displaced by a mod, so the tree currently holds the MOD's bytes.
        write(&textures.join("ball.dds"), b"aqua ball!!");
        // Added by a mod, so it is not part of the base at all.
        write(&textures.join("btn_a.png"), b"xbox prompt");

        lay_mod(&mods_root, "Aqua Football", &[("ball.dds", b"aqua ball!!")]);
        lay_mod(&mods_root, "Xbox Controls", &[("btn_a.png", b"xbox prompt")]);
        lay_applied(
            &mods_root,
            "Aqua Football",
            &["User/Load/Textures/GN7E69/ball.dds"],
            &["User/Load/Textures/GN7E69/ball.dds"],
        );
        // No backup: an add-only mod displaced nothing.
        lay_applied(
            &mods_root,
            "Xbox Controls",
            &["User/Load/Textures/GN7E69/btn_a.png"],
            &[],
        );

        // The backup holds the ORIGINAL ball, which is what pristine means.
        let backup = mods_root
            .join("!BACKUP")
            .join("User")
            .join("Load")
            .join("Textures")
            .join("GN7E69");
        std::fs::create_dir_all(&backup).unwrap();
        write(&backup.join("ball.dds.Aqua Football"), b"the real ball");

        mods_root
    }

    #[test]
    fn a_pristine_copy_restores_what_was_displaced_and_drops_what_was_added() {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("install");
        let mods_root = lay_live_install(&install);
        let to = dir.path().join("pristine");

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).unwrap();
        let build = GameId::parse("GN7E69").unwrap();
        let plan = library.pristine_plan(&install, &build, &to).unwrap();

        assert_eq!(plan.blocked, None, "{plan:?}");
        assert_eq!(plan.restore.len(), 1, "the displaced file: {plan:?}");
        assert_eq!(plan.remove.len(), 1, "the added file: {plan:?}");

        let report = plan.execute().unwrap();
        assert!(report.is_exact(), "missing: {:?}", report.missing);
        assert_eq!(report.restored, 1);
        assert_eq!(report.removed, 1);
        assert!(
            report.restored_absent.is_empty(),
            "the displaced path was in the tree, so nothing was restored into a gap"
        );

        // The displaced file is the ORIGINAL, not the mod's version. Getting
        // this wrong is the whole failure: the mod would be baked into the base
        // and could never be turned off.
        assert_eq!(
            std::fs::read(to.join("ball.dds")).unwrap(),
            b"the real ball",
            "the backup is the original and must win"
        );
        // The added file is gone, because it was never part of the base.
        assert!(
            !to.join("btn_a.png").exists(),
            "an add-only mod's file must not survive into the pristine tree"
        );
        // And everything untouched survives byte for byte.
        assert_eq!(std::fs::read(to.join("crowd.dds")).unwrap(), b"base crowd");
    }

    #[test]
    fn the_live_install_is_never_written_to() {
        // What makes this safe to run against someone's only copy. A failure
        // part way costs the copy time and nothing else.
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("install");
        let mods_root = lay_live_install(&install);
        let to = dir.path().join("pristine");

        let before = tree_snapshot(&install);
        let library = JsgmeLibrary::scan(&mods_root, &recognized()).unwrap();
        let build = GameId::parse("GN7E69").unwrap();
        library
            .pristine_plan(&install, &build, &to)
            .unwrap()
            .execute()
            .unwrap();

        assert_eq!(
            before,
            tree_snapshot(&install),
            "the install changed, which it must never do"
        );
    }

    #[test]
    fn a_destination_that_already_holds_a_tree_is_refused() {
        // A pristine tree is what every composition is layered over. Replacing
        // one silently would change what every applied mod sits on top of, and
        // there is no way to tell a stale tree from a good one.
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("install");
        let mods_root = lay_live_install(&install);
        let to = dir.path().join("pristine");
        std::fs::create_dir_all(&to).unwrap();
        write(&to.join("something.dds"), b"already here");

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).unwrap();
        let build = GameId::parse("GN7E69").unwrap();
        let plan = library.pristine_plan(&install, &build, &to).unwrap();

        let why = plan
            .blocked
            .clone()
            .expect("a non-empty destination must be refused");
        assert!(why.contains("already holds files"), "{why}");
        assert!(
            plan.execute().is_err(),
            "and executing a blocked plan must fail rather than proceed"
        );
        assert_eq!(
            std::fs::read(to.join("something.dds")).unwrap(),
            b"already here",
            "nothing may be touched"
        );
    }

    #[test]
    fn an_unreadable_install_log_blocks_the_whole_thing() {
        // The log is the only record of what a mod wrote. Without it there is no
        // way to know which files to reverse, and a tree that MIGHT still hold
        // a mod is not a pristine tree. Guessing here would produce a base that
        // silently contains someone's football.
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("install");
        let mods_root = lay_live_install(&install);
        let to = dir.path().join("pristine");

        // A log whose contents cannot be read. Bytes that are not UTF-8, which
        // is the realistic failure: the file is there, JSGME wrote it, and the
        // paths inside are unrecoverable.
        //
        // A DIRECTORY named like a log does not do it, which is worth recording
        // because it is what this test tried first: the scanner walks files, so
        // a directory never becomes a log at all and nothing is reported as
        // unreadable.
        let logs = mods_root.join("!INSTLOGS");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("Broken Mod install.log"), [0xff, 0xfe, 0x00]).unwrap();

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).unwrap();
        let build = GameId::parse("GN7E69").unwrap();
        let plan = library.pristine_plan(&install, &build, &to).unwrap();

        let why = plan
            .blocked
            .expect("an unreadable log must block the reconstruction");
        assert!(
            why.contains("Broken Mod"),
            "the refusal must name the mod: {why}"
        );
        assert!(!to.exists(), "and nothing may be written");
    }

    #[test]
    fn a_library_with_nothing_applied_is_copied_as_it_is() {
        // The normal case for a fresh install, and it must not be mistaken for
        // an error: nothing to reverse means the tree already IS the base.
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("install");
        let mods_root = lay_mods_root(&install);
        let textures = install
            .join("User")
            .join("Load")
            .join("Textures")
            .join("GN7E69");
        std::fs::create_dir_all(&textures).unwrap();
        write(&textures.join("crowd.dds"), b"base crowd");
        let to = dir.path().join("pristine");

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).unwrap();
        let build = GameId::parse("GN7E69").unwrap();
        let plan = library.pristine_plan(&install, &build, &to).unwrap();
        assert_eq!(plan.blocked, None);
        assert_eq!(plan.reversing(), 0);

        let report = plan.execute().unwrap();
        assert_eq!(report.files_copied, 1);
        assert!(report.is_exact());
        assert_eq!(std::fs::read(to.join("crowd.dds")).unwrap(), b"base crowd");
    }

    /// Every path under a root with its bytes, for proving nothing changed.
    fn tree_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(bytes) = std::fs::read(&path) {
                    out.push((path, bytes));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn an_original_whose_path_the_live_tree_lost_is_still_restored() {
        // The real install's unexplained file count. Nine of eleven backed-up
        // originals went to paths the live tree no longer had: the mod wrote
        // them, something later deleted them, and the backup still holds the
        // base game's version. The original belongs in a pristine tree whether
        // or not the live tree kept it, so it is restored AND counted
        // separately, because it means the pristine tree is not a subset of the
        // live one.
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("install");
        let mods_root = lay_live_install(&install);
        let to = dir.path().join("pristine");

        // Someone cleaned the live tree after the mod was applied.
        std::fs::remove_file(
            install
                .join("User")
                .join("Load")
                .join("Textures")
                .join("GN7E69")
                .join("ball.dds"),
        )
        .unwrap();

        let library = JsgmeLibrary::scan(&mods_root, &recognized()).unwrap();
        let build = GameId::parse("GN7E69").unwrap();
        let report = library
            .pristine_plan(&install, &build, &to)
            .unwrap()
            .execute()
            .unwrap();

        assert_eq!(report.restored, 1);
        assert_eq!(
            report.restored_absent,
            vec!["ball.dds".to_string()],
            "restoring into a gap has to be said, not folded into the total"
        );
        assert_eq!(
            std::fs::read(to.join("ball.dds")).unwrap(),
            b"the real ball",
            "and the base game's file must be there regardless"
        );
    }

}
