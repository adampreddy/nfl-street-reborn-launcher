//! Composing the texture tree instead of overwriting it.
//!
//! # What this replaces
//!
//! The shipped 2.1 product manages mods with JSGME. JSGME applies a mod by
//! **moving** the original file into a `!BACKUP` folder and copying the mod file
//! into its place, so `!BACKUP` becomes the only copy of the original that
//! exists anywhere. The 2.1 updater then runs `DelTree` over the whole `MODS`
//! folder, `!BACKUP` included. Anyone who updates with mods applied is left
//! with overlaid files and no way back, which is why [`crate::migration`]
//! treats "mods applied with no restorable backup" as a hard blocker rather
//! than a warning: at that point nothing on the machine knows what the original
//! texture was.
//!
//! # The model
//!
//! ```text
//!   library/textures/<gameid>/                 PRISTINE.  Never written to.
//!   mods/packages/<id>/<version>/              validated package payloads
//!   <user folder>/Load/Textures/<gameid>/      COMPOSED.  What Dolphin loads, one per user folder.
//!   mods/activation.json                       the ordered list the composed tree is built from
//! ```
//!
//! The composed tree is every pristine file, hardlinked, with active package
//! files placed over the ones they override. **Un-applying a mod is rebuilding
//! the composed tree from the activation list, not restoring anything.** There
//! is no reverse map and no backup folder because nothing is ever displaced, so
//! there is nothing whose loss could strand the user.
//!
//! The composed tree lives INSIDE the user folder, as a real directory, so
//! no junction and no absolute path joins the two (a junction stored the
//! data root's name by path, and every content update had to trust it). A
//! content update swaps the pristine folder whole, which leaves the composed
//! hardlinks on the old bytes; the record carries the pristine tree's
//! identity so the swap rebuilds it (see `ComposedRecord`).
//!
//! # Four rules, and the failure each one prevents
//!
//! 1. **The pristine tree is read only.** [`PristineTree`] has no method that
//!    writes, and [`Composer::plan`] refuses when the composed root is inside
//!    the pristine root or the other way round. That refusal is the whole
//!    safety property: a composed root of `library/textures/GN7E69` would make
//!    composition into exactly the destructive overwrite this module exists to
//!    replace.
//! 2. **Two mods writing one path is reported, never resolved by order.** The
//!    seven football mods in the 2.1 catalog all write the single path
//!    `tex1_128x128_m_ffc5151a141db330_14.dds`, and `PS Controls` and
//!    `Xbox Controls` have identical 19-file name sets. They are mutually
//!    exclusive by construction. Last-one-wins would silently pick for the user
//!    and the picked one would change if the activation order changed, so an
//!    overlapping path refuses the plan and names both packages.
//! 3. **The launcher only ever deletes a directory it composed itself.** A
//!    composed tree carries `composed.json` at its root. Rebuilding removes the
//!    old tree, and that removal is permitted only when the marker is there. A
//!    non-empty directory with no marker is refused, because the one thing worse
//!    than JSGME losing the originals would be this module deleting a folder the
//!    user pointed it at by mistake.
//! 4. **A composed tree is verified against the plan that built it.** The
//!    marker records the activation, the entry count and the SHA-256 of every
//!    overlaid file, so "Dolphin is loading a mod the user turned off" is a
//!    detectable state. Every discrepancy has the same remedy, a rebuild, which
//!    is the payoff of owning nothing irreplaceable.
//!
//! # The cost, stated honestly
//!
//! A hardlink cannot cross a filesystem, so the pristine and composed trees
//! must share a volume, and exFAT and FAT32 do not support hardlinks at all.
//! On those the fallback is a full copy of the whole texture tree, which for the
//! measured `GN7E69` component is 2,077,362,716 bytes. That is the default
//! format of a Steam Deck SD card, so it is the common case on a handheld and
//! not an edge case. [`CompositionPlan::method`] says which cost the plan
//! expects and [`ComposeReport`] says which one was actually paid.

use crate::controller::ControlsProfile;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::library::hash_file;
use crate::manifest::Sha256Hex;
use crate::platform::{DataDir, Error, Platform, Result};
use crate::supervisor::Supervisor;

/// The only mod package schema this module understands.
pub const MOD_SCHEMA: &str = "nsr.mod.v1";
/// The magic in a package manifest, from the ruled `.nsmod` format.
pub const MOD_MAGIC: &str = "NSMD";
/// Schema of `mods/activation.json`.
pub const ACTIVATION_SCHEMA: &str = "nsr.activation.v1";
/// Schema of the marker at the root of a composed tree.
pub const COMPOSED_SCHEMA: &str = "nsr.composed.v1";
/// The manifest filename inside an extracted package.
pub const PACKAGE_MANIFEST: &str = "package.json";
/// The payload subdirectory inside an extracted package.
pub const PAYLOAD_SUBDIR: &str = "files";
/// The marker at the root of a composed tree.
///
/// It is both the record used for verification and the launcher's proof that it
/// created this directory, which is what makes removing the directory safe.
pub const COMPOSED_MARKER: &str = "composed.json";
/// The ordered activation list, under `mods/`.
pub const ACTIVATION_FILE: &str = "activation.json";
/// Where the installer puts content, under the install root.
pub const SHIPPED_CONTENT_DIR: &str = "content";
/// Written INSIDE a pristine tree in the data root, by whatever put it
/// there deliberately. Its absence is the signal that a tree was
/// reconstructed from a 2.1 install rather than delivered as a release.
pub const PRISTINE_MARKER: &str = "pristine.json";

/// Extensions a payload entry may have, from the ruled two-zone entry policy.
const PAYLOAD_EXTENSIONS: &[&str] = &["dds", "png"];

fn invalid(message: String) -> Error {
    Error::PlainIo(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

fn refused(message: String) -> Error {
    Error::PlainIo(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

// ---------------------------------------------------------------------------
// Names that become path segments
// ---------------------------------------------------------------------------

/// A disc identifier, which is also a directory name in both trees.
///
/// Validated rather than passed through as a string because it is joined onto
/// two roots. `GameId::parse("..")` would make the composed root the parent of
/// `Load/Textures`, and every write would land outside the tree the launcher is
/// allowed to touch.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct GameId(String);

impl GameId {
    /// Parse, rejecting anything that is not ASCII letters and digits.
    ///
    /// The three ids that exist today are `GN7E69`, `FR4N31` and `CR4E12`, all
    /// six characters. The bound here is 16 and is a path-safety limit, not a
    /// claim about the format: a longer id would still be safe, a `..` or a
    /// separator would not.
    pub fn parse(s: &str) -> Result<Self> {
        if s.is_empty() || s.len() > 16 || !s.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(invalid(format!(
                "{s:?} is not a game id. Use 1 to 16 ASCII letters and digits, \
                 for example GN7E69. It becomes a directory name under both \
                 library/textures and the user folder's Load/Textures, so anything else could point \
                 composition outside those trees"
            )));
        }
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<GameId> for String {
    fn from(v: GameId) -> String {
        v.0
    }
}

impl TryFrom<String> for GameId {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        Self::parse(&s)
    }
}

/// A package identifier such as `reborn.ball.purple`.
///
/// Also a directory name, under `mods/packages/<id>/<version>/`, so it carries
/// the same validation for the same reason.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct ModId(String);

impl ModId {
    pub fn parse(s: &str) -> Result<Self> {
        let shape_ok = !s.is_empty()
            && s.len() <= 100
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
            && !s.starts_with(['.', '-'])
            && !s.ends_with(['.', '-'])
            && !s.contains("..");
        if !shape_ok {
            return Err(invalid(format!(
                "{s:?} is not a package id. Use lowercase letters, digits, dots \
                 and hyphens, for example reborn.ball.purple. It becomes a \
                 directory name under mods/packages, so a separator or a `..` \
                 would let a package payload be read from outside the library"
            )));
        }
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ModId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<ModId> for String {
    fn from(v: ModId) -> String {
        v.0
    }
}

impl TryFrom<String> for ModId {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        Self::parse(&s)
    }
}

/// A package version, which is the second directory segment of a payload path.
///
/// Not parsed as semantic versioning here. This module never compares two
/// versions, it only joins one onto a path and records it, so validating the
/// shape is the honest amount of work: claiming to understand precedence would
/// invite callers to rely on an ordering that is not implemented.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct PackageVersion(String);

impl PackageVersion {
    pub fn parse(s: &str) -> Result<Self> {
        let shape_ok = !s.is_empty()
            && s.len() <= 40
            && s.bytes().all(|b| {
                b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'+' || b == b'_'
            })
            && !s.starts_with('.')
            && !s.contains("..");
        if !shape_ok {
            return Err(invalid(format!(
                "{s:?} is not a package version. Use letters, digits, dots, \
                 hyphens, plus and underscore, for example 1.0.0. It becomes a \
                 directory name under mods/packages/<id>/"
            )));
        }
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PackageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<PackageVersion> for String {
    fn from(v: PackageVersion) -> String {
        v.0
    }
}

impl TryFrom<String> for PackageVersion {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        Self::parse(&s)
    }
}

/// One file inside a package payload, relative to the package's `files/`
/// directory, so it always begins with a game id: `GN7E69/tex1_....dds`.
///
/// This is the type that decides where a mod is allowed to write. Every
/// composed path comes from here, so the traversal defense lives in
/// [`PayloadPath::parse`] and nowhere else.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct PayloadPath {
    full: String,
    game: GameId,
    tail: String,
}

impl PayloadPath {
    /// Parse a payload path, refusing anything that could escape the composed
    /// tree or name a different file than it appears to.
    ///
    /// The trailing dot and space check is the subtle one. Windows silently
    /// strips a trailing `.` or space from a filename, so a manifest entry of
    /// `ball.dds ` would be recorded under one name and created under another.
    /// Verification would then hash a file the manifest does not describe and
    /// report a mismatch forever, which reads as corruption and is not.
    pub fn parse(s: &str) -> Result<Self> {
        let reject = |why: &str| {
            Err(invalid(format!(
                "{s:?} is not a usable payload path: {why}. A payload path is \
                 <gameid>/<name>.dds or <gameid>/<name>.png, relative to the \
                 package's files/ directory, with forward slashes"
            )))
        };

        if s.is_empty() {
            return reject("it is empty");
        }
        if s.len() > 512 {
            return reject("it is longer than 512 characters");
        }
        // A backslash separates on Windows and is an ordinary filename
        // character on Linux, so one payload path would name two different
        // destinations depending on the machine.
        if s.contains('\\') {
            return reject("it contains a backslash");
        }
        if s.contains(':') {
            return reject("it contains a colon, which can name a volume or an NTFS data stream");
        }
        if s.starts_with('/') {
            return reject("it is absolute");
        }
        if s.ends_with('/') {
            return reject("it names a directory, not a file");
        }
        if s.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return reject("it contains a control character");
        }

        let segments: Vec<&str> = s.split('/').collect();
        if segments.len() < 2 {
            return reject("it has no game id segment");
        }
        for seg in &segments {
            if seg.is_empty() {
                return reject("it has an empty path segment");
            }
            if *seg == "." || *seg == ".." {
                return reject("it contains a relative segment");
            }
            if seg.ends_with('.') || seg.ends_with(' ') {
                return reject("a segment ends in a dot or a space, which Windows silently strips");
            }
        }

        let game = GameId::parse(segments[0])?;
        let tail = segments[1..].join("/");

        let extension = tail
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        // Version 1 of the package format carries texture overlays and
        // nothing else, so this is both the scope limit of the format and
        // the reason a package cannot place an executable in the tree
        // Dolphin reads.
        if !PAYLOAD_EXTENSIONS.contains(&extension.as_str()) {
            return reject("its extension is not .dds or .png");
        }

        Ok(Self {
            full: s.to_string(),
            game,
            tail,
        })
    }

    /// The path relative to the package's `files/` directory, game id included.
    pub fn as_str(&self) -> &str {
        &self.full
    }

    /// Which build this file is for.
    pub fn game_id(&self) -> &GameId {
        &self.game
    }

    /// The path relative to the composed tree root, game id stripped.
    ///
    /// This is the value conflict detection compares, because two packages
    /// collide when they write the same place in the composed tree, not when
    /// their payload paths happen to look alike.
    pub fn tail(&self) -> &str {
        &self.tail
    }
}

impl std::fmt::Display for PayloadPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.full)
    }
}

impl From<PayloadPath> for String {
    fn from(v: PayloadPath) -> String {
        v.full
    }
}

impl TryFrom<String> for PayloadPath {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        Self::parse(&s)
    }
}

// ---------------------------------------------------------------------------
// Packages
// ---------------------------------------------------------------------------

/// One file a package contributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModFile {
    pub path: PayloadPath,
    pub sha256: Sha256Hex,
    /// The byte length, not the size on disk.
    ///
    /// A 4 KiB-allocated size taken from a file listing is a different number,
    /// and recording one here makes every future verification of a correct
    /// install fail.
    pub bytes: u64,
}

/// A mod package, as far as composition is concerned.
///
/// This is the composition view of `nsr.mod.v1` and deliberately not the
/// catalog view. Unknown fields deserialize and are ignored, so `author`,
/// `category`, `description` and `changelog` can live in the same file without
/// this module claiming to understand them. None of the eleven migrated 2.1
/// packages has a known author, and a type that required the field would
/// invite somebody to fill it in with a guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModPackage {
    pub schema: String,
    pub magic: String,
    pub id: ModId,
    /// What the user sees, for example "Purple Football".
    pub name: String,
    pub version: PackageVersion,
    /// Which builds this package has payload for.
    pub targets: Vec<GameId>,
    /// A conceptual exclusivity group such as `ball` or `prompts`.
    ///
    /// Absent means the package declares no conceptual conflict. This drives a
    /// pick-one interface only. It cannot create a hard conflict and it cannot
    /// excuse one, because overlapping paths are computed from the file lists
    /// and no manifest field can override what the data says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict_group: Option<String>,
    pub files: Vec<ModFile>,
}

impl ModPackage {
    /// Read and validate `package.json` from an extracted package directory.
    pub fn load(package_dir: &Path) -> Result<Self> {
        let path = package_dir.join(PACKAGE_MANIFEST);
        let raw = std::fs::read(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        let package: ModPackage = serde_json::from_slice(&raw).map_err(|e| {
            invalid(format!(
                "{} is not a readable {MOD_SCHEMA} manifest: {e}",
                path.display()
            ))
        })?;
        package.validate()?;
        Ok(package)
    }

    /// Refuse a package this module cannot compose without guessing.
    ///
    /// Duplicate paths inside one package are the interesting case: the package
    /// would be describing two different contents for one destination, and
    /// picking either is a coin flip presented as a result.
    pub fn validate(&self) -> Result<()> {
        if self.schema != MOD_SCHEMA {
            return Err(invalid(format!(
                "package {} declares schema {:?}, but this launcher composes \
                 {MOD_SCHEMA} only. Update the launcher, or re-export the package",
                self.id, self.schema
            )));
        }
        if self.magic != MOD_MAGIC {
            return Err(invalid(format!(
                "package {} declares magic {:?} rather than {MOD_MAGIC:?}, so it \
                 is not an .nsmod package",
                self.id, self.magic
            )));
        }
        if self.targets.is_empty() {
            return Err(invalid(format!(
                "package {} names no target builds, so there is no tree it could \
                 be composed into",
                self.id
            )));
        }
        if self.files.is_empty() {
            return Err(invalid(format!(
                "package {} lists no files. An empty package would activate, \
                 change nothing, and look like a broken mod",
                self.id
            )));
        }
        if let Some(group) = &self.conflict_group {
            if group.trim().is_empty() {
                return Err(invalid(format!(
                    "package {} declares an empty conflict_group. Omit the field \
                     instead: an empty group would put every such package in one \
                     exclusivity set",
                    self.id
                )));
            }
        }
        let mut seen = BTreeSet::new();
        for file in &self.files {
            if !self.targets.contains(file.path.game_id()) {
                return Err(invalid(format!(
                    "package {} carries {} but does not list {} in targets, so \
                     the payload and the manifest disagree about which builds it \
                     is for",
                    self.id,
                    file.path,
                    file.path.game_id()
                )));
            }
            if !seen.insert(file.path.as_str()) {
                return Err(invalid(format!(
                    "package {} lists {} twice with no way to tell which content \
                     is meant",
                    self.id, file.path
                )));
            }
            if file.path.tail() == COMPOSED_MARKER {
                return Err(invalid(format!(
                    "package {} writes {COMPOSED_MARKER} at the root of the \
                     composed tree, which is the launcher's own record of what it \
                     composed. Rename the file",
                    self.id
                )));
            }
        }
        Ok(())
    }

    /// Whether this package has payload for a build.
    pub fn targets_game(&self, game: &GameId) -> bool {
        self.targets.contains(game)
    }

    /// The files this package contributes to one build.
    pub fn files_for(&self, game: &GameId) -> Vec<&ModFile> {
        self.files
            .iter()
            .filter(|f| f.path.game_id() == game)
            .collect()
    }

    /// Which of this package's payloads a tree for `game` composes: its own when it has one, else `fallback`'s,
    /// else none.
    pub fn payload_game(&self, game: &GameId, fallback: Option<&GameId>) -> Option<GameId> {
        if self.targets_game(game) {
            return Some(game.clone());
        }
        fallback.filter(|f| self.targets_game(f)).cloned()
    }

    /// Where the extracted payload for one file lives.
    pub fn payload_file(&self, packages_root: &Path, file: &ModFile) -> PathBuf {
        let mut path = packages_root
            .join(self.id.as_str())
            .join(self.version.as_str())
            .join(PAYLOAD_SUBDIR);
        for segment in file.path.as_str().split('/') {
            path = path.join(segment);
        }
        path
    }

    /// What this package writes into one composed tree, relative to its root.
    pub fn composed_paths(&self, game: &GameId) -> BTreeSet<&str> {
        self.files_for(game)
            .into_iter()
            .map(|f| f.path.tail())
            .collect()
    }

    /// This package as an entry in the activation list.
    pub fn as_active(&self) -> ActivePackage {
        ActivePackage {
            id: self.id.clone(),
            version: self.version.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Conflicts
// ---------------------------------------------------------------------------

/// Why two packages cannot both be active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    /// Both packages write these paths in the composed tree.
    ///
    /// Computed from the file lists, and authoritative: no manifest field can
    /// override it, because the overlap is a fact about the data.
    SamePath { paths: Vec<String> },
    /// Both packages declare the same conceptual group and no path overlaps.
    ///
    /// Advisory. It drives a pick-one interface and does not stop composition,
    /// because nothing about the composed tree is ambiguous.
    DeclaredGroup { group: String },
}

/// Two packages that cannot both be active for one build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub game: GameId,
    pub first: ModId,
    pub second: ModId,
    pub kind: ConflictKind,
}

impl Conflict {
    /// Whether this stops composition.
    pub fn is_hard(&self) -> bool {
        matches!(self.kind, ConflictKind::SamePath { .. })
    }

    /// A sentence naming both packages and what to do.
    pub fn message(&self) -> String {
        match &self.kind {
            ConflictKind::SamePath { paths } => {
                let sample = paths.first().map(String::as_str).unwrap_or("");
                format!(
                    "{} and {} both write {} file(s) in {}, starting with {}. \
                     Turn one of them off: composing both would pick one by \
                     activation order and the choice would silently change if \
                     that order changed.",
                    self.first,
                    self.second,
                    paths.len(),
                    self.game,
                    sample
                )
            }
            ConflictKind::DeclaredGroup { group } => format!(
                "{} and {} are both in the {:?} group. They do not overwrite \
                 each other, so both can be composed, but they are meant to be \
                 used one at a time.",
                self.first, self.second, group
            ),
        }
    }
}

/// Every conflict among a set of packages for one build.
///
/// Pairwise, and every overlapping path is listed rather than summarized,
/// because the whole point is that the user is told which files are contested
/// instead of having the launcher choose.
///
/// `fallback` is the build whose payload a package without one of its own
/// contributes (see [`payload_fallback_for`]); each package is compared by the
/// tails of the payload it actually composes.
pub fn detect_conflicts(packages: &[ModPackage], game: &GameId, fallback: Option<&GameId>) -> Vec<Conflict> {
    let mut out = Vec::new();
    let relevant: Vec<(&ModPackage, GameId)> = packages
        .iter()
        .filter_map(|p| p.payload_game(game, fallback).map(|g| (p, g)))
        .collect();

    for (i, (a, a_payload)) in relevant.iter().enumerate() {
        for (b, b_payload) in relevant.iter().skip(i + 1) {
            let (pa, pb) = (a.composed_paths(a_payload), b.composed_paths(b_payload));
            let overlap: Vec<String> = pa.intersection(&pb).map(|s| s.to_string()).collect();

            if !overlap.is_empty() {
                out.push(Conflict {
                    game: game.clone(),
                    first: a.id.clone(),
                    second: b.id.clone(),
                    kind: ConflictKind::SamePath { paths: overlap },
                });
                // The computed conflict is the authoritative one, so a declared
                // group adds nothing here: both are already refused together.
                continue;
            }

            match (&a.conflict_group, &b.conflict_group) {
                (Some(ga), Some(gb)) if ga == gb => out.push(Conflict {
                    game: game.clone(),
                    first: a.id.clone(),
                    second: b.id.clone(),
                    kind: ConflictKind::DeclaredGroup { group: ga.clone() },
                }),
                _ => {}
            }
        }
    }
    out
}

/// A set of packages that cannot be on together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExclusiveGroup {
    /// Every package in the set, sorted, so the order is stable across runs and
    /// an interface does not reshuffle its own list.
    pub members: Vec<ModId>,
    /// A path more than one of them writes, as evidence.
    ///
    /// Carried so an interface can say WHY these are exclusive instead of
    /// asserting it. Absent when the group comes only from a declared group,
    /// where there is no overlapping path to point at.
    pub example_path: Option<String>,
    /// True when the members overlap on disk, false when they only declare the
    /// same group.
    ///
    /// The difference matters: an overlap is refused by the composer, while a
    /// declared group is advice and composes fine. An interface that treated
    /// them the same would either forbid something legal or allow something
    /// that will be refused later.
    pub computed: bool,
}

/// Group the packages that cannot be on at the same time.
///
/// Derived from the file lists, not from a curated table. On the real library
/// this returns two groups: seven recoloured footballs that all write one
/// texture path, and the two prompt packs that write the same nineteen. No
/// metadata was added to get either, and there is nothing to maintain.
///
/// The jersey mods are correctly NOT grouped. Patriots Throwback and Saints
/// Alternate touch different files, so they compose together fine, and sweeping
/// them in would forbid something legal.
///
/// This is what a pick-one interface needs. Showing seven footballs as
/// independently switchable is telling the player something untrue: turning on
/// two is refused, and before it was refused JSGME simply kept whichever landed
/// last without saying so.
///
/// Transitive on purpose. If A overlaps B and B overlaps C, all three land in
/// one group even when A and C share no path, because a pick-one control has to
/// offer a single choice rather than a puzzle.
pub fn exclusive_groups(packages: &[ModPackage], game: &GameId) -> Vec<ExclusiveGroup> {
    let conflicts = detect_conflicts(packages, game, payload_fallback_for(game).as_ref());
    if conflicts.is_empty() {
        return Vec::new();
    }

    // Union find over the conflict pairs, keyed by id string.
    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    fn root(parent: &mut BTreeMap<String, String>, id: &str) -> String {
        let mut cur = id.to_string();
        while let Some(next) = parent.get(&cur) {
            if next == &cur {
                break;
            }
            cur = next.clone();
        }
        cur
    }

    for c in &conflicts {
        let (a, b) = (c.first.as_str().to_string(), c.second.as_str().to_string());
        parent.entry(a.clone()).or_insert_with(|| a.clone());
        parent.entry(b.clone()).or_insert_with(|| b.clone());
        let (ra, rb) = (root(&mut parent, &a), root(&mut parent, &b));
        if ra != rb {
            parent.insert(rb, ra);
        }
    }

    // Collect members per root, and the evidence for each.
    let mut members: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
    let ids: BTreeMap<String, ModId> = packages
        .iter()
        .map(|p| (p.id.as_str().to_string(), p.id.clone()))
        .collect();
    let keys: Vec<String> = parent.keys().cloned().collect();
    for id in keys {
        let r = root(&mut parent, &id);
        if let Some(mod_id) = ids.get(&id) {
            members.entry(r).or_default().push(mod_id.clone());
        }
    }

    let mut out: Vec<ExclusiveGroup> = Vec::new();
    for (r, mut group) in members {
        group.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        // The first computed conflict inside this group, for the evidence. A
        // declared-group conflict has no path to show, which is why the field is
        // optional rather than filled with something invented.
        let in_group = |id: &ModId| group.iter().any(|m| m == id);
        let mut example_path = None;
        let mut computed = false;
        for c in &conflicts {
            if !in_group(&c.first) || !in_group(&c.second) {
                continue;
            }
            if let ConflictKind::SamePath { paths } = &c.kind {
                computed = true;
                example_path = paths.first().cloned();
                break;
            }
        }
        let _ = r;
        out.push(ExclusiveGroup {
            members: group,
            example_path,
            computed,
        });
    }

    // Largest first: the group a player is most likely to be choosing from.
    out.sort_by(|a, b| {
        b.members
            .len()
            .cmp(&a.members.len())
            .then_with(|| a.members[0].as_str().cmp(b.members[0].as_str()))
    });
    out
}

// ---------------------------------------------------------------------------
// Making room in the user folder
// ---------------------------------------------------------------------------

/// Make room for the composed tree in the user folder.
///
/// A link (the junction earlier builds made) is removed: removing a link
/// deletes no content. A real folder that is not ours and not empty is
/// moved aside as `<game>.before-reborn.<ms>`, never deleted. Our own
/// composed tree, or an empty folder, is left for verify and apply.
pub fn prepare_texture_folder(at: &Path, now_ms: u64) -> Result<Option<PathBuf>> {
    let Ok(meta) = std::fs::symlink_metadata(at) else { return Ok(None) };
    if meta.file_type().is_symlink() {
        std::fs::remove_dir(at)
            .or_else(|_| std::fs::remove_file(at))
            .map_err(|source| Error::Io { path: at.to_path_buf(), source })?;
        return Ok(None);
    }
    if !meta.is_dir() {
        return Err(refused(format!(
            "{} is a file, not a folder; move it aside so the game's texture folder can be made",
            at.display()
        )));
    }
    if at.join(COMPOSED_MARKER).is_file() {
        return Ok(None);
    }
    let empty = std::fs::read_dir(at)
        .map_err(|source| Error::Io { path: at.to_path_buf(), source })?
        .next()
        .is_none();
    if empty {
        return Ok(None);
    }
    let name = at.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let aside = at.with_file_name(format!("{name}.before-reborn.{now_ms}"));
    std::fs::rename(at, &aside).map_err(|source| Error::Io { path: aside.clone(), source })?;
    Ok(Some(aside))
}

/// The composed tree before 2026-09-12 lived at `mods/active/<game>` in the
/// data root. One that carries our marker is removed; anything else there
/// is left alone. Best effort, logged, never fatal.
pub fn retire_old_composed_tree(old: &Path) {
    if old.join(COMPOSED_MARKER).is_file() {
        if let Err(e) = std::fs::remove_dir_all(old) {
            tracing::warn!("could not remove the old composed tree {}: {e}", old.display());
        }
    }
    if let Some(active) = old.parent() {
        if std::fs::read_dir(active).map(|mut d| d.next().is_none()).unwrap_or(false) {
            let _ = std::fs::remove_dir(active);
        }
    }
}

// ---------------------------------------------------------------------------
// The controls profile, as an activation
// ---------------------------------------------------------------------------

/// What reconciling the activation with a controls profile changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlsApplied {
    pub profile: ControlsProfile,
    /// The package now on, or none for GameCube.
    pub turned_on: Option<ModId>,
    /// Prompt packages turned off, which is how the profiles stay exclusive.
    pub turned_off: Vec<ModId>,
    /// True when the activation already said this, so nothing was written.
    ///
    /// The common case on every launch after the first. Recomposing eight
    /// thousand entries to change nothing is work a player waits through.
    pub already_correct: bool,
    /// Present when the profile needs a package that is not installed.
    ///
    /// Reported rather than raised. The profile is a recorded decision and
    /// still stands; what is missing is the art, and the game will draw its own
    /// until the package arrives. Failing the launch over button art would be a
    /// worse outcome than the wrong buttons.
    pub missing_package: Option<String>,
}

/// Every package a controls profile could own, whether or not it is installed.
///
/// Used to turn the others OFF. The profiles are mutually exclusive because the
/// two packs write the same nineteen texture paths, so leaving an old one
/// activated would make the composition refuse rather than quietly pick one.
fn prompt_package_ids() -> Vec<&'static str> {
    ControlsProfile::ALL
        .iter()
        .filter_map(|p| p.package_id())
        .collect()
}

/// Make the activation say what the controls profile says.
///
/// Called on every launch, next to the configuration regeneration, which is
/// why it exists at all: the profile is chosen once in first run and has to
/// keep being true afterwards. Changing it later is then a matter of recording
/// a new profile, with no first run to re-walk and nothing to re-apply by hand.
///
/// Writes the activation file only when something actually differs.
/// Takes the activation PATH rather than a Platform: reconciling a list is
/// this function's job, and finding the list is the caller's. That also makes
/// it testable without a whole platform standing behind it.
pub fn apply_controls_profile(
    activation_file: &Path,
    profile: ControlsProfile,
    installed: &[ModPackage],
) -> Result<ControlsApplied> {
    let mut activation = Activation::load(activation_file)?;

    let wanted = profile.package_id();
    let mut turned_off = Vec::new();
    let mut turned_on = None;
    let mut missing_package = None;
    let mut changed = false;

    // Off first, then on. The other order would briefly have two packs
    // activated, and if anything read the activation in between it would see a
    // pair that cannot compose.
    for id in prompt_package_ids() {
        if Some(id) == wanted {
            continue;
        }
        let parsed = ModId::parse(id)?;
        if activation.deactivate(&parsed) {
            turned_off.push(parsed);
            changed = true;
        }
    }

    if let Some(id) = wanted {
        let parsed = ModId::parse(id)?;
        match installed.iter().find(|p| p.id == parsed) {
            Some(package) => {
                if !activation.is_active(&parsed)
                    || activation
                        .active
                        .iter()
                        .any(|a| a.id == parsed && a.version != package.version)
                {
                    activation.activate(parsed.clone(), package.version.clone());
                    changed = true;
                }
                turned_on = Some(parsed);
            }
            None => {
                missing_package = Some(format!(
                    "{} is not installed, so the {} button art cannot be applied. \
                     The choice is still recorded and the game will draw its own \
                     art until the package is there.",
                    id,
                    profile.label()
                ));
            }
        }
    }

    if changed {
        activation.save(activation_file)?;
    }

    Ok(ControlsApplied {
        profile,
        turned_on,
        turned_off,
        already_correct: !changed,
        missing_package,
    })
}

// ---------------------------------------------------------------------------
// Activation
// ---------------------------------------------------------------------------

/// One entry in the activation list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivePackage {
    pub id: ModId,
    pub version: PackageVersion,
}

/// `mods/activation.json`: the ordered list a composed tree is built from.
///
/// This file, plus the pristine tree, is the entire state of the mod system.
/// Losing it costs the user their selection and nothing else, which is the
/// difference between this and `!BACKUP`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activation {
    pub schema: String,
    pub active: Vec<ActivePackage>,
}

impl Default for Activation {
    fn default() -> Self {
        Self {
            schema: ACTIVATION_SCHEMA.to_string(),
            active: Vec::new(),
        }
    }
}

impl Activation {
    /// Read the activation list.
    ///
    /// A missing file is an empty list, because no mods active is a real and
    /// common state and is exactly what a fresh install looks like.
    ///
    /// An unreadable or unknown-schema file is an error rather than an empty
    /// list. Treating it as empty would rebuild the composed tree with every
    /// mod off and report success, which is a silent wrong result the user
    /// would read as "the launcher turned my mods off".
    pub fn load(path: &Path) -> Result<Self> {
        let raw = match std::fs::read(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(Error::Io {
                    path: path.to_path_buf(),
                    source,
                })
            }
        };
        let parsed: Activation = serde_json::from_slice(&raw).map_err(|e| {
            invalid(format!(
                "{} is not a readable activation list: {e}. Fix or delete the \
                 file: deleting it means no mods active, which is safe, and the \
                 launcher will not guess which mods you had on",
                path.display()
            ))
        })?;
        if parsed.schema != ACTIVATION_SCHEMA {
            return Err(invalid(format!(
                "{} declares schema {:?} but this launcher reads \
                 {ACTIVATION_SCHEMA}. Refusing to rebuild the composed tree \
                 from a list it may be misreading",
                path.display(),
                parsed.schema
            )));
        }
        Ok(parsed)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| invalid(format!("could not encode the activation list: {e}")))?;
        std::fs::write(path, bytes).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Turn a package on, or move it to a different version.
    ///
    /// One id appears at most once. Two versions of one package active at the
    /// same time would conflict with themselves on every path.
    pub fn activate(&mut self, id: ModId, version: PackageVersion) {
        match self.active.iter_mut().find(|a| a.id == id) {
            Some(entry) => entry.version = version,
            None => self.active.push(ActivePackage { id, version }),
        }
    }

    /// Turn a package off. Returns whether it was on.
    pub fn deactivate(&mut self, id: &ModId) -> bool {
        let before = self.active.len();
        self.active.retain(|a| &a.id != id);
        before != self.active.len()
    }

    pub fn is_active(&self, id: &ModId) -> bool {
        self.active.iter().any(|a| &a.id == id)
    }
}

// ---------------------------------------------------------------------------
// The pristine tree
// ---------------------------------------------------------------------------

/// One file in the pristine tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PristineEntry {
    /// Path relative to the pristine root, forward slashes.
    pub relative: String,
    pub bytes: u64,
    /// Last write time, from the metadata the walk already reads. `None`
    /// where the filesystem does not report one.
    pub modified: Option<SystemTime>,
}

/// What is in the pristine tree.
#[derive(Debug, Clone, Default)]
pub struct PristineListing {
    /// Regular files, sorted by relative path.
    pub files: Vec<PristineEntry>,
    /// Entries that are neither a regular file nor a directory, and entries
    /// whose name is not valid UTF-8.
    ///
    /// Reported rather than skipped. A symlinked directory would make the walk
    /// loop forever if followed, and skipping one silently would drop a texture
    /// out of the composed tree with nothing said about it.
    pub unsupported: Vec<PathBuf>,
}

/// The tree that is never written to.
///
/// There is no method on this type that writes, and that is the entire point.
/// Composition reads through this handle, so the compiler is the thing keeping
/// the pristine tree pristine rather than a rule somebody has to remember.
#[derive(Debug, Clone)]
pub struct PristineTree {
    root: PathBuf,
}

impl PristineTree {
    /// Open a pristine tree.
    ///
    /// Fails when the directory is absent, which means the textures component
    /// is not installed. The caller's move is to install or repair it through
    /// [`crate::library`], not to compose over nothing.
    pub fn open(root: &Path) -> Result<Self> {
        if !root.is_dir() {
            return Err(Error::Io {
                path: root.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no pristine texture tree here. Install or verify the \
                     textures component before composing mods",
                ),
            });
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every regular file, sorted, with byte lengths.
    pub fn list(&self) -> Result<PristineListing> {
        walk(&self.root)
    }
}

/// Walk a directory tree, classifying by `symlink_metadata` so a symlinked
/// directory is never followed.
fn walk(root: &Path) -> Result<PristineListing> {
    let mut out = PristineListing::default();
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
            let raw_name = entry.file_name();
            let Some(name) = raw_name.to_str() else {
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
                out.files.push(PristineEntry {
                    relative,
                    bytes: meta.len(),
                    modified: meta.modified().ok(),
                });
            } else {
                out.unsupported.push(path);
            }
        }
    }

    out.files.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(out)
}

/// Whether two write times name the same write.
///
/// FAT32 keeps write times to two seconds and exFAT to ten milliseconds, so a
/// copy onto one can come back rounded. Anything within that is the same
/// write; a file replaced by a later update is minutes or days apart. A time
/// the filesystem did not report is no evidence either way.
fn same_write_time(a: Option<SystemTime>, b: Option<SystemTime>) -> bool {
    const SLACK: Duration = Duration::from_secs(2);
    match (a, b) {
        (Some(a), Some(b)) => {
            let apart = a.duration_since(b).or_else(|_| b.duration_since(a)).unwrap_or_default();
            apart <= SLACK
        }
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// How the composed tree will be built out of the pristine one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkMethod {
    /// A directory entry per file. Costs no data bytes.
    Hardlink,
    /// A full copy. The only option when the two trees are on different
    /// volumes, or on exFAT and FAT32 where hardlinks do not exist.
    Copy,
}

/// Where one composed file comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntrySource {
    Pristine {
        path: PathBuf,
        /// The source's last write time when the plan was made. A composed
        /// link shares it; a pristine file replaced under the same name does
        /// not. See [`Discrepancy::PristineFileReplaced`].
        modified: Option<SystemTime>,
    },
    Overlay {
        id: ModId,
        path: PathBuf,
        sha256: Sha256Hex,
        /// Whether this hides a pristine file, as against adding a new one.
        ///
        /// Both are normal. The 19 prompt textures are additions the pristine
        /// component does not contain at all.
        replaces_pristine: bool,
    },
}

/// One file the composed tree will contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedEntry {
    /// Path relative to the composed root, forward slashes.
    pub relative: String,
    pub bytes: u64,
    pub source: EntrySource,
}

/// Everything composition will do, before any of it is done.
#[derive(Debug, Clone)]
pub struct CompositionPlan {
    pub game: GameId,
    pub pristine_root: PathBuf,
    /// See [`ComposeRequest::base_root`].
    pub base_root: Option<PathBuf>,
    pub composed_root: PathBuf,
    /// See [`ComposeRequest::pristine_identity`].
    pub pristine_identity: Option<String>,
    /// What the plan expects to pay. The report says what was actually paid,
    /// because a hardlink can still fail per filesystem at composition time.
    pub method: LinkMethod,
    /// Every file, sorted by relative path.
    pub entries: Vec<PlannedEntry>,
    /// The packages that actually contribute, in activation order.
    pub activation: Vec<ActivePackage>,
    /// Packages that were asked for but have no payload for this build.
    ///
    /// Reported so the caller can say so. A mod switched on that quietly does
    /// nothing is indistinguishable from a broken mod.
    pub not_targeted: Vec<ModId>,
    /// Declared-group conflicts. These do not stop composition, and they are
    /// carried on the plan so they cannot be silently dropped either.
    pub soft_conflicts: Vec<Conflict>,
}

impl CompositionPlan {
    pub fn overlay_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.source, EntrySource::Overlay { .. }))
            .count()
    }

    pub fn pristine_count(&self) -> usize {
        self.entries.len() - self.overlay_count()
    }

    /// The bytes the composed tree holds. Paid in disk only when
    /// [`CompositionPlan::method`] is [`LinkMethod::Copy`].
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.bytes).sum()
    }

    /// The overlaid files, for verification and for the record.
    pub fn overlays(&self) -> Vec<OverlayRecord> {
        self.entries
            .iter()
            .filter_map(|e| match &e.source {
                EntrySource::Overlay { id, sha256, .. } => Some(OverlayRecord {
                    relative: e.relative.clone(),
                    id: id.clone(),
                    sha256: sha256.clone(),
                }),
                EntrySource::Pristine { .. } => None,
            })
            .collect()
    }

    /// The record written into the composed tree.
    pub fn record(&self) -> ComposedRecord {
        ComposedRecord {
            game: self.game.clone(),
            pristine_root: self.pristine_root.clone(),
            base_root: self.base_root.clone(),
            pristine_identity: self.pristine_identity.clone(),
            activation: self.activation.clone(),
            total_entries: self.entries.len() as u64,
            pristine_entries: self.pristine_count() as u64,
            overlays: self.overlays(),
        }
    }
}

/// What the caller asked to compose.
#[derive(Debug, Clone)]
pub struct ComposeRequest {
    pub game: GameId,
    pub pristine_root: PathBuf,
    /// A tree laid down UNDER `pristine_root`, or `None`. The league build ships only the textures it changes and
    /// borrows the rest of the retail pack (Adam 10-16: "idk if we want to download 10 more GB for people"), so its
    /// tree is the retail pack, then the league files, then the mods. A league file replaces every base file with
    /// its texture name, wherever it sits, because Dolphin keys a pack by file name and keeps one of two at random;
    /// and it beats a mod with that name too ("league textures win"). See [`base_root_for`].
    pub base_root: Option<PathBuf>,
    pub composed_root: PathBuf,
    pub packages_root: PathBuf,
    /// In activation order. The order is recorded and displayed, and it never
    /// decides which of two packages wins a contested path, because a contested
    /// path refuses the plan instead.
    pub packages: Vec<ModPackage>,
    /// Which pristine tree this is, as the textures content pointer names it
    /// (`<version>#<sequence>`), or `None` when no update has delivered one.
    /// Recorded in the composed tree, so a swapped pristine folder with the
    /// same names and count still rebuilds; see [`pristine_identity`].
    pub pristine_identity: Option<String>,
    /// The build whose payload a package without one for `game` contributes;
    /// see [`payload_fallback_for`].
    pub payload_fallback: Option<GameId>,
}

impl ComposeRequest {
    /// The standard layout under the launcher's data root.
    pub fn in_data_root(
        platform: &Platform,
        game: GameId,
        user_dir: &Path,
        packages: Vec<ModPackage>,
    ) -> Result<Self> {
        Ok(Self {
            pristine_root: pristine_root(platform, &game)?,
            base_root: base_root_for(platform, &game)?,
            composed_root: composed_root(user_dir, &game),
            packages_root: packages_root(platform)?,
            pristine_identity: pristine_identity(platform, &game),
            payload_fallback: payload_fallback_for(&game),
            game,
            packages,
        })
    }
}

/// The tree a build's own textures are laid over: the retail pack for the league build (it ships only what it
/// changes), nothing for every other build.
pub fn base_root_for(platform: &Platform, game: &GameId) -> Result<Option<PathBuf>> {
    if game.as_str() != crate::league_edition::LEAGUE_GAME_ID {
        return Ok(None);
    }
    let retail = GameId::parse(crate::league_edition::RETAIL_GAME_ID)
        .map_err(|e| invalid(format!("the retail game id does not parse: {e}")))?;
    Ok(Some(pristine_root(platform, &retail)?))
}

/// A texture's name as Dolphin keys it: the file name, case folded.
fn texture_name(relative: &str) -> String {
    relative.rsplit('/').next().unwrap_or(relative).to_lowercase()
}

/// The build whose mod payloads a tree borrows when a package has none of its own: the league build borrows retail
/// (one shared mod list, spec 2026-10-08 v2). Every other tree composes only what was made for it.
pub fn payload_fallback_for(game: &GameId) -> Option<GameId> {
    (game.as_str() == crate::league_edition::LEAGUE_GAME_ID)
        .then(|| GameId::parse(crate::league_edition::RETAIL_GAME_ID).ok())
        .flatten()
}

/// `library/textures/<gameid>/`, the read-only tree.
/// The texture tree to compose over.
///
/// The one the installer shipped wins, unless the data root holds a tree
/// that says it was put there deliberately, which means an update delivered
/// it and it is newer.
///
/// The obvious rule, data root first, is WRONG, and Adam caught it: an
/// upgrading player's data root tree is a reconstruction of their 2.1
/// textures, made by un-applying JSGME. Preferring it would leave every
/// upgrader on the old pack forever, never seeing a single new field, and
/// nothing would say so. Merging the two is worse: a mix of 2.1 and 3.0
/// files is a configuration nobody has ever run.
///
/// So a migrated tree is a baseline for importing someone's JSGME mods, not
/// a source of textures that outranks the release.
pub fn pristine_root(platform: &Platform, game: &GameId) -> Result<PathBuf> {
    if game.as_str() == crate::league_edition::LEAGUE_GAME_ID {
        // The league files inside its component (Plan 4 layout); the retail pack under them is `base_root_for`.
        return Ok(crate::league_edition::league_textures_root(&platform.paths.data_root()?));
    }
    let in_data = platform
        .data_dir(DataDir::LibraryTextures)?
        .join(game.as_str());
    // Two things mark the data root tree as DELIVERED rather than migrated:
    // the marker file, and a content pointer saying an update installed the
    // textures component there. Until the second was consulted, an update
    // could land a newer pack in the data root and composition would keep
    // reading the one the installer shipped, so no texture update would ever
    // have reached the game.
    if in_data.join(PRISTINE_MARKER).is_file()
        || textures_delivered(&platform.paths.data_root()?, game.as_str())
    {
        return Ok(in_data);
    }
    if game.as_str() == "GN7E69" {
        let saved = crate::settings::Settings::load(&crate::settings::settings_path(platform)?)?;
        if let Some(root) = saved.migrated_texture_root { if root.is_dir() { return Ok(root); } }
    }
    if let Some(shipped) = shipped_pristine_root(platform, game) {
        return Ok(shipped);
    }
    // No release tree anywhere. A migrated one is better than nothing, and
    // on a development build it is the only one there is.
    Ok(in_data)
}

/// The texture tree the installer put beside the launcher, if it is there.
pub fn shipped_pristine_root(platform: &Platform, game: &GameId) -> Option<PathBuf> {
    let candidate = platform
        .paths
        .install_root()
        .ok()?
        .join(SHIPPED_CONTENT_DIR)
        .join("textures")
        .join(game.as_str());
    candidate.is_dir().then_some(candidate)
}

/// The directory to hand the Builder's `--textures` flag.
///
/// The Builder documents that flag as "Reborn Dolphin's Load/Textures
/// folder" and appends the game id itself, so a caller that passes an
/// already qualified pack root gets the id twice:
/// `library/textures/GN7E69/GN7E69/`. Every roster apply did exactly that
/// until 2026-09-17, which left 460 portraits one level too deep on every
/// install that had applied a roster.
///
/// It stayed invisible because Dolphin scans the game-id tree recursively, so
/// the misplaced portraits still loaded. What Dolphin actually did was log
/// "One or more textures at path '...' were already inserted" and drop one of
/// each duplicate pair, arbitrarily. While the pairs hold the same bytes that
/// costs only disk. The pair has to differ exactly once -- the next textures
/// update that replaces a portrait at the correct level -- for the stale
/// nested copy to win and the player to see the old face.
///
/// [`pristine_root`] is game-id-qualified on every branch but one: a saved
/// `migrated_texture_root` is whatever the player's 2.1 install used. So the
/// trailing component is stripped only when it really is the game id, and an
/// unqualified root is passed through for the Builder to qualify itself.
pub fn builder_textures_arg(platform: &Platform, game: &GameId) -> Result<PathBuf> {
    Ok(strip_trailing_game_id(&pristine_root(platform, game)?, game))
}

/// `root` without a trailing `<gameid>` component, or `root` unchanged when it
/// does not end in one.
fn strip_trailing_game_id(root: &Path, game: &GameId) -> PathBuf {
    let ends_in_id = root.file_name().and_then(|n| n.to_str()) == Some(game.as_str());
    match (ends_in_id, root.parent()) {
        (true, Some(parent)) => parent.to_path_buf(),
        _ => root.to_path_buf(),
    }
}

/// What [`heal_nested_game_folder`] found and did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Healed {
    /// Nested files deleted because the same bytes already sat at the right
    /// level. Nothing was lost.
    pub duplicates: usize,
    /// Nested files deleted because a DIFFERENT file held that texture name at
    /// the right level. The right-level file is the one a manifest lists, so it
    /// wins; the nested one was a stale Builder write that no manifest knows.
    pub superseded: Vec<String>,
    /// Nested files moved up, because nothing at the right level had them.
    /// These are real content and deleting them would have lost it.
    pub moved: Vec<String>,
}

impl Healed {
    pub fn is_empty(&self) -> bool {
        self.duplicates == 0 && self.superseded.is_empty() && self.moved.is_empty()
    }
}

/// Repair a texture tree that has a copy of its own game-id folder inside it.
///
/// `root` ends in the game id (`.../GN7E69`), so the folder to fold away is
/// `root/GN7E69`. Each file in it is deleted when the same bytes are already at
/// the right level, and MOVED UP otherwise -- never pruned on sight. On the
/// install this was found on, 459 of 460 were byte-identical duplicates and one
/// was a portrait that existed nowhere else, so a prune would have lost it.
///
/// Does nothing, cheaply, when there is no such folder. See
/// [`builder_textures_arg`] for what put it there.
pub fn heal_nested_game_folder(root: &Path) -> Result<Healed> {
    let Some(leaf) = root.file_name().map(|n| n.to_os_string()) else {
        return Ok(Healed::default());
    };
    let nested = root.join(&leaf);
    if !nested.is_dir() {
        return Ok(Healed::default());
    }
    let listing = walk(&nested)?;
    let mut healed = Healed::default();
    for file in &listing.files {
        let wrong = join_relative(&nested, &file.relative);
        let right = join_relative(root, &file.relative);
        if right.is_file() {
            if same_bytes(&wrong, &right)? {
                healed.duplicates += 1;
            } else {
                healed.superseded.push(file.relative.clone());
            }
            std::fs::remove_file(&wrong).map_err(|source| Error::Io { path: wrong, source })?;
            continue;
        }
        if let Some(parent) = right.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| Error::Io { path: parent.to_path_buf(), source })?;
        }
        std::fs::rename(&wrong, &right).map_err(|source| Error::Io { path: wrong, source })?;
        healed.moved.push(file.relative.clone());
    }
    // Whatever is left is empty directories, and an unsupported entry (a
    // symlink, a device) is left exactly where it is for a human to look at.
    remove_empty_dirs(&nested)?;
    Ok(healed)
}

/// Say what a heal did, or why it could not. Never fatal: a pack that still
/// has the nested folder composes, it just keeps paying for it.
fn report_heal(tree: &Path, outcome: Result<Healed>) {
    match outcome {
        Ok(healed) if healed.is_empty() => {}
        Ok(healed) => tracing::info!(
            "texture pack layout repaired in {}: {} duplicate(s) removed, {} moved up to the right level, {} superseded by the file already there",
            tree.display(),
            healed.duplicates,
            healed.moved.len(),
            healed.superseded.len(),
        ),
        Err(e) => tracing::warn!(
            "the nested game-id folder in {} could not be folded away ({e}); composing with it in place",
            tree.display()
        ),
    }
}

fn same_bytes(a: &Path, b: &Path) -> Result<bool> {
    let left = std::fs::metadata(a).map_err(|source| Error::Io { path: a.to_path_buf(), source })?;
    let right = std::fs::metadata(b).map_err(|source| Error::Io { path: b.to_path_buf(), source })?;
    if left.len() != right.len() {
        return Ok(false);
    }
    Ok(crate::library::hash_file(a)? == crate::library::hash_file(b)?)
}

/// Remove `dir` and every directory under it, bottom up, stopping at the first
/// one that still holds something.
fn remove_empty_dirs(dir: &Path) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|source| Error::Io { path: dir.to_path_buf(), source })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io { path: dir.to_path_buf(), source })?;
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            remove_empty_dirs(&entry.path())?;
        }
    }
    // Not an error: a leftover file is a deliberate stop, not a failure.
    let _ = std::fs::remove_dir(dir);
    Ok(())
}

/// Whether a content update has installed the textures component for this
/// game id into the data root.
///
/// The pointer is the evidence. The cached live manifest, when present, says
/// which game id the delivered tree is for; without it the pointer alone is
/// taken as delivered, because the textures component has one install root
/// today. Nothing here can fail the caller: an unreadable pointer is logged
/// and read as "not delivered", which falls back to the shipped tree.
fn textures_delivered(data_root: &Path, game: &str) -> bool {
    use crate::content::{ContentLayout, Pointer};
    let layout = ContentLayout::new(data_root);
    match Pointer::load(&layout.pointer_path("textures")) {
        Ok(Some(_)) => {}
        Ok(None) => return false,
        Err(e) => {
            tracing::warn!("the textures pointer could not be read ({e}); using the shipped tree");
            return false;
        }
    }
    let Ok(bytes) = std::fs::read(layout.manifest_path("textures")) else {
        return true;
    };
    match serde_json::from_slice::<crate::manifest::Component>(&bytes) {
        Ok(c) => c.install_root == format!("{}/{game}", DataDir::LibraryTextures.relative()),
        Err(_) => true,
    }
}

/// `<user folder>/Load/Textures/<gameid>/`, the composed tree Dolphin loads.
pub fn composed_root(user_dir: &Path, game: &GameId) -> PathBuf {
    user_dir.join("Load").join("Textures").join(game.as_str())
}

/// `mods/active/<gameid>/` under the data root: where builds before
/// 2026-09-12 composed. Only named so a sync can retire what is left there.
pub fn old_composed_root(platform: &Platform, game: &GameId) -> Result<PathBuf> {
    Ok(platform.paths.data_root()?.join("mods").join("active").join(game.as_str()))
}

/// What the textures content pointer says is installed, as one string, or
/// `None` when no update has delivered textures yet.
///
/// The update engine swaps the whole pristine folder, and the composed tree
/// is hardlinks into the OLD folder: with the same names and count nothing
/// used to notice, and the game kept the old bytes. A tree without a pointer
/// is never swapped by anything, so `None` is a stable identity.
pub fn pristine_identity(platform: &Platform, game: &GameId) -> Option<String> {
    let installed = |component: &str| -> Option<String> {
        let data = platform.paths.data_root().ok()?;
        let layout = crate::content::ContentLayout::new(&data);
        let pointer = crate::content::Pointer::load(&layout.pointer_path(component)).ok().flatten()?;
        Some(format!("{}#{}", pointer.current.version, pointer.current.sequence))
    };
    match game.as_str() {
        "GN7E69" => installed("textures"),
        // The league tree links into two folders an update swaps: its own, and the retail pack under it. Either
        // one changing must rebuild it.
        id if id == crate::league_edition::LEAGUE_GAME_ID => {
            let league = installed("league-game")?;
            Some(match installed("textures") {
                Some(retail) => format!("{league}+textures {retail}"),
                None => league,
            })
        }
        _ => None,
    }
}

/// Whether the texture pack Dolphin will read is actually there, and which
/// version it is.
///
/// The player's question is "do my textures load". This answers exactly that
/// and nothing more: no hashing, and no walk beyond finding one entry. The
/// pack is 27,000 files and 2.83 GB of it is oversized upscales, so anything
/// that reads it in full would be paid on a path the player waits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackStatus {
    /// `<user dir>/Load/Textures/<gameid>`, the folder Dolphin reads.
    pub textures_dir: PathBuf,
    /// It resolves and holds at least one entry. A broken junction does not
    /// resolve; an empty folder loads nothing.
    pub reachable: bool,
    /// It carries the launcher's marker, so it is the tree we composed
    /// rather than a folder someone else put there.
    pub composed_by_us: bool,
    /// What the textures content pointer says is installed.
    pub version: Option<String>,
}

/// Answers "will the pack load, and which version is it" for the folder
/// Dolphin actually reads, given the same user directory the launch will
/// pass with `-u`.
pub fn pack_status(platform: &Platform, user_dir: &Path, game: &GameId) -> PackStatus {
    let textures_dir = composed_root(user_dir, game);
    let reachable = std::fs::read_dir(&textures_dir)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    PackStatus {
        composed_by_us: textures_dir.join(COMPOSED_MARKER).is_file(),
        version: pristine_identity(platform, game),
        textures_dir,
        reachable,
    }
}

/// Will the pack load once Play runs, given the texture policy Play will
/// apply? [`pack_status`] reads the folder as it is now, but Play composes
/// that folder just before it boots (`sync_mods_for_launch`), so a fresh
/// install, a changed user folder, an unapply or a repaired disc all read
/// "not loading" on the Play page while the game itself loads every texture
/// (2026-10-04). This is the Play page's answer instead.
pub fn pack_loads_at_play(platform: &Platform, user_dir: &Path, game: &GameId, policy: TexturePolicy) -> bool {
    let pristine = pristine_root(platform, game).ok();
    pack_loads_at_play_with(pristine.as_deref(), user_dir, game, policy)
}

/// [`pack_loads_at_play`] with the shipped pack's root given (a test passes
/// its fixture). `Manage`: Play composes the shipped pack, so it loads when
/// the folder already has it or the shipped pack is there to compose.
/// `LeaveAlone`: the folder is used exactly as it is. `Withhold`: Play
/// removes it.
pub fn pack_loads_at_play_with(pristine: Option<&Path>, user_dir: &Path, game: &GameId, policy: TexturePolicy) -> bool {
    let has_entry = |dir: &Path| std::fs::read_dir(dir).map(|mut e| e.next().is_some()).unwrap_or(false);
    let now = has_entry(&composed_root(user_dir, game));
    match policy {
        TexturePolicy::Withhold => false,
        TexturePolicy::LeaveAlone => now,
        TexturePolicy::Manage => now || pristine.is_some_and(has_entry),
    }
}

/// `mods/packages/`, the validated payloads.
pub fn packages_root(platform: &Platform) -> Result<PathBuf> {
    platform.data_dir(DataDir::ModsPackages)
}

/// `mods/activation.json`.
///
/// Derived from [`DataDir::ModsPackages`] rather than by spelling `mods`
/// again, so the layout has one authority.
pub fn activation_path(platform: &Platform) -> Result<PathBuf> {
    let packages = platform.data_dir(DataDir::ModsPackages)?;
    let mods_dir = packages.parent().ok_or_else(|| {
        invalid(format!(
            "{} has no parent directory, so there is nowhere to keep \
             {ACTIVATION_FILE}",
            packages.display()
        ))
    })?;
    Ok(mods_dir.join(ACTIVATION_FILE))
}

/// A state that stops composition, with something the caller can do about it.
///
/// None of these is a fault. Each one is a situation a real install can be in,
/// and the launcher refusing is the correct outcome for all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeRefusal {
    NoPristineTree {
        path: PathBuf,
    },
    NoPristineFiles {
        path: PathBuf,
    },
    UnsupportedPristineEntries {
        paths: Vec<PathBuf>,
    },
    /// The composed root is the pristine root or is inside it.
    ComposedInsidePristine {
        pristine: PathBuf,
        composed: PathBuf,
    },
    /// The pristine root is inside the composed root, so rebuilding would
    /// delete it.
    PristineInsideComposed {
        pristine: PathBuf,
        composed: PathBuf,
    },
    InvalidPackage {
        id: ModId,
        detail: String,
    },
    MissingPayload {
        id: ModId,
        path: PathBuf,
    },
    PayloadSizeDiffers {
        id: ModId,
        path: PathBuf,
        declared: u64,
        found: u64,
    },
    HardConflicts {
        conflicts: Vec<Conflict>,
    },
    /// Something wants to write the launcher's own marker file.
    ReservedName {
        relative: String,
    },
    /// The composed root holds files but no marker, so the launcher did not
    /// create it and will not delete it.
    ForeignComposedTree {
        path: PathBuf,
    },
    /// Two entries differ only by letter case, which is two files on ext4 and
    /// one file on NTFS and APFS.
    CaseCollision {
        first: String,
        second: String,
    },
    /// One entry is a file and also the directory prefix of another entry.
    PathShadowsDirectory {
        file: String,
        inside: String,
    },
}

impl ComposeRefusal {
    /// A sentence the user or the caller can act on.
    pub fn message(&self) -> String {
        match self {
            ComposeRefusal::NoPristineTree { path } => format!(
                "No pristine texture tree at {}. Install or verify the textures \
                 component first: mods are composed over it and there is nothing \
                 to compose over.",
                path.display()
            ),
            ComposeRefusal::NoPristineFiles { path } => format!(
                "The pristine texture tree at {} is empty. Verify and repair the \
                 textures component: composing now would produce a tree holding \
                 only mod files, which looks like most textures are missing.",
                path.display()
            ),
            ComposeRefusal::UnsupportedPristineEntries { paths } => format!(
                "The pristine texture tree holds {} entries that are not plain \
                 files or directories, starting with {}. Verify and repair the \
                 textures component. A symlinked directory in there would make \
                 composition walk in a circle, and skipping one silently would \
                 drop a texture with nothing said about it.",
                paths.len(),
                paths
                    .first()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ),
            ComposeRefusal::ComposedInsidePristine { pristine, composed } => format!(
                "Refusing to compose into {} because it is inside the pristine \
                 tree {}. That would write mod files into the tree that is the \
                 only copy of the originals, which is exactly the JSGME failure \
                 this model exists to remove. Point the composed tree at \
                 the user folder's Load/Textures/<gameid>.",
                composed.display(),
                pristine.display()
            ),
            ComposeRefusal::PristineInsideComposed { pristine, composed } => format!(
                "Refusing to compose into {} because the pristine tree {} is \
                 inside it. Rebuilding deletes the composed tree, so this would \
                 delete the originals. Point the composed tree at \
                 the user folder's Load/Textures/<gameid>.",
                composed.display(),
                pristine.display()
            ),
            ComposeRefusal::InvalidPackage { id, detail } => {
                format!("Package {id} cannot be composed: {detail}")
            }
            ComposeRefusal::MissingPayload { id, path } => format!(
                "Package {id} lists a file that is not in its payload: {}. \
                 Re-import the package: its manifest and its files disagree.",
                path.display()
            ),
            ComposeRefusal::PayloadSizeDiffers {
                id,
                path,
                declared,
                found,
            } => format!(
                "Package {id} declares {declared} bytes for {} but the file is \
                 {found}. Re-import the package. A size taken from a file \
                 listing is rounded to the allocation unit and is not the byte \
                 length, and a truncated payload looks the same from here.",
                path.display()
            ),
            ComposeRefusal::HardConflicts { conflicts } => {
                let mut lines = vec![format!(
                    "{} mod conflict(s) must be resolved before composing:",
                    conflicts.len()
                )];
                lines.extend(conflicts.iter().map(|c| format!("  {}", c.message())));
                lines.join("\n")
            }
            ComposeRefusal::ReservedName { relative } => format!(
                "{relative} is the name of the launcher's own record at the root \
                 of the composed tree. Rename the file in the package, or repair \
                 the textures component if it came from there.",
            ),
            ComposeRefusal::ForeignComposedTree { path } => format!(
                "{} holds files but no {COMPOSED_MARKER}, so the launcher did \
                 not create it and will not delete it. Move the directory aside \
                 and compose again.",
                path.display()
            ),
            ComposeRefusal::CaseCollision { first, second } => format!(
                "{first} and {second} differ only by letter case. That is two \
                 files on Linux and one file on Windows and macOS, so the \
                 composed tree would have a different number of files depending \
                 on the machine. Rename one in the package that supplies it.",
            ),
            ComposeRefusal::PathShadowsDirectory { file, inside } => format!(
                "{file} is a file and also the directory holding {inside}. One \
                 of the two cannot exist. Re-import the package that supplies \
                 them.",
            ),
        }
    }
}

/// The result of planning.
#[derive(Debug, Clone)]
pub enum Composition {
    Planned(CompositionPlan),
    Refused(ComposeRefusal),
}

impl Composition {
    pub fn plan(&self) -> Option<&CompositionPlan> {
        match self {
            Composition::Planned(p) => Some(p),
            Composition::Refused(_) => None,
        }
    }

    pub fn refusal(&self) -> Option<&ComposeRefusal> {
        match self {
            Composition::Refused(r) => Some(r),
            Composition::Planned(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// The record written into a composed tree
// ---------------------------------------------------------------------------

/// One overlaid file, as recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayRecord {
    pub relative: String,
    pub id: ModId,
    pub sha256: Sha256Hex,
}

/// What a finished composed tree claims to be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposedRecord {
    pub game: GameId,
    pub pristine_root: PathBuf,
    /// See [`ComposeRequest::base_root`]. Absent for every tree without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_root: Option<PathBuf>,
    /// Which pristine tree the links point into. Absent in records written
    /// before 2026-09-12, which reads as "unknown" and differs from any
    /// planned identity, so those trees rebuild once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pristine_identity: Option<String>,
    pub activation: Vec<ActivePackage>,
    pub total_entries: u64,
    pub pristine_entries: u64,
    /// Every overlaid file with its expected hash.
    ///
    /// Only the overlays carry hashes, and that is deliberate. A package
    /// manifest is the only place a trustworthy expected hash for these files
    /// exists, and hashing the whole 2 GB pristine side on every check would
    /// cost more than it is worth: that side is covered by the signed textures
    /// manifest through [`crate::library`]. What is checked here instead is the
    /// count, which catches a pristine file that went missing.
    pub overlays: Vec<OverlayRecord>,
}

/// The file at the root of a composed tree.
///
/// Written before the first entry with `record` absent, and rewritten with the
/// record once composition finishes. The two states are load bearing:
///
/// - Its presence, in any state, is the launcher's proof that it created this
///   directory, which is what makes deleting the directory safe.
/// - A record is only present when composition finished, so an interrupted
///   composition leaves a tree that is deletable but never trusted. The other
///   order would let a partial tree pass verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposedMarker {
    pub schema: String,
    pub game: GameId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<ComposedRecord>,
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// One way a composed tree fails to be what the plan said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discrepancy {
    NotComposedByTheLauncher {
        path: PathBuf,
    },
    UnreadableRecord {
        path: PathBuf,
        detail: String,
    },
    /// A marker with no record: composition was interrupted.
    IncompleteComposition {
        path: PathBuf,
    },
    GameDiffers {
        recorded: GameId,
        planned: GameId,
    },
    PristineRootDiffers {
        recorded: PathBuf,
        planned: PathBuf,
    },
    BaseRootDiffers {
        recorded: Option<PathBuf>,
        planned: Option<PathBuf>,
    },
    /// The pristine tree was swapped (a texture update), so the links point
    /// at the old bytes even when every name and count still matches.
    PristineIdentityDiffers {
        recorded: Option<String>,
        planned: Option<String>,
    },
    ActivationDiffers {
        recorded: Vec<ActivePackage>,
        planned: Vec<ActivePackage>,
    },
    /// The tree does not hold the number of files its own record claims.
    EntryCountDiffers {
        recorded: u64,
        found: u64,
    },
    /// The record was written by a different plan than the one being checked.
    PlanCountDiffers {
        planned: u64,
        recorded: u64,
    },
    /// A pristine file was replaced under the same name, so the composed
    /// link still holds the old bytes. A roster update does this to the
    /// portraits it rewrites, and neither the textures identity nor any count
    /// changes. `count` is how many files differ; `relative` is the first.
    PristineFileReplaced {
        relative: String,
        count: u64,
    },
    MissingOverlay {
        relative: String,
    },
    OverlayContentDiffers {
        relative: String,
        expected: Sha256Hex,
        actual: Sha256Hex,
    },
}

impl Discrepancy {
    pub fn message(&self) -> String {
        match self {
            Discrepancy::NotComposedByTheLauncher { path } => format!(
                "{} carries no {COMPOSED_MARKER}, so it was not composed by this \
                 launcher.",
                path.display()
            ),
            Discrepancy::UnreadableRecord { path, detail } => {
                format!("{} could not be read: {detail}", path.display())
            }
            Discrepancy::IncompleteComposition { path } => {
                format!("{} was left part way through composing.", path.display())
            }
            Discrepancy::GameDiffers { recorded, planned } => {
                format!("the tree was composed for {recorded}, not {planned}.")
            }
            Discrepancy::PristineRootDiffers { recorded, planned } => format!(
                "the tree was composed from {}, not {}.",
                recorded.display(),
                planned.display()
            ),
            Discrepancy::BaseRootDiffers { recorded, planned } => {
                let shown = |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "none".into());
                format!("the tree was laid over {}, not {}.", shown(recorded), shown(planned))
            }
            Discrepancy::PristineIdentityDiffers { recorded, planned } => format!(
                "the pristine textures changed (was {}, now {})",
                recorded.as_deref().unwrap_or("unknown"),
                planned.as_deref().unwrap_or("unknown")
            ),
            Discrepancy::ActivationDiffers { recorded, planned } => format!(
                "the tree was composed from {} active package(s), the plan has {}.",
                recorded.len(),
                planned.len()
            ),
            Discrepancy::EntryCountDiffers { recorded, found } => {
                format!("the tree claims {recorded} files and holds {found}.")
            }
            Discrepancy::PlanCountDiffers { planned, recorded } => {
                format!("the plan has {planned} files, the tree records {recorded}.")
            }
            Discrepancy::PristineFileReplaced { relative, count } => format!(
                "{count} pristine file(s) changed since the tree was composed, \
                 {relative} first."
            ),
            Discrepancy::MissingOverlay { relative } => {
                format!("the mod file {relative} is not in the tree.")
            }
            Discrepancy::OverlayContentDiffers {
                relative,
                expected,
                actual,
            } => format!("the mod file {relative} hashes to {actual} and should be {expected}."),
        }
    }
}

/// Whether a composed tree matches a plan.
#[derive(Debug, Clone)]
pub struct Verification {
    pub composed_root: PathBuf,
    pub discrepancies: Vec<Discrepancy>,
}

impl Verification {
    pub fn matches_the_plan(&self) -> bool {
        self.discrepancies.is_empty()
    }

    /// Whether the tree should be rebuilt.
    ///
    /// Every discrepancy has the same answer, and that is not a shortcut: it is
    /// the property the composition model buys. Rebuilding costs directory
    /// entries and loses nothing, so there is no reason to diagnose further
    /// before doing it.
    pub fn needs_rebuild(&self) -> bool {
        !self.discrepancies.is_empty()
    }

    /// One line per discrepancy, for a log or a diagnostic bundle.
    pub fn report(&self) -> String {
        self.discrepancies
            .iter()
            .map(|d| d.message())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---------------------------------------------------------------------------
// Composing
// ---------------------------------------------------------------------------

/// What composition actually did.
#[derive(Debug, Clone)]
pub struct ComposeReport {
    pub game: GameId,
    pub composed_root: PathBuf,
    pub linked: u64,
    pub copied: u64,
    /// Bytes actually written, which is zero when every entry was linked.
    pub bytes_copied: u64,
    pub overlaid: u64,
    /// The first hardlink failure, verbatim.
    ///
    /// Present means the volume does not support hardlinks, which is how exFAT
    /// and FAT32 present, and the rest of the tree was copied. Kept as the
    /// message rather than a boolean so a per-file cause is still visible.
    pub link_failure: Option<String>,
    pub record: ComposedRecord,
}

impl ComposeReport {
    pub fn entries(&self) -> u64 {
        self.linked + self.copied
    }
}

/// Building and checking composed trees.
#[derive(Debug)]
pub struct Composer;

impl Composer {
    /// Work out every file the composed tree will hold, and refuse if anything
    /// about it is ambiguous or unsafe.
    ///
    /// Read only. Nothing on disk changes here, so a caller can show the plan,
    /// the conflicts and the disk cost before committing to it.
    pub fn plan(platform: &Platform, req: &ComposeRequest) -> Result<Composition> {
        for package in &req.packages {
            if let Err(e) = package.validate() {
                return Ok(Composition::Refused(ComposeRefusal::InvalidPackage {
                    id: package.id.clone(),
                    detail: e.to_string(),
                }));
            }
        }

        // The read-only rule, enforced before anything else, because every
        // other check is irrelevant if the destination is the source. The base
        // tree is a source too, held to the same rule.
        if let Some(base) = &req.base_root {
            if contains_or_equals(base, &req.composed_root) {
                return Ok(Composition::Refused(ComposeRefusal::ComposedInsidePristine {
                    pristine: base.clone(),
                    composed: req.composed_root.clone(),
                }));
            }
            if contains_or_equals(&req.composed_root, base) {
                return Ok(Composition::Refused(ComposeRefusal::PristineInsideComposed {
                    pristine: base.clone(),
                    composed: req.composed_root.clone(),
                }));
            }
        }
        if contains_or_equals(&req.pristine_root, &req.composed_root) {
            return Ok(Composition::Refused(
                ComposeRefusal::ComposedInsidePristine {
                    pristine: req.pristine_root.clone(),
                    composed: req.composed_root.clone(),
                },
            ));
        }
        if contains_or_equals(&req.composed_root, &req.pristine_root) {
            return Ok(Composition::Refused(
                ComposeRefusal::PristineInsideComposed {
                    pristine: req.pristine_root.clone(),
                    composed: req.composed_root.clone(),
                },
            ));
        }

        if !req.pristine_root.is_dir() {
            return Ok(Composition::Refused(ComposeRefusal::NoPristineTree {
                path: req.pristine_root.clone(),
            }));
        }
        // Fold away a copy of the pack's own game-id folder written inside the
        // pack, before anything reads the pack, so the plan is made from the
        // real file list. See `heal_nested_game_folder` for what put it there.
        // A failure is logged, not fatal: a pack with the extra folder still
        // composes, it just keeps costing what it costs.
        report_heal(&req.pristine_root, heal_nested_game_folder(&req.pristine_root));
        let pristine = PristineTree::open(&req.pristine_root)?;
        let listing = pristine.list()?;
        if !listing.unsupported.is_empty() {
            return Ok(Composition::Refused(
                ComposeRefusal::UnsupportedPristineEntries {
                    paths: listing.unsupported,
                },
            ));
        }
        if listing.files.is_empty() {
            return Ok(Composition::Refused(ComposeRefusal::NoPristineFiles {
                path: req.pristine_root.clone(),
            }));
        }

        // A directory the launcher did not compose is refused now rather than
        // when it is about to be deleted, so nothing is half done.
        if let Some(refusal) = foreign_tree_refusal(&req.composed_root)? {
            return Ok(Composition::Refused(refusal));
        }

        // Now that the tree is known to be one the launcher composed, the same
        // repair applies to it: an earlier compose copied the nested folder
        // across, and that is the copy Dolphin actually reads. Deliberately
        // after the check above and never before it -- a folder the launcher
        // did not compose is the player's, and this would have written into it.
        report_heal(&req.composed_root, heal_nested_game_folder(&req.composed_root));

        let conflicts = detect_conflicts(&req.packages, &req.game, req.payload_fallback.as_ref());
        let (hard, soft): (Vec<Conflict>, Vec<Conflict>) =
            conflicts.into_iter().partition(|c| c.is_hard());
        if !hard.is_empty() {
            return Ok(Composition::Refused(ComposeRefusal::HardConflicts {
                conflicts: hard,
            }));
        }

        let mut entries: BTreeMap<String, PlannedEntry> = BTreeMap::new();
        // The texture names the build's own tree holds, when it is laid over a
        // base: a base file or a mod file with one of these names is left out.
        let mut own_names: BTreeSet<String> = BTreeSet::new();
        if let Some(base_root) = &req.base_root {
            if !base_root.is_dir() {
                return Ok(Composition::Refused(ComposeRefusal::NoPristineTree { path: base_root.clone() }));
            }
            report_heal(base_root, heal_nested_game_folder(base_root));
            let base = PristineTree::open(base_root)?.list()?;
            if !base.unsupported.is_empty() {
                return Ok(Composition::Refused(ComposeRefusal::UnsupportedPristineEntries { paths: base.unsupported }));
            }
            if base.files.is_empty() {
                return Ok(Composition::Refused(ComposeRefusal::NoPristineFiles { path: base_root.clone() }));
            }
            own_names = listing.files.iter().map(|f| texture_name(&f.relative)).collect();
            for file in &base.files {
                if file.relative == COMPOSED_MARKER || own_names.contains(&texture_name(&file.relative)) {
                    continue;
                }
                entries.insert(
                    file.relative.clone(),
                    PlannedEntry {
                        relative: file.relative.clone(),
                        bytes: file.bytes,
                        source: EntrySource::Pristine {
                            path: join_relative(base_root, &file.relative),
                            modified: file.modified,
                        },
                    },
                );
            }
        }
        for file in &listing.files {
            if file.relative == COMPOSED_MARKER {
                return Ok(Composition::Refused(ComposeRefusal::ReservedName {
                    relative: file.relative.clone(),
                }));
            }
            entries.insert(
                file.relative.clone(),
                PlannedEntry {
                    relative: file.relative.clone(),
                    bytes: file.bytes,
                    source: EntrySource::Pristine {
                        path: join_relative(&req.pristine_root, &file.relative),
                        modified: file.modified,
                    },
                },
            );
        }

        let mut activation = Vec::new();
        let mut not_targeted = Vec::new();
        for package in &req.packages {
            let Some(payload) = package.payload_game(&req.game, req.payload_fallback.as_ref()) else {
                not_targeted.push(package.id.clone());
                continue;
            };
            for file in package.files_for(&payload) {
                // The build's own textures win over a mod (Adam 10-16).
                if own_names.contains(&texture_name(file.path.tail())) {
                    continue;
                }
                let source = package.payload_file(&req.packages_root, file);
                let meta = match std::fs::metadata(&source) {
                    Ok(m) if m.is_file() => m,
                    _ => {
                        return Ok(Composition::Refused(ComposeRefusal::MissingPayload {
                            id: package.id.clone(),
                            path: source,
                        }))
                    }
                };
                if meta.len() != file.bytes {
                    return Ok(Composition::Refused(ComposeRefusal::PayloadSizeDiffers {
                        id: package.id.clone(),
                        path: source,
                        declared: file.bytes,
                        found: meta.len(),
                    }));
                }

                let relative = file.path.tail().to_string();
                let replaces_pristine = matches!(
                    entries.get(&relative).map(|e| &e.source),
                    Some(EntrySource::Pristine { .. })
                );
                entries.insert(
                    relative.clone(),
                    PlannedEntry {
                        relative,
                        bytes: file.bytes,
                        source: EntrySource::Overlay {
                            id: package.id.clone(),
                            path: source,
                            sha256: file.sha256.clone(),
                            replaces_pristine,
                        },
                    },
                );
            }
            activation.push(package.as_active());
        }

        if let Some(refusal) = shape_refusal(&entries) {
            return Ok(Composition::Refused(refusal));
        }

        // A cross-volume hardlink cannot work, so there is no point attempting
        // one. Within a volume it can still fail, on exFAT and FAT32, and that
        // is handled at composition time.
        let linkable = |from: &Path| matches!(platform.files.same_volume(from, &req.composed_root), Ok(true));
        let method = if linkable(&req.pristine_root) && req.base_root.as_deref().is_none_or(linkable) {
            LinkMethod::Hardlink
        } else {
            LinkMethod::Copy
        };

        Ok(Composition::Planned(CompositionPlan {
            game: req.game.clone(),
            pristine_root: req.pristine_root.clone(),
            base_root: req.base_root.clone(),
            composed_root: req.composed_root.clone(),
            pristine_identity: req.pristine_identity.clone(),
            method,
            entries: entries.into_values().collect(),
            activation,
            not_targeted,
            soft_conflicts: soft,
        }))
    }

    /// Build the composed tree, discarding whatever was there.
    ///
    /// Un-applying a mod is this function with a shorter package list. There is
    /// no separate un-apply path, and that is the design: an operation that
    /// restores files can fail half way and leave a tree that is neither state,
    /// while a rebuild either finishes or is discarded and run again.
    ///
    /// `_lock` is proof that this process holds the single-instance lock. Two
    /// launchers composing one tree would interleave writes into the directory
    /// Dolphin is about to load, and the loser's composition would be silently
    /// half present. Taking the [`Supervisor`] makes that check something the
    /// compiler tracks instead of something to remember.
    pub fn apply(
        platform: &Platform,
        _lock: &Supervisor,
        plan: &CompositionPlan,
    ) -> Result<ComposeReport> {
        // Re-checked rather than trusted from the plan. A plan is a value with
        // public fields that can be built or edited anywhere, and this is the
        // check whose failure destroys the only copy of the originals.
        let overlaps = |source: &Path| {
            contains_or_equals(source, &plan.composed_root) || contains_or_equals(&plan.composed_root, source)
        };
        if let Some(base) = plan.base_root.as_deref().filter(|b| overlaps(b)) {
            return Err(refused(format!(
                "refusing to compose: the composed tree {} and the base tree {} overlap, so composing would \
                 write into or delete the only copy of the original textures",
                plan.composed_root.display(),
                base.display()
            )));
        }
        if overlaps(&plan.pristine_root) {
            return Err(refused(format!(
                "refusing to compose: the composed tree {} and the pristine tree \
                 {} overlap, so composing would write into or delete the only \
                 copy of the original textures",
                plan.composed_root.display(),
                plan.pristine_root.display()
            )));
        }

        // Built beside the live tree, then swapped in. The user folder's
        // texture path is a junction that names the composed root by path,
        // so the game sees the whole old tree until the swap and the whole
        // new one after it, never a cleared or half-built folder. A build
        // that fails leaves the old tree exactly as it was. (Adam,
        // 2026-09-12, after an update on another PC: "all the textures went
        // away", "It shouldn't happen at all though".)
        let building = beside(&plan.composed_root, ".building");
        let previous = beside(&plan.composed_root, ".previous");
        Self::discard(_lock, &building)?;
        Self::discard(_lock, &previous)?;
        std::fs::create_dir_all(&building).map_err(|source| Error::Io {
            path: building.clone(),
            source,
        })?;

        // The in-progress marker goes down first, and is flushed, because it is
        // the permission to delete this directory. If composition dies after
        // the first entry is written but before the marker reached the disk,
        // the tree would look foreign and the next rebuild would refuse to
        // clear it, leaving no way forward without the user deleting a folder
        // by hand.
        write_marker_durably(
            &building,
            &ComposedMarker {
                schema: COMPOSED_SCHEMA.to_string(),
                game: plan.game.clone(),
                record: None,
            },
        )?;

        let mut linked = 0u64;
        let mut copied = 0u64;
        let mut bytes_copied = 0u64;
        let mut overlaid = 0u64;
        let mut link_failure: Option<String> = None;
        let mut links_unavailable = plan.method == LinkMethod::Copy;

        for entry in &plan.entries {
            let source = match &entry.source {
                EntrySource::Pristine { path, .. } => path,
                EntrySource::Overlay { path, .. } => {
                    overlaid += 1;
                    path
                }
            };
            let dest = join_relative(&building, &entry.relative);

            if !links_unavailable {
                match platform.files.hardlink(source, &dest) {
                    Ok(()) => {
                        linked += 1;
                        continue;
                    }
                    Err(e) => {
                        // A filesystem either supports hardlinks or it does
                        // not, so the whole rest of the tree is copied rather
                        // than failing 2,762 more times.
                        links_unavailable = true;
                        link_failure = Some(e.to_string());
                    }
                }
            }

            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let written = std::fs::copy(source, &dest).map_err(|source| Error::Io {
                path: dest.clone(),
                source,
            })?;
            // A copy has to carry its source's write time, as a link does, or
            // verification would read every copied file as replaced and
            // rebuild on every launch. Windows keeps it on copy; other
            // platforms do not.
            if let Ok(when) = std::fs::metadata(source).and_then(|m| m.modified()) {
                if let Err(e) = std::fs::File::options()
                    .write(true)
                    .open(&dest)
                    .and_then(|f| f.set_modified(when))
                {
                    tracing::warn!("could not carry the write time onto {}: {e}", dest.display());
                }
            }
            copied += 1;
            bytes_copied += written;
        }

        let record = plan.record();
        write_marker_durably(
            &building,
            &ComposedMarker {
                schema: COMPOSED_SCHEMA.to_string(),
                game: plan.game.clone(),
                record: Some(record.clone()),
            },
        )?;

        // The swap. The old tree is checked the way discard checks it: only
        // a composed tree (or an empty folder) is ever moved out of the way.
        if plan.composed_root.exists() {
            if !plan.composed_root.is_dir() {
                return Err(refused(format!(
                    "{} is not a directory, so it is not a composed tree. Move it \
                     aside and compose again",
                    plan.composed_root.display()
                )));
            }
            if let Some(refusal) = foreign_tree_refusal(&plan.composed_root)? {
                return Err(refused(refusal.message()));
            }
            std::fs::rename(&plan.composed_root, &previous).map_err(|source| Error::Io {
                path: plan.composed_root.clone(),
                source,
            })?;
        }
        if let Err(source) = std::fs::rename(&building, &plan.composed_root) {
            // The old tree goes back so the game is never left without one.
            if previous.exists() {
                let _ = std::fs::rename(&previous, &plan.composed_root);
            }
            return Err(Error::Io { path: plan.composed_root.clone(), source });
        }
        if previous.exists() {
            // Best effort: a leftover is discarded at the next build.
            let _ = std::fs::remove_dir_all(&previous);
        }

        Ok(ComposeReport {
            game: plan.game.clone(),
            composed_root: plan.composed_root.clone(),
            linked,
            copied,
            bytes_copied,
            overlaid,
            link_failure,
            record,
        })
    }

    /// Remove a composed tree. Returns whether anything was removed.
    ///
    /// Only a directory carrying [`COMPOSED_MARKER`], or an empty one, is
    /// removed. Anything else is refused with a message, because the shipped
    /// 2.1 uninstaller ran `DelTree` over a folder holding the user's memory
    /// cards and this module is not going to add a second recursive delete that
    /// trusts whatever path it was handed.
    pub fn discard(_lock: &Supervisor, composed_root: &Path) -> Result<bool> {
        if !composed_root.exists() {
            return Ok(false);
        }
        if !composed_root.is_dir() {
            return Err(refused(format!(
                "{} is not a directory, so it is not a composed tree. Move it \
                 aside and compose again",
                composed_root.display()
            )));
        }
        if let Some(refusal) = foreign_tree_refusal(composed_root)? {
            return Err(refused(refusal.message()));
        }
        std::fs::remove_dir_all(composed_root).map_err(|source| Error::Io {
            path: composed_root.to_path_buf(),
            source,
        })?;
        Ok(true)
    }

    /// Check a composed tree against the plan that should have built it.
    ///
    /// Read only, and safe to run before a launch. The overlays are hashed in
    /// full because a package manifest gives a trustworthy expected value for
    /// them. The pristine side is checked by count, for the reason recorded on
    /// [`ComposedRecord::overlays`].
    pub fn verify(plan: &CompositionPlan) -> Result<Verification> {
        let mut found = Vec::new();
        let marker_path = plan.composed_root.join(COMPOSED_MARKER);

        let raw = match std::fs::read(&marker_path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                found.push(Discrepancy::NotComposedByTheLauncher {
                    path: plan.composed_root.clone(),
                });
                return Ok(Verification {
                    composed_root: plan.composed_root.clone(),
                    discrepancies: found,
                });
            }
            Err(source) => {
                return Err(Error::Io {
                    path: marker_path,
                    source,
                })
            }
        };

        let marker: ComposedMarker = match serde_json::from_slice(&raw) {
            Ok(m) => m,
            Err(e) => {
                found.push(Discrepancy::UnreadableRecord {
                    path: marker_path,
                    detail: e.to_string(),
                });
                return Ok(Verification {
                    composed_root: plan.composed_root.clone(),
                    discrepancies: found,
                });
            }
        };
        if marker.schema != COMPOSED_SCHEMA {
            found.push(Discrepancy::UnreadableRecord {
                path: marker_path,
                detail: format!(
                    "schema is {:?}, this launcher writes {COMPOSED_SCHEMA}",
                    marker.schema
                ),
            });
            return Ok(Verification {
                composed_root: plan.composed_root.clone(),
                discrepancies: found,
            });
        }
        let Some(record) = marker.record else {
            found.push(Discrepancy::IncompleteComposition {
                path: plan.composed_root.clone(),
            });
            return Ok(Verification {
                composed_root: plan.composed_root.clone(),
                discrepancies: found,
            });
        };

        if record.game != plan.game {
            found.push(Discrepancy::GameDiffers {
                recorded: record.game.clone(),
                planned: plan.game.clone(),
            });
        }
        if !same_path(&record.pristine_root, &plan.pristine_root) {
            found.push(Discrepancy::PristineRootDiffers {
                recorded: record.pristine_root.clone(),
                planned: plan.pristine_root.clone(),
            });
        }
        let same_base = match (&record.base_root, &plan.base_root) {
            (None, None) => true,
            (Some(a), Some(b)) => same_path(a, b),
            _ => false,
        };
        if !same_base {
            found.push(Discrepancy::BaseRootDiffers {
                recorded: record.base_root.clone(),
                planned: plan.base_root.clone(),
            });
        }
        if record.pristine_identity != plan.pristine_identity {
            found.push(Discrepancy::PristineIdentityDiffers {
                recorded: record.pristine_identity.clone(),
                planned: plan.pristine_identity.clone(),
            });
        }
        if record.activation != plan.activation {
            found.push(Discrepancy::ActivationDiffers {
                recorded: record.activation.clone(),
                planned: plan.activation.clone(),
            });
        }
        if record.total_entries != plan.entries.len() as u64 {
            found.push(Discrepancy::PlanCountDiffers {
                planned: plan.entries.len() as u64,
                recorded: record.total_entries,
            });
        }

        let listing = walk(&plan.composed_root)?;
        let on_disk = listing
            .files
            .iter()
            .filter(|f| f.relative != COMPOSED_MARKER)
            .count() as u64;
        if on_disk != record.total_entries {
            found.push(Discrepancy::EntryCountDiffers {
                recorded: record.total_entries,
                found: on_disk,
            });
        }

        // The pristine side, by length and write time, from metadata both
        // walks already read, so this costs no extra disk access. A link and
        // its source are one file and always agree; a source replaced under
        // the same name (a roster rewriting a portrait) does not.
        let composed: BTreeMap<&str, &PristineEntry> =
            listing.files.iter().map(|f| (f.relative.as_str(), f)).collect();
        let mut replaced: Option<String> = None;
        let mut replaced_count = 0u64;
        for entry in &plan.entries {
            let EntrySource::Pristine { modified, .. } = &entry.source else {
                continue;
            };
            let same = composed.get(entry.relative.as_str()).is_some_and(|c| {
                c.bytes == entry.bytes && same_write_time(c.modified, *modified)
            });
            if !same {
                replaced_count += 1;
                replaced.get_or_insert_with(|| entry.relative.clone());
            }
        }
        if let Some(relative) = replaced {
            found.push(Discrepancy::PristineFileReplaced {
                relative,
                count: replaced_count,
            });
        }

        for overlay in plan.overlays() {
            let path = join_relative(&plan.composed_root, &overlay.relative);
            if !path.is_file() {
                found.push(Discrepancy::MissingOverlay {
                    relative: overlay.relative,
                });
                continue;
            }
            let actual = hash_file(&path)?;
            if actual != overlay.sha256 {
                found.push(Discrepancy::OverlayContentDiffers {
                    relative: overlay.relative,
                    expected: overlay.sha256,
                    actual,
                });
            }
        }

        Ok(Verification {
            composed_root: plan.composed_root.clone(),
            discrepancies: found,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `None` when the directory is absent, empty, or carries the marker.
/// `path` with `suffix` appended to its last component: the folder a tree
/// is built in, or moved to, beside the live one.
fn beside(path: &Path, suffix: &str) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{name}{suffix}"))
}

fn foreign_tree_refusal(composed_root: &Path) -> Result<Option<ComposeRefusal>> {
    if !composed_root.exists() {
        return Ok(None);
    }
    if composed_root.join(COMPOSED_MARKER).exists() {
        return Ok(None);
    }
    let mut reader = std::fs::read_dir(composed_root).map_err(|source| Error::Io {
        path: composed_root.to_path_buf(),
        source,
    })?;
    match reader.next() {
        None => Ok(None),
        Some(_) => Ok(Some(ComposeRefusal::ForeignComposedTree {
            path: composed_root.to_path_buf(),
        })),
    }
}

/// Refuse an entry set that cannot exist as written on every platform.
fn shape_refusal(entries: &BTreeMap<String, PlannedEntry>) -> Option<ComposeRefusal> {
    let mut folded: BTreeMap<String, &String> = BTreeMap::new();
    for relative in entries.keys() {
        let key = relative.to_lowercase();
        if let Some(first) = folded.insert(key, relative) {
            if first != relative {
                return Some(ComposeRefusal::CaseCollision {
                    first: first.clone(),
                    second: relative.clone(),
                });
            }
        }
    }

    for relative in entries.keys() {
        let prefix = format!("{relative}/");
        if let Some((inside, _)) = entries.range(prefix.clone()..).next() {
            if inside.starts_with(&prefix) {
                return Some(ComposeRefusal::PathShadowsDirectory {
                    file: relative.clone(),
                    inside: inside.clone(),
                });
            }
        }
    }
    None
}

/// Join a forward-slash relative path onto a root, one segment at a time.
///
/// Not `root.join(relative)`: a relative path containing a separator is
/// accepted by `join` and its meaning then depends on the platform. Every
/// relative path here was built segment by segment, so it is put back together
/// the same way.
fn join_relative(root: &Path, relative: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for segment in relative.split('/') {
        path = path.join(segment);
    }
    path
}

/// Path components, absolute where possible, case folded.
///
/// Case is folded on every platform, not just the case-insensitive ones,
/// because this module may not ask which platform it is on and the safe
/// direction is unambiguous: folding can only make the containment checks
/// refuse more often, and a refusal costs a message while a miss costs the
/// original textures.
fn segments_lower(path: &Path) -> Vec<String> {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    absolute
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

/// Whether `inner` is `outer` or is inside it.
///
/// Compared component by component rather than by string prefix, so
/// `textures2` is not treated as being inside `textures`.
fn contains_or_equals(outer: &Path, inner: &Path) -> bool {
    let (o, i) = (segments_lower(outer), segments_lower(inner));
    i.len() >= o.len() && i[..o.len()] == o[..]
}

fn same_path(a: &Path, b: &Path) -> bool {
    segments_lower(a) == segments_lower(b)
}

/// Write the marker and get it to the disk before returning.
///
/// Flushed for the same reason the session log is: it is the only evidence of
/// something the launcher cannot reconstruct, in this case its own permission
/// to delete the directory it is about to fill.
fn write_marker_durably(composed_root: &Path, marker: &ComposedMarker) -> Result<()> {
    use std::io::Write;

    let path = composed_root.join(COMPOSED_MARKER);
    let bytes = serde_json::to_vec_pretty(marker)
        .map_err(|e| invalid(format!("could not encode the composed record: {e}")))?;
    let mut file = std::fs::File::create(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    file.write_all(&bytes).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    file.sync_all().map_err(|source| Error::Io { path, source })
}

// ---------------------------------------------------------------------------
// Making the active set the one the game loads
// ---------------------------------------------------------------------------

/// Everything a sync needs, named explicitly so a test can point every root
/// at a temporary directory. [`SyncRequest::in_data_root`] is the layout a
/// real launch uses.
#[derive(Debug, Clone)]
pub struct SyncRequest {
    pub game: GameId,
    pub pristine_root: PathBuf,
    /// See [`ComposeRequest::base_root`].
    pub base_root: Option<PathBuf>,
    pub composed_root: PathBuf,
    pub packages_root: PathBuf,
    pub activation_file: PathBuf,
    /// The emulator's user directory. `Load/Textures/<game>` under it IS the
    /// composed tree.
    pub user_dir: PathBuf,
    /// Where builds before 2026-09-12 composed (`mods/active/<game>`), so a
    /// sync can retire what is left there. `None` skips that.
    pub old_composed_root: Option<PathBuf>,
    /// See [`ComposeRequest::pristine_identity`].
    pub pristine_identity: Option<String>,
    /// See [`payload_fallback_for`].
    pub payload_fallback: Option<GameId>,
    /// Compose THIS list instead of reading `activation.json`, leaving the
    /// file as it is. `Some(vec![])` is "start with mods disabled": the
    /// game loads the pristine tree once, and the player's selection is
    /// still there for the next launch.
    pub active_override: Option<Vec<ActivePackage>>,
    /// What the launcher may do with the emulator's texture folder.
    pub texture_policy: TexturePolicy,
}

/// What the launcher may do with `Load/Textures/<game>`.
///
/// Three states, not two, because "do not compose" and "take the pack away"
/// are opposite intentions that a single boolean used to have to carry.
/// [`TexturePolicy::LeaveAlone`] means somebody else owns that folder;
/// [`TexturePolicy::Withhold`] means the folder is ours and the image about
/// to boot is not, so what is in it has to go. A boolean shared between them
/// answered "leave it exactly as it is" to both questions, which is right
/// for the author and is precisely the "old names with new faces" bug for
/// the unverified disc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexturePolicy {
    /// Compose the active set into it. The normal case.
    Manage,
    /// A texture author owns that folder: nothing is composed, and nothing
    /// in it is touched or removed. Theirs is the source of truth.
    LeaveAlone,
    /// The image this is for is not one Reborn verified as its own, so
    /// Reborn's art must not be painted on it. A composed tree already there
    /// is removed, which is the only way the pack actually stops loading:
    /// `HiresTextures` is a locked setting and Dolphin loads whatever sits
    /// under the booted game's ID.
    Withhold,
}

impl SyncRequest {
    /// The standard layout under the launcher's data root, for the user
    /// directory a launch resolved.
    pub fn in_data_root(platform: &Platform, game: GameId, user_dir: &Path) -> Result<Self> {
        Ok(Self {
            pristine_root: pristine_root(platform, &game)?,
            base_root: base_root_for(platform, &game)?,
            composed_root: composed_root(user_dir, &game),
            packages_root: packages_root(platform)?,
            activation_file: activation_path(platform)?,
            user_dir: user_dir.to_path_buf(),
            old_composed_root: old_composed_root(platform, &game).ok(),
            pristine_identity: pristine_identity(platform, &game),
            payload_fallback: payload_fallback_for(&game),
            game,
            active_override: None,
            texture_policy: TexturePolicy::Manage,
        })
    }
}

/// What the composed tree needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeStep {
    /// Not composed, on purpose: the folder is managed by hand.
    LeftAlone,
    /// Not composed, and what Reborn had composed there was taken away,
    /// because the image this sync was for is not one Reborn verified.
    /// `removed` is false when there was nothing of ours there to remove.
    Withheld { removed: bool },
    /// It already matched the active set, so it was left alone.
    ///
    /// This is the common case, and it is what makes syncing on every launch
    /// affordable: a verification is a directory walk plus a hash of the few
    /// overlaid files, where a rebuild is eight thousand deletes and eight
    /// thousand links.
    AlreadyCurrent,
    /// It did not match, so it was rebuilt from the pristine tree.
    Rebuilt {
        entries: u64,
        overlaid: u64,
        copied: u64,
        /// Why hardlinking was abandoned part way, when it was.
        link_failure: Option<String>,
    },
}

/// The game will load the active set, and this is what it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub game: GameId,
    pub composed_root: PathBuf,
    /// What `activation.json` listed, in order.
    pub active: Vec<ActivePackage>,
    /// Listed as active but not installed, so left out of the tree.
    ///
    /// Reported, and left in the list. Removing the entry would be the launcher
    /// deciding a mod is off because its files went missing, and a player who
    /// reinstalls the package expects it to come back on by itself.
    pub missing: Vec<ActivePackage>,
    pub tree: TreeStep,
    /// Where a folder of somebody's own textures went, when one sat where
    /// the composed tree goes. Renamed beside it, never deleted.
    pub moved_aside: Option<PathBuf>,
}

/// The outcome of a sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// Boxed: the report carries paths and lists, and an unboxed one made
    /// every enum that carries this one, up to the launch result, that big.
    Done(Box<SyncReport>),
    /// Nothing on disk changed, and this is why. The game loads whatever it
    /// loaded before, which is the right thing to do with a refusal: a launch
    /// blocked over button art is worse than the art being stale.
    Refused(String),
}

impl SyncReport {
    /// One line for a player or a log.
    pub fn explain(&self) -> String {
        let count = self.active.len() - self.missing.len();
        let tree = match &self.tree {
            TreeStep::LeftAlone => "left alone, because the launcher was asked not to manage it".to_string(),
            TreeStep::Withheld { removed } => if *removed {
                "removed, because this game file is not an NFL Street Reborn build".to_string()
            } else {
                "not composed, because this game file is not an NFL Street Reborn build".to_string()
            },
            TreeStep::AlreadyCurrent => "already in place".to_string(),
            TreeStep::Rebuilt { entries, .. } => format!("rebuilt, {entries} entries"),
        };
        let link = match &self.moved_aside {
            Some(aside) => format!(
                "in the user folder; what was there is now {}",
                aside.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            ),
            None => "in the user folder".to_string(),
        };
        let missing = if self.missing.is_empty() {
            String::new()
        } else {
            format!(
                " {} listed but not installed: {}.",
                self.missing.len(),
                self.missing
                    .iter()
                    .map(|m| m.id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        format!("{count} mod(s) on, tree {tree}, texture folder {link}.{missing}")
    }
}

impl SyncOutcome {
    /// One line for a player or a log.
    pub fn explain(&self) -> String {
        match self {
            SyncOutcome::Refused(why) => format!("Mods were not applied: {why}"),
            SyncOutcome::Done(report) => report.explain(),
        }
    }
}

/// Make the game load exactly what `activation.json` says is on.
///
/// This is the one operation everything that changes the active set ends in:
/// a launch, a controls profile switch, a mod toggled on the Mods page. Before
/// it existed, activation was written correctly by every one of those and
/// nothing rebuilt the tree or pointed the emulator at it, so the activation
/// was right and invisible.
///
/// Order: read the list, load what it names, plan, verify the tree that is
/// there against the plan, rebuild only if it differs, then link. Verification
/// before rebuild is what keeps this cheap enough to run on every launch.
///
/// Refuses rather than errors when the plan refuses, because a plan refusal
/// is a state a library can be in (two mods on one football) and the caller
/// decides whether that blocks anything. Real I/O failures are errors.
pub fn sync_active(
    platform: &Platform,
    lock: &Supervisor,
    req: &SyncRequest,
    now_ms: u64,
) -> Result<SyncOutcome> {
    let active = match &req.active_override {
        Some(list) => list.clone(),
        None => Activation::load(&req.activation_file)?.active,
    };

    match req.texture_policy {
        TexturePolicy::LeaveAlone => {
            // The author's folder: nothing is composed and nothing is touched.
            //
            // This is checked BEFORE Withhold on purpose. Somebody who turned
            // the launcher's management off has asserted that the contents of
            // that folder are theirs, and Reborn does not delete out of a
            // folder it was told not to manage. The composed marker is not
            // proof the tree is free of their work either: an author who
            // started from Reborn's pack and edited files in place still has
            // the marker sitting next to their edits. Adam lost a folder to a
            // delete like that once, and the launcher is not going to add a
            // second one to enforce a rule the player has already overridden
            // in the only way the launcher offers.
            return Ok(SyncOutcome::Done(Box::new(SyncReport {
                game: req.game.clone(),
                composed_root: req.composed_root.clone(),
                active,
                missing: Vec::new(),
                tree: TreeStep::LeftAlone,
                moved_aside: None,
            })));
        }
        TexturePolicy::Withhold => {
            // Not our image, so not our art on it. Removing the composed tree
            // is the only thing that actually switches the pack off: Dolphin
            // has `HiresTextures` locked on and loads whatever is filed under
            // the booted disc's game ID, so composing nothing this time would
            // leave every previously composed player exactly as broken.
            //
            // `Composer::discard` is reused rather than a second removal
            // written here: it removes only a directory carrying our own
            // marker, or an empty one, and refuses anything else with a
            // sentence. A refusal is carried out as a refusal, never as an
            // error, because the tree standing is a state the caller has to
            // be able to show rather than a failure that should stop a launch.
            if let Some(refusal) = foreign_tree_refusal(&req.composed_root)? {
                return Ok(SyncOutcome::Refused(refusal.message()));
            }
            let removed = Composer::discard(lock, &req.composed_root)?;
            return Ok(SyncOutcome::Done(Box::new(SyncReport {
                game: req.game.clone(),
                composed_root: req.composed_root.clone(),
                active,
                missing: Vec::new(),
                tree: TreeStep::Withheld { removed },
                moved_aside: None,
            })));
        }
        TexturePolicy::Manage => {}
    }

    let mut packages = Vec::new();
    let mut missing = Vec::new();
    for entry in &active {
        let dir = req
            .packages_root
            .join(entry.id.as_str())
            .join(entry.version.as_str());
        match ModPackage::load(&dir) {
            Ok(package) => packages.push(package),
            Err(_) => missing.push(entry.clone()),
        }
    }

    let compose = ComposeRequest {
        game: req.game.clone(),
        pristine_root: req.pristine_root.clone(),
        base_root: req.base_root.clone(),
        composed_root: req.composed_root.clone(),
        packages_root: req.packages_root.clone(),
        packages,
        pristine_identity: req.pristine_identity.clone(),
        payload_fallback: req.payload_fallback.clone(),
    };
    let plan = match Composer::plan(platform, &compose)? {
        Composition::Planned(plan) => plan,
        Composition::Refused(refusal) => return Ok(SyncOutcome::Refused(refusal.message())),
    };

    // Whatever sits where the composed tree goes: a junction from an earlier
    // build, or a folder of the player's own, is dealt with before verify
    // looks at it.
    let moved_aside = prepare_texture_folder(&req.composed_root, now_ms)?;

    let tree = if Composer::verify(&plan)?.matches_the_plan() {
        TreeStep::AlreadyCurrent
    } else {
        let report = Composer::apply(platform, lock, &plan)?;
        TreeStep::Rebuilt {
            entries: report.entries(),
            overlaid: report.overlaid,
            copied: report.copied,
            link_failure: report.link_failure,
        }
    };

    if let Some(old) = &req.old_composed_root {
        retire_old_composed_tree(old);
    }

    Ok(SyncOutcome::Done(Box::new(SyncReport {
        game: req.game.clone(),
        composed_root: req.composed_root.clone(),
        active,
        missing,
        tree,
        moved_aside,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::{Startup, SystemClock};

    const BALL: &str = "tex1_128x128_m_ffc5151a141db330_14.dds";

    fn game() -> GameId {
        GameId::parse("GN7E69").unwrap()
    }

    // --- the nested game-id folder: a pack root with a copy of itself inside
    //
    // What put it there: the Builder's --textures flag wants Dolphin's
    // Load/Textures folder and appends the game id, but four call sites passed
    // the already qualified pack root. See mods::builder_textures_arg.

    #[test]
    fn strip_trailing_game_id_removes_only_a_real_game_id() {
        let g = game();
        // Qualified: the Builder would double it, so hand it the parent.
        assert_eq!(
            strip_trailing_game_id(Path::new("/data/library/textures/GN7E69"), &g),
            PathBuf::from("/data/library/textures")
        );
        // Not qualified (a saved migrated_texture_root): pass it through and
        // let the Builder qualify it itself.
        assert_eq!(
            strip_trailing_game_id(Path::new("/old/Reborn 2.1/User/Load/Textures"), &g),
            PathBuf::from("/old/Reborn 2.1/User/Load/Textures")
        );
        // A different game id is not this game's id.
        assert_eq!(
            strip_trailing_game_id(Path::new("/data/library/textures/LG4P76"), &g),
            PathBuf::from("/data/library/textures/LG4P76")
        );
    }

    #[test]
    fn heal_deletes_duplicates_moves_real_content_and_drops_the_nested_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("GN7E69");
        // Byte-identical at both levels: the nested one is pure waste.
        write(&root.join("dup.png"), b"the same portrait");
        write(&root.join("GN7E69").join("dup.png"), b"the same portrait");
        // Nowhere else on disk: this is the one a prune would have lost.
        write(&root.join("GN7E69").join("only-here.png"), b"a portrait that ships nowhere");
        // A subdirectory inside the nested folder is folded too.
        write(&root.join("GN7E69").join("sub").join("deep.png"), b"deep");

        let healed = heal_nested_game_folder(&root).unwrap();

        assert_eq!(healed.duplicates, 1);
        assert_eq!(healed.superseded, Vec::<String>::new());
        let mut moved = healed.moved.clone();
        moved.sort();
        assert_eq!(moved, vec!["only-here.png".to_string(), "sub/deep.png".to_string()]);
        assert!(!root.join("GN7E69").exists(), "the nested folder is gone");
        assert_eq!(std::fs::read(root.join("dup.png")).unwrap(), b"the same portrait");
        assert_eq!(
            std::fs::read(root.join("only-here.png")).unwrap(),
            b"a portrait that ships nowhere"
        );
        assert_eq!(std::fs::read(root.join("sub").join("deep.png")).unwrap(), b"deep");
    }

    #[test]
    fn heal_keeps_the_right_level_file_when_the_nested_copy_differs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("GN7E69");
        // The right-level file is what a manifest lists, so it wins. The
        // nested one is a stale Builder write no manifest knows about, and
        // leaving it is what makes a later update show the OLD face.
        write(&root.join("face.png"), b"the new face, from the manifest");
        write(&root.join("GN7E69").join("face.png"), b"the old face");

        let healed = heal_nested_game_folder(&root).unwrap();

        assert_eq!(healed.duplicates, 0);
        assert_eq!(healed.superseded, vec!["face.png".to_string()]);
        assert!(healed.moved.is_empty());
        assert_eq!(
            std::fs::read(root.join("face.png")).unwrap(),
            b"the new face, from the manifest"
        );
        assert!(!root.join("GN7E69").exists());
    }

    /// The repair writes into the composed tree, so it must happen only after
    /// the tree has been shown to be one the launcher composed. Placed before
    /// that check, as it first was, it deleted and moved files inside a folder
    /// the very next line refuses to touch: the player's own.
    #[test]
    fn a_texture_folder_the_launcher_did_not_compose_is_never_repaired() {
        let p = Platform::detect();
        let fx = fixture();
        // No marker: this folder is somebody's own. Give it the same nested
        // game-id folder the repair exists for, holding a file that would be
        // deleted as a duplicate if the repair ever ran here.
        write(&fx.composed.join("theirs.dds"), b"their own texture");
        write(
            &fx.composed.join("GN7E69").join("theirs.dds"),
            b"their own texture",
        );

        let r = refusal(&p, &fx.request(Vec::new()));

        assert!(matches!(r, ComposeRefusal::ForeignComposedTree { .. }));
        assert!(
            fx.composed.join("GN7E69").join("theirs.dds").is_file(),
            "the repair reached into a folder the launcher did not compose"
        );
    }

    #[test]
    fn heal_does_nothing_to_a_correctly_laid_out_pack() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("GN7E69");
        lay_pristine(&root);
        let before = walk(&root).unwrap().files;

        let healed = heal_nested_game_folder(&root).unwrap();

        assert!(healed.is_empty());
        assert_eq!(walk(&root).unwrap().files, before);
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    /// A pristine tree shaped like the real one: DDS at the root and in a
    /// subdirectory.
    fn lay_pristine(root: &Path) {
        write(&root.join(BALL), b"the original football");
        write(&root.join("sub").join("tex1_other.dds"), b"another texture");
    }

    /// Extract a package into `mods/packages/<id>/<version>/files/` and return
    /// its manifest, with hashes and byte lengths taken from what was written.
    fn lay_package(
        packages_root: &Path,
        id: &str,
        group: Option<&str>,
        files: &[(&str, &[u8])],
    ) -> ModPackage {
        let id = ModId::parse(id).unwrap();
        let version = PackageVersion::parse("1.0.0").unwrap();
        let mut mod_files = Vec::new();
        let mut targets: Vec<GameId> = Vec::new();

        for (relative, bytes) in files {
            let path = PayloadPath::parse(relative).unwrap();
            if !targets.contains(path.game_id()) {
                targets.push(path.game_id().clone());
            }
            let mut dest = packages_root
                .join(id.as_str())
                .join(version.as_str())
                .join(PAYLOAD_SUBDIR);
            for segment in path.as_str().split('/') {
                dest = dest.join(segment);
            }
            write(&dest, bytes);
            mod_files.push(ModFile {
                path,
                sha256: Sha256Hex::from_bytes(bytes),
                bytes: bytes.len() as u64,
            });
        }

        ModPackage {
            schema: MOD_SCHEMA.to_string(),
            magic: MOD_MAGIC.to_string(),
            id,
            name: "Test Package".to_string(),
            version,
            targets,
            conflict_group: group.map(str::to_string),
            files: mod_files,
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        pristine: PathBuf,
        composed: PathBuf,
        packages: PathBuf,
        run: PathBuf,
    }

    impl Fixture {
        /// Where the activation list lives for this fixture.
        fn activation(&self) -> PathBuf {
            self.packages
                .parent()
                .expect("packages always has a parent")
                .join("activation.json")
        }
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let pristine = dir.path().join("library").join("textures").join("GN7E69");
        let composed = dir.path().join("mods").join("active").join("GN7E69");
        let packages = dir.path().join("mods").join("packages");
        let run = dir.path().join("cache");
        lay_pristine(&pristine);
        std::fs::create_dir_all(&packages).unwrap();
        Fixture {
            _dir: dir,
            pristine,
            composed,
            packages,
            run,
        }
    }

    impl Fixture {
        fn request(&self, packages: Vec<ModPackage>) -> ComposeRequest {
            ComposeRequest {
                game: game(),
                pristine_root: self.pristine.clone(),
                base_root: None,
                composed_root: self.composed.clone(),
                packages_root: self.packages.clone(),
                packages,
                pristine_identity: None,
                payload_fallback: None,
            }
        }
    }

    fn paths_under(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        fn go(dir: &Path, root: &Path, out: &mut Vec<String>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        go(&p, root, out);
                    } else {
                        out.push(p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        go(root, root, &mut out);
        out.sort();
        out
    }

    /// A build that fails part way leaves the live tree exactly as it was:
    /// the new tree is built beside it and swapped in only when whole, so
    /// the game is never pointed at a cleared or half-built folder.
    #[test]
    fn a_failed_build_leaves_the_previous_tree_in_place() {
        let p = Platform::detect();
        let fx = fixture();
        let req = fx.request(vec![]);
        let plan = planned(&p, &req);
        let guard = lock(&p, &fx);
        Composer::apply(&p, &guard, &plan).unwrap();
        let before = paths_under(&fx.composed);
        assert!(before.len() > 1);

        // a source that vanishes between planning and building
        let plan2 = planned(&p, &req);
        let victim = match &plan2.entries[0].source {
            EntrySource::Pristine { path, .. } => path.clone(),
            EntrySource::Overlay { path, .. } => path.clone(),
        };
        let bytes = std::fs::read(&victim).unwrap();
        std::fs::remove_file(&victim).unwrap();
        Composer::apply(&p, &guard, &plan2).expect_err("a missing source fails the build");
        assert_eq!(paths_under(&fx.composed), before, "the live tree is untouched by a build that failed");
        assert!(Composer::verify(&plan).unwrap().matches_the_plan());

        // the next build discards the leftover and succeeds once the source is back
        std::fs::write(&victim, bytes).unwrap();
        Composer::apply(&p, &guard, &planned(&p, &req)).unwrap();
        assert_eq!(paths_under(&fx.composed), before);
        assert!(!beside(&fx.composed, ".building").exists(), "no leftover build folder");
        assert!(!beside(&fx.composed, ".previous").exists(), "no leftover previous folder");
    }

    fn planned(platform: &Platform, req: &ComposeRequest) -> CompositionPlan {
        match Composer::plan(platform, req).expect("planning is not fallible here") {
            Composition::Planned(plan) => plan,
            Composition::Refused(r) => panic!("unexpected refusal: {}", r.message()),
        }
    }

    fn refusal(platform: &Platform, req: &ComposeRequest) -> ComposeRefusal {
        match Composer::plan(platform, req).expect("planning is not fallible here") {
            Composition::Refused(r) => r,
            Composition::Planned(_) => panic!("expected a refusal"),
        }
    }

    /// A supervisor holding a real lock, which `apply` requires as proof that
    /// no second launcher is composing the same tree.
    fn lock(platform: &Platform, fx: &Fixture) -> Supervisor {
        match Supervisor::acquire_at(platform, &fx.run, SystemClock).unwrap() {
            Startup::Owner(s) => *s,
            Startup::AlreadyRunning { .. } => panic!("nothing else should hold this lock"),
        }
    }

    /// The rule the whole module exists for. JSGME moved the original aside and
    /// then the updater deleted the folder holding it. Here the original is
    /// still exactly where it was, byte for byte, after a mod is applied.
    #[test]
    fn applying_a_mod_never_writes_into_the_pristine_tree() {
        let p = Platform::detect();
        let fx = fixture();
        let before: Vec<(String, Vec<u8>)> = PristineTree::open(&fx.pristine)
            .unwrap()
            .list()
            .unwrap()
            .files
            .iter()
            .map(|f| {
                (
                    f.relative.clone(),
                    std::fs::read(join_relative(&fx.pristine, &f.relative)).unwrap(),
                )
            })
            .collect();

        let package = lay_package(
            &fx.packages,
            "reborn.ball.purple",
            Some("ball"),
            &[(&format!("GN7E69/{BALL}"), b"the purple football")],
        );
        let req = fx.request(vec![package]);
        let plan = planned(&p, &req);
        let guard = lock(&p, &fx);
        Composer::apply(&p, &guard, &plan).unwrap();

        let after: Vec<(String, Vec<u8>)> = PristineTree::open(&fx.pristine)
            .unwrap()
            .list()
            .unwrap()
            .files
            .iter()
            .map(|f| {
                (
                    f.relative.clone(),
                    std::fs::read(join_relative(&fx.pristine, &f.relative)).unwrap(),
                )
            })
            .collect();
        assert_eq!(
            before, after,
            "the pristine tree changed. It is the only copy of the originals"
        );

        assert_eq!(
            std::fs::read(fx.composed.join(BALL)).unwrap(),
            b"the purple football",
            "the composed tree must carry the mod file"
        );
    }

    /// Un-applying is a rebuild, so it works with the mod payload gone. This is
    /// the exact situation that leaves a JSGME install unrecoverable: the folder
    /// holding the restore data is deleted by the updater.
    #[test]
    fn unapplying_works_after_the_mod_payload_is_deleted() {
        let p = Platform::detect();
        let fx = fixture();
        let package = lay_package(
            &fx.packages,
            "reborn.ball.pink",
            Some("ball"),
            &[(&format!("GN7E69/{BALL}"), b"the pink football")],
        );
        let guard = lock(&p, &fx);

        let with_mod = fx.request(vec![package]);
        let plan = planned(&p, &with_mod);
        Composer::apply(&p, &guard, &plan).unwrap();
        assert_eq!(
            std::fs::read(fx.composed.join(BALL)).unwrap(),
            b"the pink football"
        );

        // The updater deletes the whole mods folder, payload included.
        std::fs::remove_dir_all(&fx.packages).unwrap();

        let without = fx.request(Vec::new());
        let plan = planned(&p, &without);
        let report = Composer::apply(&p, &guard, &plan).unwrap();

        assert_eq!(report.overlaid, 0);
        assert_eq!(
            std::fs::read(fx.composed.join(BALL)).unwrap(),
            b"the original football",
            "un-applying must restore the original with no backup folder to lose"
        );
        assert!(
            Composer::verify(&plan).unwrap().matches_the_plan(),
            "the rebuilt tree must match its own plan"
        );
    }

    /// Two mods writing one path is the real shape of the 2.1 catalog: all seven
    /// football mods write the same single file. Composing both and letting the
    /// activation order decide would pick for the user, and the pick would
    /// change silently if the order changed.
    #[test]
    fn two_mods_writing_one_path_are_refused_and_both_are_named() {
        let p = Platform::detect();
        let fx = fixture();
        let purple = lay_package(
            &fx.packages,
            "reborn.ball.purple",
            Some("ball"),
            &[(&format!("GN7E69/{BALL}"), b"purple")],
        );
        let pink = lay_package(
            &fx.packages,
            "reborn.ball.pink",
            Some("ball"),
            &[(&format!("GN7E69/{BALL}"), b"pink!!")],
        );

        let req = fx.request(vec![purple, pink]);
        let ComposeRefusal::HardConflicts { conflicts } = refusal(&p, &req) else {
            panic!("two mods on one path must refuse the plan");
        };
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].is_hard());
        let text = conflicts[0].message();
        assert!(text.contains("reborn.ball.purple"), "{text}");
        assert!(text.contains("reborn.ball.pink"), "{text}");
        assert!(
            text.contains(BALL),
            "the contested path must be named: {text}"
        );
        assert!(
            !fx.composed.exists(),
            "a refused plan must not have built anything"
        );
    }

    /// Overlapping paths are computed from the data and outrank any declared
    /// group, so a conflicting pair cannot be talked into composing by its
    /// manifests.
    #[test]
    fn a_path_overlap_outranks_a_declared_group() {
        let fx = fixture();
        let a = lay_package(
            &fx.packages,
            "reborn.prompts.ps",
            Some("prompts"),
            &[("GN7E69/btn_a.png", b"ps")],
        );
        let b = lay_package(
            &fx.packages,
            "reborn.prompts.xbox",
            Some("prompts"),
            &[("GN7E69/btn_a.png", b"xb")],
        );
        let conflicts = detect_conflicts(&[a, b], &game(), None);
        assert_eq!(
            conflicts.len(),
            1,
            "one pair must produce one conflict, not two"
        );
        assert!(matches!(conflicts[0].kind, ConflictKind::SamePath { .. }));
    }

    /// A declared group with no overlapping path is advisory. It must be
    /// reported, and it must not stop a composition that is unambiguous.
    #[test]
    fn a_declared_group_conflict_is_reported_but_still_composes() {
        let p = Platform::detect();
        let fx = fixture();
        let a = lay_package(
            &fx.packages,
            "reborn.jersey.saints-alternate",
            Some("jersey"),
            &[("GN7E69/saints.dds", b"saints")],
        );
        let b = lay_package(
            &fx.packages,
            "reborn.jersey.patriots-throwback",
            Some("jersey"),
            &[("GN7E69/patriots.dds", b"patriots")],
        );
        let req = fx.request(vec![a, b]);
        let plan = planned(&p, &req);

        assert_eq!(plan.soft_conflicts.len(), 1, "the group must be reported");
        assert!(!plan.soft_conflicts[0].is_hard());
        assert_eq!(plan.overlay_count(), 2, "both mods must compose");
    }

    /// Composing into the pristine tree is the destructive model this module
    /// replaces, so it is refused rather than merely discouraged.
    #[test]
    fn composing_into_the_pristine_tree_is_refused() {
        let p = Platform::detect();
        let fx = fixture();

        let mut req = fx.request(Vec::new());
        req.composed_root = fx.pristine.clone();
        assert!(matches!(
            refusal(&p, &req),
            ComposeRefusal::ComposedInsidePristine { .. }
        ));

        let mut req = fx.request(Vec::new());
        req.composed_root = fx.pristine.join("composed");
        let r = refusal(&p, &req);
        assert!(matches!(r, ComposeRefusal::ComposedInsidePristine { .. }));
        assert!(
            r.message().contains("Load/Textures"),
            "the refusal must say where to point it: {}",
            r.message()
        );

        // And the other direction: rebuilding deletes the composed tree, so the
        // pristine tree must not be inside it.
        let mut req = fx.request(Vec::new());
        req.composed_root = fx.pristine.parent().unwrap().to_path_buf();
        assert!(matches!(
            refusal(&p, &req),
            ComposeRefusal::PristineInsideComposed { .. }
        ));
    }

    /// A refused plan must not have deleted the directory it was pointed at,
    /// and `apply` re-checks rather than trusting a plan it was handed.
    #[test]
    fn apply_refuses_a_plan_whose_trees_overlap() {
        let p = Platform::detect();
        let fx = fixture();
        let mut plan = planned(&p, &fx.request(Vec::new()));
        plan.composed_root = fx.pristine.clone();

        let guard = lock(&p, &fx);
        let err = Composer::apply(&p, &guard, &plan)
            .expect_err("apply must not trust a plan that would write into the pristine tree");
        assert!(err.to_string().contains("only copy"), "{err}");
        assert!(
            fx.pristine.join(BALL).is_file(),
            "the pristine tree must be untouched"
        );
    }

    /// The launcher never deletes a directory it cannot prove it created. The
    /// 2.1 uninstaller ran DelTree over a folder holding memory cards, and this
    /// is the check that stops the same shape of bug appearing here.
    #[test]
    fn a_directory_the_launcher_did_not_compose_is_never_deleted() {
        let p = Platform::detect();
        let fx = fixture();
        let precious = fx.composed.join("memory-card.raw");
        write(&precious, b"somebody's franchise");

        let r = refusal(&p, &fx.request(Vec::new()));
        assert!(matches!(r, ComposeRefusal::ForeignComposedTree { .. }));
        assert!(
            r.message().contains("Move the directory aside"),
            "no way forward given: {}",
            r.message()
        );
        assert_eq!(
            std::fs::read(&precious).unwrap(),
            b"somebody's franchise",
            "a directory with no marker must survive"
        );

        let guard = lock(&p, &fx);
        assert!(Composer::discard(&guard, &fx.composed).is_err());
        assert!(precious.is_file());
    }

    /// An interrupted composition leaves a marker with no record. That tree is
    /// deletable, because the launcher made it, and is never trusted, because it
    /// was never finished.
    #[test]
    fn an_interrupted_composition_is_deletable_and_never_trusted() {
        let p = Platform::detect();
        let fx = fixture();
        std::fs::create_dir_all(&fx.composed).unwrap();
        write_marker_durably(
            &fx.composed,
            &ComposedMarker {
                schema: COMPOSED_SCHEMA.to_string(),
                game: game(),
                record: None,
            },
        )
        .unwrap();
        write(&fx.composed.join(BALL), b"half composed");

        let plan = planned(&p, &fx.request(Vec::new()));
        let verdict = Composer::verify(&plan).unwrap();
        assert!(
            matches!(
                verdict.discrepancies.first(),
                Some(Discrepancy::IncompleteComposition { .. })
            ),
            "a partial tree must not verify: {:?}",
            verdict.discrepancies
        );
        assert!(verdict.needs_rebuild());

        let guard = lock(&p, &fx);
        let report = Composer::apply(&p, &guard, &plan).unwrap();
        assert_eq!(report.entries(), plan.entries.len() as u64);
        assert!(Composer::verify(&plan).unwrap().matches_the_plan());
    }

    /// Verification exists to catch the case where Dolphin would load something
    /// the user did not choose.
    #[test]
    fn verification_catches_an_overlay_whose_content_changed() {
        let p = Platform::detect();
        let fx = fixture();
        let package = lay_package(
            &fx.packages,
            "reborn.ball.camo",
            None,
            &[(&format!("GN7E69/{BALL}"), b"camo football")],
        );
        let req = fx.request(vec![package]);
        let plan = planned(&p, &req);
        let guard = lock(&p, &fx);
        Composer::apply(&p, &guard, &plan).unwrap();
        assert!(Composer::verify(&plan).unwrap().matches_the_plan());

        // Something replaced the composed file. On a hardlinked tree this has
        // to be a replacement rather than an in place write, or the pristine
        // file would change with it.
        let composed_file = fx.composed.join(BALL);
        std::fs::remove_file(&composed_file).unwrap();
        write(&composed_file, b"not the camo football");

        let verdict = Composer::verify(&plan).unwrap();
        assert!(verdict.needs_rebuild());
        assert!(
            verdict
                .discrepancies
                .iter()
                .any(|d| matches!(d, Discrepancy::OverlayContentDiffers { .. })),
            "{:?}",
            verdict.discrepancies
        );
    }

    /// A tree composed from a different set of mods must not pass, or turning a
    /// mod off would leave it loaded with the launcher reporting success.
    #[test]
    fn verification_catches_a_tree_composed_from_a_different_activation() {
        let p = Platform::detect();
        let fx = fixture();
        let package = lay_package(
            &fx.packages,
            "reborn.ball.green",
            None,
            &[(&format!("GN7E69/{BALL}"), b"green football")],
        );
        let with_mod = planned(&p, &fx.request(vec![package]));
        let guard = lock(&p, &fx);
        Composer::apply(&p, &guard, &with_mod).unwrap();

        let without = planned(&p, &fx.request(Vec::new()));
        let verdict = Composer::verify(&without).unwrap();
        assert!(
            verdict
                .discrepancies
                .iter()
                .any(|d| matches!(d, Discrepancy::ActivationDiffers { .. })),
            "{:?}",
            verdict.discrepancies
        );
        assert!(verdict.needs_rebuild());
    }

    /// A tree nobody composed must not be reported as matching, whatever is in
    /// it. Otherwise a stray folder at the mod load path would be trusted.
    #[test]
    fn verification_refuses_a_tree_with_no_record() {
        let p = Platform::detect();
        let fx = fixture();
        let plan = planned(&p, &fx.request(Vec::new()));
        std::fs::create_dir_all(&fx.composed).unwrap();

        let verdict = Composer::verify(&plan).unwrap();
        assert!(matches!(
            verdict.discrepancies.first(),
            Some(Discrepancy::NotComposedByTheLauncher { .. })
        ));
    }

    /// A missing pristine file is caught by the count. This is the one thing the
    /// hash check cannot see, because no trustworthy expected hash for the
    /// pristine side is passed to this module.
    #[test]
    fn verification_catches_a_composed_file_that_went_missing() {
        let p = Platform::detect();
        let fx = fixture();
        let plan = planned(&p, &fx.request(Vec::new()));
        let guard = lock(&p, &fx);
        Composer::apply(&p, &guard, &plan).unwrap();

        std::fs::remove_file(fx.composed.join("sub").join("tex1_other.dds")).unwrap();

        let verdict = Composer::verify(&plan).unwrap();
        assert!(
            verdict
                .discrepancies
                .iter()
                .any(|d| matches!(d, Discrepancy::EntryCountDiffers { .. })),
            "{:?}",
            verdict.discrepancies
        );
    }

    /// Activation order is recorded and displayed and must never decide content.
    /// Two orders of the same non-conflicting mods must produce the same tree.
    #[test]
    fn activation_order_does_not_change_the_composed_tree() {
        let p = Platform::detect();
        let fx = fixture();
        let a = lay_package(
            &fx.packages,
            "reborn.jersey.saints-alternate",
            None,
            &[("GN7E69/saints.dds", b"saints")],
        );
        let b = lay_package(
            &fx.packages,
            "reborn.prompts.ps",
            None,
            &[("GN7E69/btn_a.png", b"ps prompts")],
        );

        let guard = lock(&p, &fx);
        let mut trees = Vec::new();
        for packages in [vec![a.clone(), b.clone()], vec![b, a]] {
            let mut req = fx.request(packages);
            req.composed_root = fx.composed.with_extension(format!("{}", trees.len()));
            let plan = planned(&p, &req);
            Composer::apply(&p, &guard, &plan).unwrap();

            let mut contents: Vec<(String, Vec<u8>)> = walk(&req.composed_root)
                .unwrap()
                .files
                .into_iter()
                .filter(|f| f.relative != COMPOSED_MARKER)
                .map(|f| {
                    let bytes =
                        std::fs::read(join_relative(&req.composed_root, &f.relative)).unwrap();
                    (f.relative, bytes)
                })
                .collect();
            contents.sort();
            trees.push(contents);
        }
        assert_eq!(
            trees[0], trees[1],
            "the composed tree must not depend on activation order"
        );
    }

    /// Every file is either linked or copied, and every one arrives with the
    /// right content. Which of the two happened depends on the filesystem, and
    /// on exFAT and FAT32 it is always a copy.
    #[test]
    fn every_planned_entry_is_either_linked_or_copied() {
        let p = Platform::detect();
        let fx = fixture();
        let package = lay_package(
            &fx.packages,
            "reborn.prompts.xbox",
            None,
            &[("GN7E69/btn_a.png", b"xbox prompt")],
        );
        let req = fx.request(vec![package]);
        let plan = planned(&p, &req);
        let guard = lock(&p, &fx);
        let report = Composer::apply(&p, &guard, &plan).unwrap();

        assert_eq!(
            report.entries(),
            plan.entries.len() as u64,
            "some entry was neither linked nor copied"
        );
        assert_eq!(report.overlaid, 1);
        assert_eq!(
            std::fs::read(fx.composed.join("btn_a.png")).unwrap(),
            b"xbox prompt"
        );
        assert_eq!(
            std::fs::read(fx.composed.join(BALL)).unwrap(),
            b"the original football"
        );
        if report.copied > 0 {
            assert!(
                report.bytes_copied > 0,
                "a copy that wrote no bytes did not happen"
            );
        }
    }

    /// A payload path is the only thing that decides where a mod writes, so it
    /// is the traversal defense. Every one of these produces a file outside the
    /// composed tree if accepted.
    #[test]
    fn payload_paths_that_could_escape_the_composed_tree_are_rejected() {
        for bad in [
            "",
            "GN7E69",
            "ball.dds",
            "/GN7E69/ball.dds",
            "GN7E69/../../ball.dds",
            "../GN7E69/ball.dds",
            "GN7E69/./ball.dds",
            "GN7E69//ball.dds",
            "GN7E69\\ball.dds",
            "C:/GN7E69/ball.dds",
            "GN7E69/ball.dds:stream",
            "GN7E69/ball.dds/",
            "GN7E69/ball.dds ",
            "GN7E69/ball.dds.",
            "GN7E69/ball.exe",
            "GN7E69/ball",
            "../../etc/passwd",
        ] {
            assert!(
                PayloadPath::parse(bad).is_err(),
                "{bad:?} must not be accepted as a payload path"
            );
        }

        let good = PayloadPath::parse("GN7E69/sub/tex1_a.DDS").unwrap();
        assert_eq!(good.game_id().as_str(), "GN7E69");
        assert_eq!(
            good.tail(),
            "sub/tex1_a.DDS",
            "the tail is what the composed tree sees"
        );
    }

    /// A game id is joined onto both roots, so `..` there is the same escape by
    /// another route.
    #[test]
    fn game_ids_and_package_ids_cannot_be_path_tricks() {
        for bad in ["", "..", ".", "GN7E69/x", "GN7E69\\x", "a:b", "GN 7E69"] {
            assert!(GameId::parse(bad).is_err(), "{bad:?} must not be a game id");
        }
        for bad in ["", "..", "a/b", "a\\b", ".hidden", "UPPER", "trailing-"] {
            assert!(ModId::parse(bad).is_err(), "{bad:?} must not be a mod id");
        }
        for bad in ["", "../1.0.0", "1/0", "1\\0", ".1"] {
            assert!(
                PackageVersion::parse(bad).is_err(),
                "{bad:?} must not be a version"
            );
        }
        assert!(ModId::parse("reborn.ball.purple").is_ok());
        assert!(PackageVersion::parse("1.0.0").is_ok());
    }

    /// Two entries differing only by case are two files on ext4 and one file on
    /// NTFS and APFS. Composing them would produce a tree whose file count
    /// depends on the machine, so verification would fail forever on one of
    /// them.
    #[test]
    fn entries_differing_only_by_case_are_refused() {
        let p = Platform::detect();
        let fx = fixture();
        let shouty = BALL.to_uppercase();
        let package = lay_package(
            &fx.packages,
            "reborn.ball.aqua",
            None,
            &[(&format!("GN7E69/{shouty}"), b"aqua football")],
        );
        let req = fx.request(vec![package]);
        assert!(
            matches!(refusal(&p, &req), ComposeRefusal::CaseCollision { .. }),
            "a case-only collision must be refused, not composed"
        );
    }

    /// A declared byte length that does not match the payload means the package
    /// was recorded from a directory listing, where the size is rounded to the
    /// allocation unit, or the payload is truncated. Both make every later
    /// verification of a correct install fail.
    #[test]
    fn a_declared_size_that_does_not_match_the_payload_is_refused() {
        let p = Platform::detect();
        let fx = fixture();
        let mut package = lay_package(
            &fx.packages,
            "reborn.ball.blue",
            None,
            &[(&format!("GN7E69/{BALL}"), b"blue football")],
        );
        package.files[0].bytes = 4096;

        let req = fx.request(vec![package]);
        let r = refusal(&p, &req);
        assert!(matches!(r, ComposeRefusal::PayloadSizeDiffers { .. }));
        assert!(r.message().contains("allocation unit"), "{}", r.message());
    }

    /// A manifest that names a file the payload does not have must refuse before
    /// anything is written, not fail half way through composing.
    #[test]
    fn a_missing_payload_file_is_refused_before_anything_is_written() {
        let p = Platform::detect();
        let fx = fixture();
        let package = lay_package(
            &fx.packages,
            "reborn.ball.panda",
            None,
            &[(&format!("GN7E69/{BALL}"), b"panda football")],
        );
        std::fs::remove_dir_all(fx.packages.join("reborn.ball.panda")).unwrap();

        let req = fx.request(vec![package]);
        assert!(matches!(
            refusal(&p, &req),
            ComposeRefusal::MissingPayload { .. }
        ));
        assert!(!fx.composed.exists());
    }

    /// A package with no payload for this build is skipped and named, because
    /// the activation list covers every build at once. A mod switched on that
    /// silently does nothing is indistinguishable from a broken one.
    #[test]
    fn a_package_for_another_build_is_skipped_and_reported() {
        let p = Platform::detect();
        let fx = fixture();
        let other = lay_package(
            &fx.packages,
            "reborn.ball.other",
            None,
            &[("FR4N31/ball.dds", b"for the online build")],
        );
        let req = fx.request(vec![other]);
        let plan = planned(&p, &req);

        assert_eq!(plan.overlay_count(), 0);
        assert_eq!(plan.not_targeted.len(), 1);
        assert!(
            plan.activation.is_empty(),
            "a package that contributes nothing must not be recorded as active \
             for this tree"
        );
    }

    // --- the league build: its own pristine tree, the retail mod list (spec 2026-10-08 v2)

    fn league() -> GameId {
        GameId::parse(crate::league_edition::LEAGUE_GAME_ID).unwrap()
    }

    /// A platform whose data root is `root`, so data-root lookups stay in a temp folder.
    fn platform_at(root: &Path) -> Platform {
        struct TempPaths(PathBuf);
        impl crate::platform::Paths for TempPaths {
            fn install_root(&self) -> crate::platform::Result<PathBuf> { Ok(self.0.join("install")) }
            fn data_root(&self) -> crate::platform::Result<PathBuf> { Ok(self.0.clone()) }
            fn cache_root(&self) -> crate::platform::Result<PathBuf> { Ok(self.0.join("cache")) }
            fn exe_suffix(&self) -> &'static str { ".exe" }
        }
        let mut p = Platform::detect();
        p.paths = Box::new(TempPaths(root.to_path_buf()));
        p
    }

    #[test]
    fn only_the_league_build_borrows_retail_payloads() {
        assert_eq!(payload_fallback_for(&league()), Some(GameId::parse("GN7E69").unwrap()));
        assert_eq!(payload_fallback_for(&GameId::parse("GN7E69").unwrap()), None);
        assert_eq!(payload_fallback_for(&GameId::parse("FR4N31").unwrap()), None);
    }

    /// One shared mod list (spec v2): a mod switched on for Reborn is in the league game too.
    #[test]
    fn a_retail_mod_is_composed_into_the_league_tree() {
        let p = Platform::detect();
        let fx = fixture();
        let pkg = lay_package(&fx.packages, "reborn.ball.purple", None, &[("GN7E69/ball.dds", b"purple")]);
        let mut req = fx.request(vec![pkg]);
        req.game = league();
        req.payload_fallback = payload_fallback_for(&req.game);
        let plan = planned(&p, &req);
        assert_eq!(plan.overlay_count(), 1);
        assert!(plan.not_targeted.is_empty());
    }

    #[test]
    fn a_league_payload_beats_the_retail_one() {
        let both = lay_package(&fixture().packages, "reborn.ball.both", None, &[
            ("GN7E69/ball.dds", b"retail"),
            (&format!("{}/ball.dds", crate::league_edition::LEAGUE_GAME_ID), b"league"),
        ]);
        let gn7 = GameId::parse("GN7E69").unwrap();
        assert_eq!(both.payload_game(&league(), Some(&gn7)), Some(league()));
        let retail_only = lay_package(&fixture().packages, "reborn.ball.retail", None, &[("GN7E69/ball.dds", b"retail")]);
        assert_eq!(retail_only.payload_game(&league(), Some(&gn7)), Some(gn7.clone()));
        assert_eq!(retail_only.payload_game(&league(), None), None, "no fallback, no payload");
        let online_only = lay_package(&fixture().packages, "reborn.ball.online", None, &[("FR4N31/ball.dds", b"online")]);
        assert_eq!(online_only.payload_game(&league(), Some(&gn7)), None);
    }

    /// The league tree composes the league payload's BYTES, not the retail one's, when a package ships both.
    #[test]
    fn a_package_with_a_league_payload_composes_that_payload() {
        let p = Platform::detect();
        let fx = fixture();
        let pkg = lay_package(&fx.packages, "reborn.ball.both", None, &[
            ("GN7E69/ball.dds", b"retail!"),
            (&format!("{}/ball.dds", crate::league_edition::LEAGUE_GAME_ID), b"league"),
        ]);
        let mut req = fx.request(vec![pkg]);
        req.game = league();
        req.payload_fallback = payload_fallback_for(&req.game);
        let plan = planned(&p, &req);
        assert_eq!(plan.overlay_count(), 1);
        let ball = plan.entries.iter().find(|e| e.relative == "ball.dds").unwrap();
        assert_eq!(ball.bytes, 6, "the league payload, not the 7-byte retail one");
    }

    /// Two mods that write the same tail clash in the league tree even when one borrows the retail payload.
    #[test]
    fn a_borrowed_payload_still_conflicts_in_the_league_tree() {
        let fx = fixture();
        let a = lay_package(&fx.packages, "reborn.ball.a", None, &[("GN7E69/ball.dds", b"a")]);
        let b = lay_package(&fx.packages, "reborn.ball.b", None, &[
            (&format!("{}/ball.dds", crate::league_edition::LEAGUE_GAME_ID), b"b"),
        ]);
        let gn7 = GameId::parse("GN7E69").unwrap();
        let pkgs = [a, b];
        assert_eq!(detect_conflicts(&pkgs, &league(), Some(&gn7)).len(), 1);
        assert!(detect_conflicts(&pkgs, &league(), None).is_empty(), "without the fallback only b is in the tree");
        assert!(detect_conflicts(&pkgs, &gn7, None).is_empty(), "the retail tree has only a");
    }

    #[test]
    fn retail_composition_ignores_the_fallback() {
        let p = Platform::detect();
        let fx = fixture();
        let other = lay_package(&fx.packages, "reborn.ball.other", None, &[("FR4N31/ball.dds", b"online")]);
        let req = fx.request(vec![other]);
        assert_eq!(req.payload_fallback, None);
        assert_eq!(planned(&p, &req).not_targeted.len(), 1, "a retail tree never borrows");
    }

    /// Composing retail after the league change produces exactly the tree it did before: the pristine set plus the
    /// retail payloads, and a league-only package is not in it.
    #[test]
    fn retail_composition_is_unchanged_by_league_packages() {
        let p = Platform::detect();
        let fx = fixture();
        let retail = lay_package(&fx.packages, "reborn.ball.retail", None, &[("GN7E69/ball.dds", b"retail")]);
        let league_only = lay_package(&fx.packages, "reborn.ball.league", None, &[
            (&format!("{}/kick.dds", crate::league_edition::LEAGUE_GAME_ID), b"league"),
        ]);
        let before = planned(&p, &fx.request(vec![retail.clone()]));
        let mut req = fx.request(vec![retail, league_only.clone()]);
        req.payload_fallback = payload_fallback_for(&req.game);
        assert_eq!(req.payload_fallback, None);
        let after = planned(&p, &req);
        assert_eq!(after.entries, before.entries, "byte-for-byte the same retail tree");
        assert_eq!(after.not_targeted, vec![league_only.id]);
    }

    /// The league build's own textures are the ones in its component; the retail pack is its base, not its tree.
    #[test]
    fn the_league_pristine_tree_is_the_league_components() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let p = platform_at(root);
        assert_eq!(pristine_root(&p, &league()).unwrap(), crate::league_edition::league_textures_root(root));
    }

    /// A league update swaps the league folder, so its identity is the league-game pointer, never the textures one.
    #[test]
    fn the_league_pristine_identity_is_the_league_game_pointer() {
        use crate::content::{ContentLayout, InstalledVersion, Pointer, POINTER_SCHEMA};
        let dir = tempfile::tempdir().unwrap();
        let p = platform_at(dir.path());
        let layout = ContentLayout::new(dir.path());
        let save = |component: &str, version: &str, sequence| {
            Pointer {
                schema: POINTER_SCHEMA.to_string(),
                component: component.to_string(),
                current: InstalledVersion { version: version.to_string(), sequence },
                previous: None,
            }
            .save(&p, &layout.pointer_path(component))
            .unwrap();
        };
        save("textures", "1.3.4", 42);
        assert_eq!(pristine_identity(&p, &league()), None, "the retail textures pointer is not the league's");
        assert_eq!(pristine_identity(&p, &game()).as_deref(), Some("1.3.4#42"));
        save("league-game", "2026.10.09", 7);
        assert_eq!(
            pristine_identity(&p, &league()).as_deref(),
            Some("2026.10.09#7+textures 1.3.4#42"),
            "the league tree links into the retail pack too, so a retail update rebuilds it"
        );
        assert_eq!(pristine_identity(&p, &game()).as_deref(), Some("1.3.4#42"), "retail unchanged");
        save("textures", "1.3.5", 43);
        assert_eq!(pristine_identity(&p, &league()).as_deref(), Some("2026.10.09#7+textures 1.3.5#43"));
    }

    /// The league ships only what it changes (Adam 10-16): its tree is laid over the retail pack.
    #[test]
    fn the_league_tree_is_laid_over_the_retail_pack() {
        let dir = tempfile::tempdir().unwrap();
        let p = platform_at(dir.path());
        assert_eq!(
            base_root_for(&p, &league()).unwrap(),
            Some(pristine_root(&p, &game()).unwrap()),
            "the same retail tree the retail game composes from"
        );
        assert_eq!(base_root_for(&p, &game()).unwrap(), None);
        assert_eq!(base_root_for(&p, &GameId::parse("FR4N31").unwrap()).unwrap(), None);
    }

    /// The fixture's retail pack as the base, and a league tree holding one changed texture (the football, at
    /// another path) and one the retail pack does not have.
    fn league_request(fx: &Fixture, packages: Vec<ModPackage>) -> ComposeRequest {
        let own = fx.pristine.parent().unwrap().join(crate::league_edition::LEAGUE_GAME_ID);
        write(&own.join("jerseys").join(BALL.to_uppercase()), b"league ball");
        write(&own.join("tex1_league_logo.dds"), b"league logo");
        let mut req = fx.request(packages);
        req.game = league();
        req.payload_fallback = payload_fallback_for(&req.game);
        req.base_root = Some(fx.pristine.clone());
        req.pristine_root = own;
        req
    }

    #[test]
    fn a_league_texture_replaces_the_retail_one_by_name_wherever_it_sits() {
        let p = Platform::detect();
        let fx = fixture();
        let plan = planned(&p, &league_request(&fx, vec![]));
        let names: Vec<&str> = plan.entries.iter().map(|e| e.relative.as_str()).collect();
        assert_eq!(
            names,
            vec![format!("jerseys/{}", BALL.to_uppercase()).as_str(), "sub/tex1_other.dds", "tex1_league_logo.dds"],
            "the retail football is gone (Dolphin would keep one of two at random); the rest of retail stays"
        );
        assert_eq!(plan.overlay_count(), 0);
        assert_eq!(plan.base_root.as_deref(), Some(fx.pristine.as_path()));
    }

    /// "league textures win": a mod with a league texture's name is left out; a mod over a retail texture applies.
    #[test]
    fn a_league_texture_beats_a_mod_and_a_mod_still_beats_retail() {
        let p = Platform::detect();
        let fx = fixture();
        let ball = lay_package(&fx.packages, "reborn.ball.purple", None, &[(&format!("GN7E69/{BALL}"), b"purple")]);
        let other = lay_package(&fx.packages, "reborn.other.green", None, &[("GN7E69/sub/tex1_other.dds", b"green")]);
        let plan = planned(&p, &league_request(&fx, vec![ball, other]));
        assert_eq!(plan.overlays().iter().map(|o| o.relative.as_str()).collect::<Vec<_>>(), vec!["sub/tex1_other.dds"]);
        assert!(!plan.entries.iter().any(|e| e.relative == BALL), "neither the retail nor the mod football");
    }

    #[test]
    fn a_league_tree_composes_and_verifies_and_a_new_base_rebuilds_it() {
        let p = Platform::detect();
        let fx = fixture();
        let req = league_request(&fx, vec![]);
        let plan = planned(&p, &req);
        let guard = lock(&p, &fx);
        Composer::apply(&p, &guard, &plan).unwrap();
        assert!(Composer::verify(&plan).unwrap().matches_the_plan());
        assert_eq!(std::fs::read(fx.composed.join("sub").join("tex1_other.dds")).unwrap(), b"another texture");

        let elsewhere = fx.pristine.parent().unwrap().join("retail-copy");
        lay_pristine(&elsewhere);
        let mut moved = req.clone();
        moved.base_root = Some(elsewhere);
        let found = Composer::verify(&planned(&p, &moved)).unwrap();
        assert!(found.discrepancies.iter().any(|d| matches!(d, Discrepancy::BaseRootDiffers { .. })), "{}", found.report());
    }

    #[test]
    fn a_missing_retail_pack_refuses_the_league_tree() {
        let p = Platform::detect();
        let fx = fixture();
        let mut req = league_request(&fx, vec![]);
        req.base_root = Some(fx.pristine.parent().unwrap().join("absent"));
        assert!(matches!(refusal(&p, &req), ComposeRefusal::NoPristineTree { .. }));
    }

    #[test]
    fn a_composed_tree_inside_the_base_is_refused() {
        let p = Platform::detect();
        let fx = fixture();
        let mut req = league_request(&fx, vec![]);
        req.composed_root = fx.pristine.join("composed");
        assert!(matches!(refusal(&p, &req), ComposeRefusal::ComposedInsidePristine { .. }));
    }

    /// The activation list is the source of truth for a rebuild. A schema it
    /// does not understand must not be read as an empty list, or the rebuild
    /// turns every mod off and reports success.
    #[test]
    fn an_unknown_activation_schema_is_refused_rather_than_read_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ACTIVATION_FILE);

        assert!(
            Activation::load(&path).unwrap().active.is_empty(),
            "a missing list is genuinely no mods active"
        );

        std::fs::write(
            &path,
            br#"{"schema":"nsr.activation.v99","active":[{"id":"x","version":"1"}]}"#,
        )
        .unwrap();
        let err = Activation::load(&path).expect_err("an unknown schema must not be guessed at");
        assert!(err.to_string().contains("nsr.activation.v1"), "{err}");

        std::fs::write(&path, b"{ truncated").unwrap();
        let err = Activation::load(&path).expect_err("a truncated list must not read as empty");
        assert!(err.to_string().contains("no mods active"), "{err}");
    }

    /// One package id appears at most once. Two versions of one package active
    /// together would conflict with themselves on every path they carry.
    #[test]
    fn activation_holds_one_version_per_package_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ACTIVATION_FILE);
        let id = ModId::parse("reborn.ball.purple").unwrap();

        let mut activation = Activation::default();
        activation.activate(id.clone(), PackageVersion::parse("1.0.0").unwrap());
        activation.activate(id.clone(), PackageVersion::parse("1.1.0").unwrap());
        assert_eq!(activation.active.len(), 1);
        assert_eq!(activation.active[0].version.as_str(), "1.1.0");

        activation.save(&path).unwrap();
        assert_eq!(Activation::load(&path).unwrap(), activation);

        assert!(activation.deactivate(&id));
        assert!(!activation.deactivate(&id));
        assert!(!activation.is_active(&id));
    }

    /// The on-disk shape of the composed record is ours to keep readable. If a
    /// field is renamed, this fails and whoever renamed it has to think about
    /// every composed tree already on a user's disk.
    #[test]
    fn the_composed_marker_shape_is_pinned() {
        let marker = ComposedMarker {
            schema: COMPOSED_SCHEMA.to_string(),
            game: game(),
            record: None,
        };
        let json = serde_json::to_value(&marker).unwrap();
        assert_eq!(json["schema"], "nsr.composed.v1");
        assert_eq!(json["game"], "GN7E69");
        assert!(
            json.get("record").is_none(),
            "an unfinished composition must not carry a record"
        );

        let finished = ComposedMarker {
            record: Some(ComposedRecord {
                game: game(),
                pristine_root: PathBuf::from("pristine"),
                base_root: None,
                pristine_identity: None,
                activation: vec![ActivePackage {
                    id: ModId::parse("reborn.ball.purple").unwrap(),
                    version: PackageVersion::parse("1.0.0").unwrap(),
                }],
                total_entries: 3,
                pristine_entries: 2,
                overlays: vec![OverlayRecord {
                    relative: BALL.to_string(),
                    id: ModId::parse("reborn.ball.purple").unwrap(),
                    sha256: Sha256Hex::from_bytes(b"x"),
                }],
            }),
            ..marker
        };
        let json = serde_json::to_value(&finished).unwrap();
        assert_eq!(json["record"]["activation"][0]["id"], "reborn.ball.purple");
        assert_eq!(json["record"]["total_entries"], 3);
        assert_eq!(json["record"]["overlays"][0]["relative"], BALL);
    }

    /// A package manifest round trips through the file the launcher reads, and a
    /// schema this launcher does not compose is refused by name.
    #[test]
    fn a_package_manifest_round_trips_and_a_wrong_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let packages = dir.path().join("packages");
        let package = lay_package(
            &packages,
            "reborn.ball.purple",
            Some("ball"),
            &[(&format!("GN7E69/{BALL}"), b"purple football")],
        );
        let package_dir = packages.join("reborn.ball.purple").join("1.0.0");
        std::fs::write(
            package_dir.join(PACKAGE_MANIFEST),
            serde_json::to_vec_pretty(&package).unwrap(),
        )
        .unwrap();
        assert_eq!(ModPackage::load(&package_dir).unwrap(), package);

        let mut wrong = package.clone();
        wrong.schema = "nsr.mod.v2".to_string();
        std::fs::write(
            package_dir.join(PACKAGE_MANIFEST),
            serde_json::to_vec_pretty(&wrong).unwrap(),
        )
        .unwrap();
        let err = ModPackage::load(&package_dir).expect_err("an unknown schema must be refused");
        assert!(err.to_string().contains("nsr.mod.v1"), "{err}");
    }

    /// A package listing one destination twice is describing two contents for
    /// one file. Composing it would pick one and call it a result.
    #[test]
    fn a_package_that_lists_one_path_twice_is_refused() {
        let fx = fixture();
        let mut package = lay_package(
            &fx.packages,
            "reborn.ball.purple",
            None,
            &[(&format!("GN7E69/{BALL}"), b"purple football")],
        );
        package.files.push(package.files[0].clone());
        let err = package
            .validate()
            .expect_err("a duplicate path is ambiguous");
        assert!(err.to_string().contains("twice"), "{err}");
    }

    /// The marker is the launcher's own record. A texture with that name at the
    /// root of the composed tree would overwrite it, and then verification would
    /// demand a rebuild forever.
    #[test]
    fn nothing_may_write_the_marker_filename() {
        let fx = fixture();
        let mut package = lay_package(
            &fx.packages,
            "reborn.ball.purple",
            None,
            &[(&format!("GN7E69/{BALL}"), b"purple football")],
        );
        package.files[0].path = PayloadPath::parse("GN7E69/composed.json.dds").unwrap();
        assert!(
            package.validate().is_ok(),
            "a name that merely looks similar is fine"
        );

        // The reserved name itself cannot be spelled as a payload path at all,
        // because the extension allowlist rejects it, so the pristine side is
        // where the check has to bite.
        assert!(PayloadPath::parse("GN7E69/composed.json").is_err());

        let p = Platform::detect();
        write(&fx.pristine.join(COMPOSED_MARKER), b"not a texture");
        assert!(matches!(
            refusal(&p, &fx.request(Vec::new())),
            ComposeRefusal::ReservedName { .. }
        ));
    }

    /// An empty pristine tree composes to a tree of mod files only, which looks
    /// to the player like most textures vanished.
    #[test]
    fn an_empty_pristine_tree_is_refused() {
        let p = Platform::detect();
        let fx = fixture();
        std::fs::remove_file(fx.pristine.join(BALL)).unwrap();
        std::fs::remove_dir_all(fx.pristine.join("sub")).unwrap();

        let r = refusal(&p, &fx.request(Vec::new()));
        assert!(matches!(r, ComposeRefusal::NoPristineFiles { .. }));
        assert!(r.message().contains("Verify and repair"), "{}", r.message());
    }

    /// An absent pristine tree means the textures component is not installed.
    #[test]
    fn a_missing_pristine_tree_is_refused_with_a_usable_message() {
        let p = Platform::detect();
        let fx = fixture();
        std::fs::remove_dir_all(&fx.pristine).unwrap();

        let r = refusal(&p, &fx.request(Vec::new()));
        assert!(matches!(r, ComposeRefusal::NoPristineTree { .. }));
        assert!(
            r.message().contains("textures component"),
            "{}",
            r.message()
        );
    }

    /// One entry cannot be both a file and a directory. Left to composition
    /// time this fails part way through, leaving a partial tree.
    #[test]
    fn a_file_that_is_also_a_directory_prefix_is_refused_at_plan_time() {
        let mut entries = BTreeMap::new();
        for relative in ["sub.dds", "sub.dds/inner.dds", "other.dds"] {
            entries.insert(
                relative.to_string(),
                PlannedEntry {
                    relative: relative.to_string(),
                    bytes: 1,
                    source: EntrySource::Pristine {
                        path: PathBuf::from(relative),
                        modified: None,
                    },
                },
            );
        }
        match shape_refusal(&entries) {
            Some(ComposeRefusal::PathShadowsDirectory { file, inside }) => {
                assert_eq!(file, "sub.dds");
                assert_eq!(inside, "sub.dds/inner.dds");
            }
            other => panic!("expected a shadowing refusal, got {other:?}"),
        }
    }

    /// A link to a directory that points at an ancestor makes the walk loop
    /// forever if it is followed, and skipping it silently would drop textures
    /// out of the composed tree with nothing said about it. The link here points
    /// at the pristine root from inside it, which is that cycle exactly.
    ///
    /// Creating a symlink needs a privilege on Windows that a normal user does
    /// not have, so the fallback is a directory junction, which does not and
    /// which the standard library classifies the same way: a reparse point that
    /// is neither a file nor a directory.
    #[test]
    fn a_link_to_a_directory_in_the_pristine_tree_is_refused_rather_than_followed() {
        let p = Platform::detect();
        let fx = fixture();
        let link = fx.pristine.join("loop");

        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&fx.pristine, &link);
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&fx.pristine, &link).or_else(|_| {
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(&fx.pristine)
                .output()
                .and_then(|out| {
                    if link.exists() {
                        Ok(())
                    } else {
                        Err(std::io::Error::other(
                            String::from_utf8_lossy(&out.stderr).to_string(),
                        ))
                    }
                })
        });
        #[cfg(not(any(unix, windows)))]
        let made: std::io::Result<()> = Err(std::io::Error::other("no link API"));

        if made.is_err() {
            eprintln!(
                "could not create a symlink here, so the guard cannot be \
                 exercised on this machine"
            );
            return;
        }

        let r = refusal(&p, &fx.request(Vec::new()));
        assert!(
            matches!(r, ComposeRefusal::UnsupportedPristineEntries { .. }),
            "a symlink must be reported: {}",
            r.message()
        );
        assert!(link.exists());
    }

    /// The plan is read only. A caller can show conflicts and the disk cost
    /// before anything is committed to.
    #[test]
    fn planning_writes_nothing() {
        let p = Platform::detect();
        let fx = fixture();
        let package = lay_package(
            &fx.packages,
            "reborn.ball.purple",
            None,
            &[(&format!("GN7E69/{BALL}"), b"purple football")],
        );
        let plan = planned(&p, &fx.request(vec![package]));

        assert!(
            !fx.composed.exists(),
            "planning must not create the composed tree"
        );
        assert_eq!(plan.entries.len(), 2);
        assert_eq!(plan.overlay_count(), 1);
        assert_eq!(plan.pristine_count(), 1);

        // The overridden pristine file must not be counted as well as the mod
        // file that hides it, or the disk budget shown to the user is wrong by
        // the size of every overlay.
        let overlay_bytes = std::fs::metadata(
            fx.packages
                .join("reborn.ball.purple")
                .join("1.0.0")
                .join(PAYLOAD_SUBDIR)
                .join("GN7E69")
                .join(BALL),
        )
        .unwrap()
        .len();
        let kept_pristine = std::fs::metadata(fx.pristine.join("sub").join("tex1_other.dds"))
            .unwrap()
            .len();
        assert_eq!(plan.total_bytes(), overlay_bytes + kept_pristine);
        assert!(matches!(
            plan.entries
                .iter()
                .find(|e| e.relative == BALL)
                .map(|e| &e.source),
            Some(EntrySource::Overlay {
                replaces_pristine: true,
                ..
            }),
        ));
    }
    /// A package built in memory, with no payload on disk.
    ///
    /// exclusive_groups only reads the file lists, so laying real bytes down
    /// would slow the test without testing anything more.
    fn package_writing(id: &str, game: &GameId, tails: &[&str]) -> ModPackage {
        let files: Vec<ModFile> = tails
            .iter()
            .map(|tail| ModFile {
                path: PayloadPath::parse(&format!("{}/{tail}", game.as_str())).unwrap(),
                sha256: Sha256Hex::from_bytes(tail.as_bytes()),
                bytes: tail.len() as u64,
            })
            .collect();
        ModPackage {
            schema: MOD_SCHEMA.to_string(),
            magic: MOD_MAGIC.to_string(),
            id: ModId::parse(id).unwrap(),
            name: id.to_string(),
            version: PackageVersion::parse("1.0.0").unwrap(),
            targets: vec![game.clone()],
            conflict_group: None,
            files,
        }
    }

    #[test]
    fn the_footballs_come_out_as_one_pick_one_group() {
        // Derived from the file lists, with no curated table. Seven recoloured
        // footballs writing one texture path is a real library state, and
        // showing them as independently switchable would be untrue: turning on
        // two is refused, and before that JSGME kept whichever landed last
        // without saying so.
        let game = GameId::parse("GN7E69").unwrap();

        let mut packages: Vec<ModPackage> = ["aqua", "blue", "camo", "green", "pink"]
            .iter()
            .map(|name| package_writing(&format!("jsgme.{name}-football"), &game, &[BALL]))
            .collect();
        // One that touches nothing else, which must stay on its own.
        packages.push(package_writing(
            "jsgme.saints-alternate",
            &game,
            &["saints_jersey.dds"],
        ));

        let groups = exclusive_groups(&packages, &game);
        assert_eq!(groups.len(), 1, "one group, not one per pair: {groups:?}");
        assert_eq!(groups[0].members.len(), 5);
        assert!(
            groups[0].computed,
            "an overlap is refused by the composer, so it is not mere advice"
        );
        assert_eq!(
            groups[0].example_path.as_deref(),
            Some(BALL),
            "an interface has to be able to say why, not just assert it"
        );
        assert!(
            !groups[0]
                .members
                .iter()
                .any(|m| m.as_str() == "jsgme.saints-alternate"),
            "a mod that overlaps nothing must not be swept into a group"
        );
    }

    #[test]
    fn the_two_prompt_packs_are_exclusive_of_each_other() {
        // They write the same nineteen paths, which is why the controls profile
        // is one choice of three rather than two independent switches.
        let game = GameId::parse("GN7E69").unwrap();
        let prompt = "tex1_32x32_0121eda1cca926b4_86cd9b4a5d76e9f0_9.png";
        let packages = vec![
            package_writing("jsgme.xbox-controls", &game, &[prompt]),
            package_writing("jsgme.ps-controls", &game, &[prompt]),
        ];
        let groups = exclusive_groups(&packages, &game);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 2);
    }

    #[test]
    fn exclusivity_is_transitive_so_a_pick_one_is_a_single_choice() {
        // A overlaps B, B overlaps C, A and C share nothing. All three still
        // belong to one group: a pick-one control has to offer a choice, not a
        // puzzle about which pairs happen to be legal.
        let game = GameId::parse("GN7E69").unwrap();
        let packages = vec![
            package_writing("mod.a", &game, &["shared_ab.dds", "only_a.dds"]),
            package_writing("mod.b", &game, &["shared_ab.dds", "shared_bc.dds"]),
            package_writing("mod.c", &game, &["shared_bc.dds", "only_c.dds"]),
        ];
        let groups = exclusive_groups(&packages, &game);
        assert_eq!(groups.len(), 1, "got {groups:?}");
        assert_eq!(groups[0].members.len(), 3);
    }

    #[test]
    fn a_library_with_no_overlap_has_no_groups() {
        // No group is the normal case, and an empty result must not be dressed
        // up as one group containing everything.
        let game = GameId::parse("GN7E69").unwrap();
        let packages = vec![
            package_writing("mod.a", &game, &["a.dds"]),
            package_writing("mod.b", &game, &["b.dds"]),
        ];
        assert!(exclusive_groups(&packages, &game).is_empty());
    }

    // ---- the controls profile as an activation ------------------------------

    #[test]
    fn choosing_a_profile_turns_the_other_prompt_pack_off() {
        // The two packs write the same nineteen paths. Leaving an old one on
        // would make the composition refuse rather than quietly pick one, so
        // the profiles have to be exclusive in the activation and not only in
        // the interface.
        let fx = fixture();
        let xbox = lay_package(
            &fx.packages,
            "jsgme.xbox-controls",
            None,
            &[(&format!("GN7E69/{BALL}"), b"xbox prompt")],
        );
        let ps = lay_package(
            &fx.packages,
            "jsgme.ps-controls",
            None,
            &[(&format!("GN7E69/{BALL}"), b"ps prompt")],
        );
        let installed = vec![xbox.clone(), ps.clone()];

        let first =
            apply_controls_profile(&fx.activation(), ControlsProfile::Xbox, &installed).unwrap();
        assert_eq!(first.turned_on, Some(xbox.id.clone()));
        assert!(!first.already_correct);

        // Switching profiles turns one off and the other on.
        let second =
            apply_controls_profile(&fx.activation(), ControlsProfile::PlayStation, &installed).unwrap();
        assert_eq!(second.turned_on, Some(ps.id.clone()));
        assert_eq!(
            second.turned_off,
            vec![xbox.id.clone()],
            "the previous pack must come off, or the two cannot compose together"
        );

        let path = fx.activation();
        let saved = Activation::load(&path).unwrap();
        assert!(saved.is_active(&ps.id));
        assert!(!saved.is_active(&xbox.id));
    }

    #[test]
    fn gamecube_turns_every_prompt_pack_off_and_installs_nothing() {
        // The profile that applies nothing. Not a gap: the prompt textures are
        // add-only, so having no pack uncovers what the disc itself drew.
        let fx = fixture();
        let xbox = lay_package(
            &fx.packages,
            "jsgme.xbox-controls",
            None,
            &[(&format!("GN7E69/{BALL}"), b"xbox prompt")],
        );
        let installed = vec![xbox.clone()];

        apply_controls_profile(&fx.activation(), ControlsProfile::Xbox, &installed).unwrap();
        let out =
            apply_controls_profile(&fx.activation(), ControlsProfile::GameCube, &installed).unwrap();

        assert_eq!(out.turned_on, None, "GameCube must install nothing");
        assert_eq!(out.turned_off, vec![xbox.id.clone()]);
        assert_eq!(out.missing_package, None, "nothing was missing; none is wanted");

        let saved = Activation::load(&fx.activation()).unwrap();
        assert!(!saved.is_active(&xbox.id));
    }

    #[test]
    fn a_second_launch_changes_nothing() {
        // This runs on every launch. Recomposing eight thousand entries to
        // change nothing is time a player waits through, so an unchanged
        // profile has to be detectable without writing.
        let fx = fixture();
        let xbox = lay_package(
            &fx.packages,
            "jsgme.xbox-controls",
            None,
            &[(&format!("GN7E69/{BALL}"), b"xbox prompt")],
        );
        let installed = vec![xbox];

        apply_controls_profile(&fx.activation(), ControlsProfile::Xbox, &installed).unwrap();
        let again =
            apply_controls_profile(&fx.activation(), ControlsProfile::Xbox, &installed).unwrap();
        assert!(
            again.already_correct,
            "an unchanged profile must not rewrite the activation"
        );
        assert!(again.turned_off.is_empty());
    }

    #[test]
    fn a_profile_whose_package_is_not_installed_is_reported_not_raised() {
        // The choice is a recorded decision and still stands. What is missing is
        // the art, and the game draws its own until the package arrives.
        // Failing a launch over button art would be worse than wrong buttons.
        let fx = fixture();
        let installed: Vec<ModPackage> = Vec::new();

        let out =
            apply_controls_profile(&fx.activation(), ControlsProfile::PlayStation, &installed).unwrap();
        assert_eq!(out.turned_on, None);
        let why = out.missing_package.expect("the gap must be reported");
        assert!(why.contains("jsgme.ps-controls"), "{why}");
        assert!(
            why.contains("PlayStation"),
            "the message must name the profile a player chose: {why}"
        );
        assert!(
            !why.contains("  "),
            "a doubled space means a lost line continuation: {why}"
        );
    }

    // ---- the composed tree in the user folder --------------------------------

    #[test]
    fn a_foreign_texture_folder_is_moved_aside_never_deleted_and_ours_is_reused() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user");
        let at = user.join("Load").join("Textures").join("GN7E69");
        // nothing there: nothing to do
        assert_eq!(prepare_texture_folder(&at, 1).unwrap(), None);
        // a real folder of somebody's own textures
        write(&at.join("theirs.dds"), b"theirs");
        let aside = prepare_texture_folder(&at, 42).unwrap().expect("moved aside");
        assert_eq!(aside, user.join("Load").join("Textures").join("GN7E69.before-reborn.42"));
        assert!(aside.join("theirs.dds").is_file() && !at.exists());
        // an empty folder is simply reused
        std::fs::create_dir_all(&at).unwrap();
        assert_eq!(prepare_texture_folder(&at, 43).unwrap(), None);
        assert!(at.is_dir());
        // a composed tree of ours is left for verify/apply
        write(&at.join(COMPOSED_MARKER), b"{}");
        assert_eq!(prepare_texture_folder(&at, 44).unwrap(), None);
        assert!(at.join(COMPOSED_MARKER).is_file());
    }

    /// The player's question is "do my textures load", and the honest answer
    /// is whether the folder Dolphin reads exists and has files in it. A
    /// folder that resolves but is empty loads nothing, so it is not
    /// reachable.
    #[test]
    fn a_missing_or_empty_texture_folder_is_not_reachable() {
        let dir = tempfile::tempdir().unwrap();
        let game = game();
        let user = dir.path().join("user");
        let p = Platform::detect();

        // Nothing there at all.
        let absent = pack_status(&p, &user, &game);
        assert!(!absent.reachable, "an absent folder cannot load textures");

        // Present but empty: resolves, loads nothing.
        let composed = composed_root(&user, &game);
        std::fs::create_dir_all(&composed).unwrap();
        let empty = pack_status(&p, &user, &game);
        assert!(!empty.reachable, "an empty folder loads no textures");
        assert_eq!(empty.textures_dir, composed, "it must name the folder Dolphin reads");

        // One file is enough to load something.
        write(&composed.join("tex1_x.png"), b"not really a png");
        let full = pack_status(&p, &user, &game);
        assert!(full.reachable, "a folder with files in it loads textures");
    }

    /// A tree the launcher composed carries its marker. A folder the player
    /// or another tool put there does not, and saying so is how the Play
    /// page can tell "your pack" from "someone else's folder".
    #[test]
    fn a_folder_we_did_not_compose_is_reported_as_not_ours() {
        let dir = tempfile::tempdir().unwrap();
        let game = game();
        let user = dir.path().join("user");
        let p = Platform::detect();
        let composed = composed_root(&user, &game);
        write(&composed.join("tex1_x.png"), b"x");

        assert!(!pack_status(&p, &user, &game).composed_by_us);

        write(&composed.join(COMPOSED_MARKER), b"{}");
        assert!(pack_status(&p, &user, &game).composed_by_us);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_the_way_is_removed_without_touching_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("old-composed");
        write(&target.join("a.dds"), b"a");
        let at = dir.path().join("user").join("Load").join("Textures").join("GN7E69");
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &at).unwrap();
        assert_eq!(prepare_texture_folder(&at, 1).unwrap(), None);
        assert!(!at.exists() && target.join("a.dds").is_file());
    }

    #[test]
    fn the_composed_tree_lives_in_the_user_folder_and_the_game_folder_is_a_real_directory() {
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let guard = lock(&p, &fx);
        let report = done(sync_active(&p, &guard, &sync_request(&fx, &user_dir), 1).unwrap());
        let at = user_dir.join("Load").join("Textures").join("GN7E69");
        assert_eq!(report.composed_root, at);
        assert!(std::fs::symlink_metadata(&at).unwrap().is_dir(), "a real directory, not a link");
        assert!(at.join(COMPOSED_MARKER).is_file());
        assert!(at.join(BALL).is_file());
        assert!(!at.parent().unwrap().join("GN7E69.building").exists());
        assert!(report.moved_aside.is_none());
    }

    #[test]
    fn an_old_composed_tree_under_mods_active_is_retired_once() {
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        write(&fx.composed.join(COMPOSED_MARKER), b"{}");
        write(&fx.composed.join("a.dds"), b"a");
        let foreign = fx.composed.parent().unwrap().join("FR4N31");
        write(&foreign.join("keep.dds"), b"not ours: no marker");
        let guard = lock(&p, &fx);
        done(sync_active(&p, &guard, &sync_request(&fx, &user_dir), 1).unwrap());
        assert!(!fx.composed.exists(), "ours, retired");
        assert!(foreign.join("keep.dds").is_file(), "no marker, never deleted");
    }

    /// The update engine swaps the whole pristine folder. The composed tree
    /// is hardlinks into the OLD folder, so with the same names and count
    /// nothing used to notice and the game kept the old bytes. The record
    /// carries the pristine tree's identity, so a new identity rebuilds.
    #[test]
    fn a_pristine_tree_swapped_under_the_same_names_rebuilds_the_composed_tree() {
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let guard = lock(&p, &fx);
        let mut req = sync_request(&fx, &user_dir);
        req.pristine_identity = Some("1.1.0#2".into());
        done(sync_active(&p, &guard, &req, 1).unwrap());
        let composed = composed_root(&user_dir, &game()).join(BALL);
        assert_eq!(std::fs::read(&composed).unwrap(), b"the original football");

        // the swap: same names, new bytes
        let prior = fx.pristine.with_file_name("GN7E69.prior");
        std::fs::rename(&fx.pristine, &prior).unwrap();
        lay_pristine(&fx.pristine);
        std::fs::write(fx.pristine.join(BALL), b"the NEW football").unwrap();
        req.pristine_identity = Some("1.2.0#3".into());
        let rebuilt = done(sync_active(&p, &guard, &req, 2).unwrap());
        assert!(matches!(rebuilt.tree, TreeStep::Rebuilt { .. }), "{:?}", rebuilt.tree);
        assert_eq!(std::fs::read(&composed).unwrap(), b"the NEW football");
    }

    /// The roster case, 2026-09-23: the Builder rewrites a portrait INTO the
    /// pristine tree under its old name. The new file is a new file, so the
    /// composed link keeps the old face, and the textures identity, every
    /// name and every count stay the same. Michael Penix Jr. took Cooper
    /// Rush's slot and wore Rush's face on every install. Same length here on
    /// purpose: a 64x64 BC7 portrait is the same size whoever it shows.
    #[test]
    fn a_pristine_file_replaced_under_the_same_name_rebuilds_the_composed_tree() {
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let guard = lock(&p, &fx);
        let mut req = sync_request(&fx, &user_dir);
        req.pristine_identity = Some("1.3.3#8".into());
        done(sync_active(&p, &guard, &req, 1).unwrap());
        let composed = composed_root(&user_dir, &game()).join(BALL);
        assert_eq!(std::fs::read(&composed).unwrap(), b"the original football");

        // what the Builder does: write a new file, rename it over the old one
        let source = fx.pristine.join(BALL);
        let fresh = fx.pristine.join("portrait.tmp");
        write(&fresh, b"the replaced football");
        assert_eq!(std::fs::metadata(&fresh).unwrap().len(), std::fs::metadata(&source).unwrap().len());
        let later = std::fs::metadata(&source).unwrap().modified().unwrap() + Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&fresh).unwrap().set_modified(later).unwrap();
        std::fs::rename(&fresh, &source).unwrap();
        assert_eq!(std::fs::read(&composed).unwrap(), b"the original football", "the link kept the old bytes");

        let rebuilt = done(sync_active(&p, &guard, &req, 2).unwrap());
        assert!(matches!(rebuilt.tree, TreeStep::Rebuilt { .. }), "{:?}", rebuilt.tree);
        assert_eq!(std::fs::read(&composed).unwrap(), b"the replaced football");

        let again = done(sync_active(&p, &guard, &req, 3).unwrap());
        assert_eq!(again.tree, TreeStep::AlreadyCurrent, "a rebuilt tree must not rebuild again");
    }

    #[test]
    fn write_times_within_filesystem_rounding_are_the_same_write() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        assert!(same_write_time(Some(t), Some(t)));
        assert!(same_write_time(Some(t), Some(t + Duration::from_secs(2))), "FAT32 rounds to two seconds");
        assert!(same_write_time(Some(t + Duration::from_millis(10)), Some(t)), "either direction");
        assert!(!same_write_time(Some(t), Some(t + Duration::from_secs(3))));
        assert!(same_write_time(None, Some(t)), "no time reported is no evidence");
    }

    // -----------------------------------------------------------------------
    /// `lay_package` plus the manifest on disk, which is what makes a
    /// package INSTALLED rather than merely laid out: sync reads the list
    /// and loads each package from its directory, so a package that exists
    /// only as a struct in a test is, to sync, missing.
    fn lay_installed_package(
        packages_root: &Path,
        id: &str,
        group: Option<&str>,
        files: &[(&str, &[u8])],
    ) -> ModPackage {
        let package = lay_package(packages_root, id, group, files);
        let manifest = packages_root
            .join(package.id.as_str())
            .join(package.version.as_str())
            .join(PACKAGE_MANIFEST);
        write(&manifest, &serde_json::to_vec_pretty(&package).unwrap());
        package
    }

    // sync_active: the one operation every change to the active set ends in
    // -----------------------------------------------------------------------

    fn sync_request(fx: &Fixture, user_dir: &Path) -> SyncRequest {
        SyncRequest {
            game: game(),
            pristine_root: fx.pristine.clone(),
            base_root: None,
            composed_root: composed_root(user_dir, &game()),
            packages_root: fx.packages.clone(),
            activation_file: fx.activation(),
            user_dir: user_dir.to_path_buf(),
            old_composed_root: Some(fx.composed.clone()),
            pristine_identity: None,
            payload_fallback: None,
            active_override: None,
            texture_policy: TexturePolicy::Manage,
        }
    }

    #[test]
    fn a_fresh_user_dir_is_not_a_missing_pack_when_play_will_compose_it() {
        // 2026-10-04: every fresh install showed "Texture pack not loading" on Play, because the folder Dolphin reads
        // is only composed when Play starts.
        let fx = fixture();
        let user = fx.run.parent().unwrap().join("dolphin-users").join("fresh-1");
        std::fs::create_dir_all(&user).unwrap();
        let p = Some(fx.pristine.as_path());
        assert!(!std::fs::read_dir(composed_root(&user, &game())).map(|mut e| e.next().is_some()).unwrap_or(false), "nothing composed yet");
        assert!(pack_loads_at_play_with(p, &user, &game(), TexturePolicy::Manage), "fresh folder + shipped pack: Play composes it");
        assert!(!pack_loads_at_play_with(p, &user, &game(), TexturePolicy::Withhold), "a withheld image: Play removes the pack");
        assert!(!pack_loads_at_play_with(p, &user, &game(), TexturePolicy::LeaveAlone), "an empty folder the player manages loads nothing");
        write(&composed_root(&user, &game()).join(BALL), b"an author's football");
        assert!(pack_loads_at_play_with(p, &user, &game(), TexturePolicy::LeaveAlone), "the player's own folder, used as it is");
        std::fs::remove_dir_all(composed_root(&user, &game())).unwrap();
        std::fs::remove_dir_all(&fx.pristine).unwrap();
        assert!(!pack_loads_at_play_with(p, &user, &game(), TexturePolicy::Manage), "no shipped pack: nothing to compose");
        assert!(!pack_loads_at_play_with(None, &user, &game(), TexturePolicy::Manage));
    }

    /// What the emulator would load: read from the user directory, which is
    /// where the composed tree lives.
    fn loaded_by_the_game(user_dir: &Path, relative: &str) -> Vec<u8> {
        std::fs::read(
            user_dir
                .join("Load")
                .join("Textures")
                .join("GN7E69")
                .join(relative),
        )
        .unwrap()
    }

    fn done(outcome: SyncOutcome) -> SyncReport {
        match outcome {
            SyncOutcome::Done(r) => *r,
            SyncOutcome::Refused(why) => panic!("unexpected refusal: {why}"),
        }
    }

    #[test]
    fn sync_makes_the_game_load_what_activation_says_and_is_idle_when_nothing_changed() {
        // Before this existed: the controls profile wrote activation.json
        // correctly at every launch, and the game never saw it, because
        // nothing composed the tree or pointed Load/Textures at it.
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let package = lay_installed_package(
            &fx.packages,
            "reborn.ball.purple",
            Some("ball"),
            &[(&format!("GN7E69/{BALL}"), b"the purple football")],
        );
        let mut activation = Activation::default();
        activation.activate(package.id.clone(), package.version.clone());
        activation.save(&fx.activation()).unwrap();
        let guard = lock(&p, &fx);
        let req = sync_request(&fx, &user_dir);

        let first = done(sync_active(&p, &guard, &req, 1).unwrap());
        assert!(
            matches!(first.tree, TreeStep::Rebuilt { overlaid: 1, .. }),
            "a fresh sync builds the tree: {:?}",
            first.tree
        );
        assert!(first.moved_aside.is_none());
        assert!(first.missing.is_empty());
        assert_eq!(loaded_by_the_game(&user_dir, BALL), b"the purple football");

        // Nothing changed, so nothing is rebuilt. This is what makes a sync on
        // every launch affordable on an eight thousand file tree.
        let again = done(sync_active(&p, &guard, &req, 2).unwrap());
        assert_eq!(again.tree, TreeStep::AlreadyCurrent);
        assert!(again.moved_aside.is_none());

        // Turned off: the game goes back to the original, and the original was
        // never touched to get there.
        Activation::default().save(&fx.activation()).unwrap();
        let off = done(sync_active(&p, &guard, &req, 3).unwrap());
        assert!(matches!(off.tree, TreeStep::Rebuilt { overlaid: 0, .. }));
        assert_eq!(loaded_by_the_game(&user_dir, BALL), b"the original football");
        assert_eq!(
            std::fs::read(fx.pristine.join(BALL)).unwrap(),
            b"the original football"
        );
        assert!(off.explain().starts_with("0 mod(s) on, tree rebuilt"), "{}", off.explain());
    }

    #[test]
    fn sync_reports_a_listed_package_that_is_not_installed_and_leaves_it_listed() {
        // Its files are gone but the player turned it on. The tree is built
        // without it and the entry stays, so reinstalling the package brings
        // it back without the player having to remember to turn it on again.
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let ghost = ActivePackage {
            id: ModId::parse("reborn.ball.ghost").unwrap(),
            version: PackageVersion::parse("1.0.0").unwrap(),
        };
        let activation = Activation {
            active: vec![ghost.clone()],
            ..Default::default()
        };
        activation.save(&fx.activation()).unwrap();
        let guard = lock(&p, &fx);

        let report = done(sync_active(&p, &guard, &sync_request(&fx, &user_dir), 1).unwrap());
        assert_eq!(report.missing, vec![ghost.clone()]);
        assert_eq!(report.active, vec![ghost.clone()]);
        assert!(matches!(report.tree, TreeStep::Rebuilt { overlaid: 0, .. }));
        assert_eq!(loaded_by_the_game(&user_dir, BALL), b"the original football");
        assert_eq!(
            Activation::load(&fx.activation()).unwrap().active,
            vec![ghost],
            "the entry must survive: the launcher does not decide a mod is off"
        );
        assert!(
            report.explain().contains("listed but not installed: reborn.ball.ghost"),
            "{}",
            report.explain()
        );
    }

    #[test]
    fn sync_refuses_a_hard_conflict_and_changes_nothing() {
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let purple = lay_installed_package(
            &fx.packages,
            "reborn.ball.purple",
            None,
            &[(&format!("GN7E69/{BALL}"), b"purple")],
        );
        let green = lay_installed_package(
            &fx.packages,
            "reborn.ball.green",
            None,
            &[(&format!("GN7E69/{BALL}"), b"green")],
        );
        let mut activation = Activation::default();
        activation.activate(purple.id.clone(), purple.version.clone());
        activation.activate(green.id.clone(), green.version.clone());
        activation.save(&fx.activation()).unwrap();
        let guard = lock(&p, &fx);

        let outcome = sync_active(&p, &guard, &sync_request(&fx, &user_dir), 1).unwrap();
        let SyncOutcome::Refused(why) = outcome else {
            panic!("two packages on one football must refuse, got {outcome:?}");
        };
        assert!(why.contains("both write"), "{why}");
        assert!(!fx.composed.exists(), "a refused sync must not build a tree");
        assert!(
            !user_dir.join("Load").exists(),
            "a refused sync must not point the game anywhere"
        );
        assert!(SyncOutcome::Refused(why).explain().starts_with("Mods were not applied: "));
    }

    #[test]
    fn starting_with_mods_disabled_loads_the_pristine_tree_and_keeps_the_selection() {
        // The recovery page's "Start with mods disabled". One launch without
        // the packages, and activation.json untouched, so the next launch has
        // the player's selection back without a click.
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let package = lay_installed_package(
            &fx.packages,
            "reborn.ball.purple",
            Some("ball"),
            &[(&format!("GN7E69/{BALL}"), b"the purple football")],
        );
        let mut activation = Activation::default();
        activation.activate(package.id.clone(), package.version.clone());
        activation.save(&fx.activation()).unwrap();
        let guard = lock(&p, &fx);

        let mut req = sync_request(&fx, &user_dir);
        req.active_override = Some(Vec::new());
        let report = done(sync_active(&p, &guard, &req, 1).unwrap());
        assert!(matches!(report.tree, TreeStep::Rebuilt { overlaid: 0, .. }), "{:?}", report.tree);
        assert!(report.active.is_empty());
        assert_eq!(loaded_by_the_game(&user_dir, BALL), b"the original football");
        assert_eq!(
            Activation::load(&fx.activation()).unwrap().active.len(),
            1,
            "the selection must survive a disabled launch"
        );

        // And the plain sync afterwards brings the mod back.
        req.active_override = None;
        let again = done(sync_active(&p, &guard, &req, 2).unwrap());
        assert!(matches!(again.tree, TreeStep::Rebuilt { overlaid: 1, .. }));
        assert_eq!(loaded_by_the_game(&user_dir, BALL), b"the purple football");
    }

    #[test]
    fn the_texture_folder_can_be_left_completely_alone() {
        // Someone who authors textures owns that folder. Nothing is composed
        // into it and nothing in it is touched. Adam lost a folder to this once.
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let theirs = user_dir.join("Load").join("Textures").join("GN7E69");

        let package = lay_installed_package(
            &fx.packages,
            "reborn.ball.purple",
            Some("ball"),
            &[(&format!("GN7E69/{BALL}"), b"the purple football")],
        );
        let mut activation = Activation::default();
        activation.activate(package.id.clone(), package.version.clone());
        activation.save(&fx.activation()).unwrap();
        let guard = lock(&p, &fx);
        // After the lock: a Load/Textures tree makes a directory look like a
        // 2.1 install, and the supervisor rightly refuses to use one of those
        // as a launcher data root.
        write(&theirs.join("mine.dds"), b"a texture I am working on");

        let mut req = sync_request(&fx, &user_dir);
        req.texture_policy = TexturePolicy::LeaveAlone;
        let report = done(sync_active(&p, &guard, &req, 1).unwrap());

        assert_eq!(report.tree, TreeStep::LeftAlone, "nothing is composed into a folder the author owns");
        assert!(report.moved_aside.is_none());
        assert_eq!(
            std::fs::read(theirs.join("mine.dds")).unwrap(),
            b"a texture I am working on",
            "their folder must be exactly as they left it"
        );
        assert!(!theirs.join(BALL).exists(), "and nothing of ours may appear in it");
        assert!(report.explain().contains("left alone"), "{}", report.explain());
    }

    /// Withholding has to TAKE THE PACK AWAY, not merely decline to rebuild it.
    ///
    /// This is the finding the whole law turned on. `manage_user_texture_folder
    /// = false` was reused to mean "this image is not ours", and all it does is
    /// return early leaving `Load/Textures/GN7E69` exactly as it was. For any
    /// player who has ever booted successfully, exactly as it was IS the whole
    /// composed pack, and Dolphin loads it by game ID with `HiresTextures`
    /// locked on. So the test composes first, the way a real player's first
    /// good boot does, and only then asks what a withheld sync leaves behind.
    #[test]
    fn withholding_removes_the_pack_a_previous_launch_composed() {
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let composed = user_dir.join("Load").join("Textures").join("GN7E69");

        let package = lay_installed_package(
            &fx.packages,
            "reborn.ball.purple",
            Some("ball"),
            &[(&format!("GN7E69/{BALL}"), b"the purple football")],
        );
        let mut activation = Activation::default();
        activation.activate(package.id.clone(), package.version.clone());
        activation.save(&fx.activation()).unwrap();
        let guard = lock(&p, &fx);

        // A good boot: the pack is composed into the user folder.
        let mut req = sync_request(&fx, &user_dir);
        let first = done(sync_active(&p, &guard, &req, 1).unwrap());
        assert!(matches!(first.tree, TreeStep::Rebuilt { .. }), "{:?}", first.tree);
        assert_eq!(loaded_by_the_game(&user_dir, BALL), b"the purple football");

        // Then the player points Reborn at a disc it cannot vouch for.
        req.texture_policy = TexturePolicy::Withhold;
        let withheld = done(sync_active(&p, &guard, &req, 2).unwrap());

        assert_eq!(
            withheld.tree,
            TreeStep::Withheld { removed: true },
            "a withheld sync must report that it took the tree away"
        );
        assert!(
            !composed.exists(),
            "Reborn's texture pack is still at {} for Dolphin to paint on a disc that is not ours",
            composed.display()
        );
        assert!(
            withheld.explain().contains("not an NFL Street Reborn build"),
            "{}",
            withheld.explain()
        );
    }

    /// Withholding never destroys somebody else's folder.
    ///
    /// The removal is `Composer::discard`, which only ever removes a tree
    /// carrying our own marker or an empty one. A folder of the player's own
    /// textures sitting at the composed path is refused with a sentence and
    /// left standing, rather than deleted to enforce the rule.
    #[test]
    fn withholding_refuses_rather_than_delete_a_folder_that_is_not_ours() {
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let theirs = user_dir.join("Load").join("Textures").join("GN7E69");
        let guard = lock(&p, &fx);
        write(&theirs.join("mine.dds"), b"a texture I am working on");

        let mut req = sync_request(&fx, &user_dir);
        req.texture_policy = TexturePolicy::Withhold;
        let outcome = sync_active(&p, &guard, &req, 1).unwrap();

        assert!(
            matches!(outcome, SyncOutcome::Refused(_)),
            "a foreign tree must be refused, not deleted: {outcome:?}"
        );
        assert_eq!(
            std::fs::read(theirs.join("mine.dds")).unwrap(),
            b"a texture I am working on",
            "their folder must be exactly as they left it"
        );
    }

    /// Withholding on a machine that never composed anything is not an error.
    #[test]
    fn withholding_with_nothing_composed_is_quiet() {
        let p = Platform::detect();
        let fx = fixture();
        let user_dir = fx.run.parent().unwrap().join("User");
        let guard = lock(&p, &fx);
        let mut req = sync_request(&fx, &user_dir);
        req.texture_policy = TexturePolicy::Withhold;
        let report = done(sync_active(&p, &guard, &req, 1).unwrap());
        assert_eq!(report.tree, TreeStep::Withheld { removed: false });
    }

    #[test]
    fn a_migrated_tree_never_shadows_the_one_the_release_shipped() {
        // Adam: "but what if we overwrote some textures from 2.1". An
        // upgrader's data root tree is their OLD pack, rebuilt by un-applying
        // JSGME. If it won, they would never see a new field and nothing would
        // say so. It only wins once something has marked it as delivered.
        let dir = tempfile::tempdir().unwrap();
        let migrated = dir.path().join("data").join("library").join("textures").join("GN7E69");
        let shipped = dir.path().join("install").join("content").join("textures").join("GN7E69");
        write(&migrated.join("old.dds"), b"a 2.1 texture");
        write(&shipped.join("new.dds"), b"a 3.0 texture");

        // The rule, applied to these two paths the way pristine_root applies it.
        let choose = |data: &Path, ship: &Path| -> PathBuf {
            if data.join(PRISTINE_MARKER).is_file() {
                data.to_path_buf()
            } else if ship.is_dir() {
                ship.to_path_buf()
            } else {
                data.to_path_buf()
            }
        };

        assert_eq!(choose(&migrated, &shipped), shipped, "the release wins over a migration");

        // Once an update has delivered a tree into the data root, that one is
        // newer than anything the installer carried and takes over.
        write(&migrated.join(PRISTINE_MARKER), br#"{"version":"3.1.0"}"#);
        assert_eq!(choose(&migrated, &shipped), migrated, "a delivered tree wins");

        // And with nothing shipped, a migrated tree is better than nothing.
        let nowhere = dir.path().join("install").join("absent");
        write(&migrated.join(PRISTINE_MARKER), b"x");
        std::fs::remove_file(migrated.join(PRISTINE_MARKER)).unwrap();
        assert_eq!(choose(&migrated, &nowhere), migrated);
    }

    #[test]
    fn a_tree_installed_by_a_content_update_counts_as_delivered() {
        // The marker was the only mark, and nothing in production wrote one.
        // So a texture update would have landed in the data root and
        // composition would have gone on reading the installer's tree. The
        // content pointer is the mark now.
        use crate::content::{ContentLayout, InstalledVersion, Pointer, POINTER_SCHEMA};
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let layout = ContentLayout::new(&data);

        assert!(!textures_delivered(&data, "GN7E69"), "no pointer, nothing delivered");

        Pointer {
            schema: POINTER_SCHEMA.to_string(),
            component: "textures".to_string(),
            current: InstalledVersion { version: "1.0.1".to_string(), sequence: 3 },
            previous: None,
        }
        .save(&Platform::detect(), &layout.pointer_path("textures"))
        .unwrap();
        assert!(textures_delivered(&data, "GN7E69"), "a pointer means an update delivered it");

        // With the cached manifest present, it says which game id the tree is for.
        std::fs::write(
            layout.manifest_path("textures"),
            br#"{"schema":"nsr.component.v1","component":"textures","version":"1.0.1","sequence":3,"install_root":"library/textures/FR4N31","customizable":false,"files":[]}"#,
        )
        .unwrap();
        assert!(!textures_delivered(&data, "GN7E69"), "delivered for another game id is not delivered for this one");
        assert!(textures_delivered(&data, "FR4N31"));
    }
}
