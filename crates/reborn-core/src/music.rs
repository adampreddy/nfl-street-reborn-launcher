//! Custom soundtrack: the pack, the order, and not eating the user's files.
//!
//! # This module owns no new rule
//!
//! The governing rule is already written down twice. [`crate::manifest`] makes
//! `customizable` a mandatory field, and [`crate::library`] turns it into
//! [`FileVerdict::UserModified`], which is the verdict that stops a repair
//! re-fetching 1.5 GB over a rebuilt image. Music is the only feature in the
//! product that deliberately makes a managed artifact stop matching its
//! manifest, so it is the feature that rule exists for.
//!
//! What this module adds is the other half of that handshake: a rebuild has to
//! actually record the hash it produced, or `UserModified` can never fire and
//! every customized install reads as damaged. [`SoundtrackState::record_into`]
//! is that step, and it refuses to record a hash no rebuild produced.
//!
//! # Why the byte-level writer is not here
//!
//! The game plays any-length songs by reading a per-stream position table
//! inside `pathfind.dat`, so a soundtrack edit is audio plus table entries.
//! This launcher does not have that table's entry layout, does not have the
//! EA-exact sample accounting `extend_slot_40big.py` performs, and does not
//! have the `template.dat` title, artist and album record layout.
//!
//! Guessing any of the three produces a disc that mounts, boots, and plays
//! silence or noise, which is the worst possible failure because it looks like
//! success. So [`write_soundtrack`] and [`read_pack`] are typed gaps that
//! return [`Error::NotImplemented`] and name what is missing. The proven Python
//! chain stays the only thing that writes bytes until the layout is documented.
//!
//! # Online is untouched, on purpose
//!
//! Music is offline only. That is not a limitation of this module, it is what
//! lets matchmaking trust the online image: `online-content` is never
//! `customizable`, so it always repairs cleanly and a verify before a match
//! means something. [`permitted_on`] refuses the online build rather than
//! leaving every caller to remember.
//!
//! [`FileVerdict::UserModified`]: crate::library::FileVerdict::UserModified

use std::collections::BTreeMap;
use std::path::{Component as PathComponent, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::RebuildState;
use crate::manifest::Sha256Hex;
use crate::platform::{DataDir, Error, Result};
use crate::runtime::GameBuild;

/// How many stream slots the table holds.
///
/// Forty, agreed by two independent things in this repository: the `.nsmp`
/// format 1 validation is a 40-stream check, and the tool that resizes them is
/// `extend_slot_40big.py`, documented as "40 bigger segments, EA-exact sample
/// accounting".
///
/// It is a hard bound rather than a hint. A slot number the table cannot hold,
/// written into the position table, is a disc that boots and plays garbage, so
/// every entry point here refuses one instead of clamping it.
pub const STREAM_SLOTS: u8 = 40;

/// Whether the table has this slot.
pub fn is_valid_slot(slot: u8) -> bool {
    slot < STREAM_SLOTS
}

/// File name of the soundtrack record, inside the `music` directory.
///
/// Deliberately not `state.json`. [`crate::library::RebuildState`] already
/// documents itself as `music/state.json`, and its `save` serializes only its
/// own single field, so putting both records in one file means whichever
/// subsystem saves last silently truncates the other's. Losing the soundtrack
/// record loses the proof that a modified image is the user's, which is exactly
/// how repair ends up overwriting a custom soundtrack.
pub const SOUNDTRACK_FILE: &str = "soundtrack.json";

/// Extensions accepted as an imported source.
///
/// A list of containers the bundled FFmpeg can open, not a claim about codecs.
/// Anything else is left out of the scan rather than offered and then failed at
/// encode time, after the user has already picked it.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "ogg", "opus", "m4a", "aac", "wma", "aiff", "aif",
];

// ---------------------------------------------------------------------------
// Duration, and the refusal to invent one
// ---------------------------------------------------------------------------

/// A length in milliseconds.
///
/// A newtype because this module also carries byte counts and slot numbers, and
/// a total duration that is silently a byte count is the kind of wrong that
/// renders as a plausible number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DurationMs(u64);

impl DurationMs {
    pub fn from_millis(ms: u64) -> Self {
        Self(ms)
    }

    pub fn as_millis(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for DurationMs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total = self.0 / 1000;
        let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
        if h > 0 {
            write!(f, "{h}:{m:02}:{s:02}")
        } else {
            write!(f, "{m}:{s:02}")
        }
    }
}

/// The length of a whole order, with the unmeasured part stated rather than
/// folded in as zero.
///
/// Measuring a track's length needs a decoder, which lives in the bundled
/// FFmpeg and not in this crate. So most tracks arrive with no length, and the
/// obvious shortcut of treating absent as zero produces a total that is simply
/// wrong and looks fine. This type makes the caller say "42:10 plus 3 tracks of
/// unknown length", which is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TotalDuration {
    /// Sum of the tracks whose length something actually measured.
    pub known: DurationMs,
    /// How many tracks contributed nothing, because nothing measured them.
    pub unmeasured_tracks: usize,
}

impl TotalDuration {
    /// Whether every track in the order has a measured length.
    pub fn is_exact(self) -> bool {
        self.unmeasured_tracks == 0
    }

    /// A phrase for the interface. Never claims more than was measured.
    pub fn describe(self) -> String {
        match (self.unmeasured_tracks, self.known.as_millis()) {
            (0, _) => self.known.to_string(),
            (n, 0) => format!("unknown, {n} {} have no measured length", tracks_word(n)),
            (n, _) => format!(
                "{} plus {n} {} of unknown length",
                self.known,
                tracks_word(n)
            ),
        }
    }
}

fn tracks_word(n: usize) -> &'static str {
    if n == 1 {
        "track"
    } else {
        "tracks"
    }
}

// ---------------------------------------------------------------------------
// Where audio comes from
// ---------------------------------------------------------------------------

/// Where a pack came from, which decides whether repair may replace it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackOrigin {
    /// Assembled by this user out of their own files. Not reproducible from
    /// anything the launcher can fetch, so it is never deleted and never
    /// overwritten.
    UserImported,
    /// Shipped by the project and pinned by the signed release, so it can
    /// always be fetched again.
    Shipped { component: String, version: String },
}

impl PackOrigin {
    /// Whether losing this would lose something nothing can get back.
    pub fn is_user_owned(&self) -> bool {
        matches!(self, PackOrigin::UserImported)
    }
}

/// Where one slot's audio comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrackSource {
    /// The song the disc shipped in this slot.
    ///
    /// Present as a source rather than an absence so a record can say "the user
    /// reset this slot on purpose", which is different information from "the
    /// user never touched this slot".
    Stock,
    /// A file the user imported, relative to `music/sources`.
    UserFile { relative_path: String },
    /// A stream inside a `.nsmp` pack, relative to `music/packs`.
    PackStream { pack_file: String, stream: u8 },
}

impl TrackSource {
    /// Whether the backing file is user data.
    ///
    /// Both `music/sources` and `music/packs` are classed
    /// [`DataDir::is_user_data`], and the layout document says neither is ever
    /// deleted, so a pack stream counts too: the user put that pack there and
    /// the launcher may not be able to fetch it again.
    pub fn is_user_owned(&self) -> bool {
        matches!(
            self,
            TrackSource::UserFile { .. } | TrackSource::PackStream { .. }
        )
    }

    /// The path this source reads, relative to its own root, if it reads one.
    pub fn relative_path(&self) -> Option<&str> {
        match self {
            TrackSource::Stock => None,
            TrackSource::UserFile { relative_path } => Some(relative_path),
            TrackSource::PackStream { pack_file, .. } => Some(pack_file),
        }
    }
}

/// One song in one slot.
///
/// There is no `slot` field. The slot is the key this is stored under in
/// [`TrackOrder`], and a number kept in two places is a number that eventually
/// disagrees with itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub source: TrackSource,
    /// Absent until a decoder measures it. Never zero as a stand-in, because a
    /// zero adds up into a total that reads as a fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<DurationMs>,
    /// Absent when no tag was read. Never the file name as a guess: the file
    /// name is what the user called it, not what the song is called, and the
    /// difference is visible on the in-game music screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
}

impl Track {
    /// A track with nothing known about it except where its audio is.
    pub fn from_source(source: TrackSource) -> Self {
        Self {
            source,
            duration: None,
            title: None,
            artist: None,
            album: None,
        }
    }

    pub fn with_duration(mut self, duration: DurationMs) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn with_metadata(
        mut self,
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
    ) -> Self {
        self.title = title;
        self.artist = artist;
        self.album = album;
        self
    }
}

// ---------------------------------------------------------------------------
// The order
// ---------------------------------------------------------------------------

/// Which track sits in which stream slot.
///
/// The slot number *is* the ordering: the game walks the position table, so
/// ascending slot order is play order. There is no second sequence field to
/// keep in step with it.
///
/// A slot outside the table cannot be represented. [`TrackOrder::assign`] is
/// the only way in and it refuses one, which is why nothing downstream has to
/// re-check. Deserialization is the exception, and that is what
/// [`TrackOrder::check`] is for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackOrder {
    slots: BTreeMap<u8, Track>,
}

impl TrackOrder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Put a track in a slot, returning whatever it replaced.
    ///
    /// Refuses a slot the table does not have, and refuses a pack stream index
    /// the table does not have. Both end up in `pathfind.dat` as positions, and
    /// a position past the end of the table plays garbage rather than raising
    /// anything the player can see.
    pub fn assign(
        &mut self,
        slot: u8,
        track: Track,
    ) -> std::result::Result<Option<Track>, MusicRefusal> {
        if !is_valid_slot(slot) {
            return Err(MusicRefusal::SlotOutOfRange { slot });
        }
        if let TrackSource::PackStream { pack_file, stream } = &track.source {
            if !is_valid_slot(*stream) {
                return Err(MusicRefusal::StreamOutOfRange {
                    stream: *stream,
                    pack_file: pack_file.clone(),
                });
            }
        }
        Ok(self.slots.insert(slot, track))
    }

    /// Drop a slot from the order entirely, so a rebuild leaves it as it
    /// shipped. Returns what was there.
    pub fn clear(&mut self, slot: u8) -> Option<Track> {
        self.slots.remove(&slot)
    }

    /// Exchange two slots, which is what a reorder is.
    ///
    /// Written as remove-then-insert rather than as two inserts, because a slot
    /// may be empty and an insert of an absent value would leave the track in
    /// both places.
    pub fn swap(&mut self, a: u8, b: u8) -> std::result::Result<(), MusicRefusal> {
        for slot in [a, b] {
            if !is_valid_slot(slot) {
                return Err(MusicRefusal::SlotOutOfRange { slot });
            }
        }
        if a == b {
            return Ok(());
        }
        let left = self.slots.remove(&a);
        let right = self.slots.remove(&b);
        if let Some(track) = right {
            self.slots.insert(a, track);
        }
        if let Some(track) = left {
            self.slots.insert(b, track);
        }
        Ok(())
    }

    pub fn get(&self, slot: u8) -> Option<&Track> {
        self.slots.get(&slot)
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Every assigned slot, in play order.
    pub fn tracks(&self) -> impl Iterator<Item = (u8, &Track)> {
        self.slots.iter().map(|(slot, track)| (*slot, track))
    }

    /// Slots whose audio the user supplied, in play order.
    pub fn user_tracks(&self) -> Vec<(u8, &Track)> {
        self.tracks()
            .filter(|(_, t)| t.source.is_user_owned())
            .collect()
    }

    /// Total length, with the unmeasured part reported separately.
    pub fn total_duration(&self) -> TotalDuration {
        let mut known = 0u64;
        let mut unmeasured = 0usize;
        for track in self.slots.values() {
            match track.duration {
                Some(d) => known = known.saturating_add(d.as_millis()),
                None => unmeasured += 1,
            }
        }
        TotalDuration {
            known: DurationMs::from_millis(known),
            unmeasured_tracks: unmeasured,
        }
    }

    /// Re-check the bounds that [`TrackOrder::assign`] enforces.
    ///
    /// Needed because deserialization bypasses `assign`, and a hand-edited or
    /// version-skewed record is exactly where an impossible slot number comes
    /// from.
    pub fn check(&self) -> std::result::Result<(), MusicRefusal> {
        for (slot, track) in self.tracks() {
            if !is_valid_slot(slot) {
                return Err(MusicRefusal::SlotOutOfRange { slot });
            }
            if let TrackSource::PackStream { pack_file, stream } = &track.source {
                if !is_valid_slot(*stream) {
                    return Err(MusicRefusal::StreamOutOfRange {
                        stream: *stream,
                        pack_file: pack_file.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// A named set of tracks with the order they go in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MusicPack {
    /// Stable identifier, used to reference the pack from a record.
    pub id: String,
    /// What the user sees.
    pub name: String,
    pub origin: PackOrigin,
    pub order: TrackOrder,
}

impl MusicPack {
    /// A pack the user built themselves.
    pub fn user_imported(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            origin: PackOrigin::UserImported,
            order: TrackOrder::new(),
        }
    }

    pub fn total_duration(&self) -> TotalDuration {
        self.order.total_duration()
    }

    /// Whether anything in here is the user's own audio, which is the question
    /// that decides whether a repair may replace this pack from the network.
    pub fn contains_user_audio(&self) -> bool {
        self.origin.is_user_owned() || !self.order.user_tracks().is_empty()
    }

    pub fn check(&self) -> std::result::Result<(), MusicRefusal> {
        self.order.check()
    }
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

/// A music operation that will not happen, and is not a fault.
///
/// Separate from [`Error`] for the same reason [`crate::session::Refusal`] is:
/// none of these is broken machinery, they are states, and each one has a
/// different thing to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicRefusal {
    /// Somebody tried to customize the online build.
    OnlineBuild,
    SlotOutOfRange {
        slot: u8,
    },
    StreamOutOfRange {
        stream: u8,
        pack_file: String,
    },
    /// A rebuild hash was asked for before a rebuild finished.
    NothingRebuilt,
}

impl MusicRefusal {
    /// A sentence that says what to do next.
    pub fn message(&self) -> String {
        match self {
            MusicRefusal::OnlineBuild => "Custom music applies to the offline game only. The \
                 online game stays exactly as shipped so matchmaking can verify \
                 it before a match. Switch to the offline game and try again."
                .to_string(),
            MusicRefusal::SlotOutOfRange { slot } => format!(
                "Slot {slot} does not exist: the soundtrack table holds \
                 {STREAM_SLOTS} slots, numbered 0 to {}. Pick a slot in range.",
                STREAM_SLOTS - 1
            ),
            MusicRefusal::StreamOutOfRange { stream, pack_file } => format!(
                "{pack_file} refers to stream {stream}, and a pack has \
                 {STREAM_SLOTS} streams, numbered 0 to {}. That pack was built \
                 for a different format version, so do not import it.",
                STREAM_SLOTS - 1
            ),
            MusicRefusal::NothingRebuilt => {
                "No rebuild has finished, so there is no image hash to \
                 record. Run the rebuild first: recording a hash the launcher \
                 did not produce would tell repair to trust a file nothing \
                 checked."
                    .to_string()
            }
        }
    }
}

/// Wrap a refusal for the call paths that have to return a [`Result`].
fn refused(refusal: MusicRefusal) -> Error {
    Error::PlainIo(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        refusal.message(),
    ))
}

/// Whether music may be applied to this build.
///
/// Delegates to [`GameBuild::is_customizable`] rather than re-listing the
/// builds, so this cannot drift away from the manifest's `customizable` field.
/// Two copies of that rule is one copy too many: the online image's whole value
/// is that it is byte-identical everywhere.
pub fn permitted_on(build: GameBuild) -> std::result::Result<(), MusicRefusal> {
    if build.is_customizable() {
        Ok(())
    } else {
        Err(MusicRefusal::OnlineBuild)
    }
}

// ---------------------------------------------------------------------------
// Paths, and the deletion guard
// ---------------------------------------------------------------------------

/// The `music` directory itself.
///
/// Derived from [`DataDir::MusicSources`] rather than spelled out again,
/// because a second copy of a path is a second thing to keep correct.
pub fn music_root(data_root: &Path) -> PathBuf {
    let rel = DataDir::MusicSources.relative();
    let head = rel.split('/').next().unwrap_or(rel);
    data_root.join(head)
}

/// Where the user's imported files live.
pub fn sources_dir(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::MusicSources.relative())
}

/// Where `.nsmp` packs live.
pub fn packs_dir(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::MusicPacks.relative())
}

/// The soundtrack record.
pub fn soundtrack_path(data_root: &Path) -> PathBuf {
    music_root(data_root).join(SOUNDTRACK_FILE)
}

/// The directories this subsystem touches.
pub const MUSIC_DIRS: &[DataDir] = &[
    DataDir::MusicSources,
    DataDir::MusicPacks,
    DataDir::LibraryWork,
];

/// Whether a repair may delete the contents of this directory.
///
/// Answered from [`DataDir`]'s own classification rather than from a
/// hand-written allow list here. A hand-written list stays true only until
/// somebody adds a directory to it, and that failure is silent, permanent, and
/// discovered by a user who has lost files.
pub fn repair_may_purge(dir: DataDir) -> bool {
    dir.is_derived() && !dir.is_user_data()
}

/// Refuse a deletion that would land in, or contain, files the user supplied.
///
/// This lives in the music module because the music rebuild is the thing that
/// deletes a derived tree, `library/work`, which sits one directory away from
/// `music/sources`. It guards every [`DataDir::is_user_data`] directory rather
/// than only the two music ones, because the failure is generic: the shipped
/// 2.1 uninstaller runs a recursive delete over a folder containing the
/// player's memory card, and the reason that is possible is that nothing ever
/// asks this question before deleting.
///
/// Three ways a plain string prefix check gets this wrong, all handled here:
///
/// - `library/work/../music/sources/song.mp3` does not begin with
///   `music/sources` as text, and lands there anyway.
/// - `MUSIC/SOURCES/song.mp3` is different text and the same file on Windows.
///   The comparison ignores case on every platform, because refusing one extra
///   deletion costs nothing and deleting somebody's music costs everything.
/// - The data root *contains* `music/sources`, so deleting an ancestor has to
///   be refused too. That is the 2.1 bug exactly.
pub fn guard_deletion(data_root: &Path, target: &Path) -> Result<()> {
    if target
        .components()
        .any(|c| matches!(c, PathComponent::ParentDir))
    {
        return Err(Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "refusing to delete {}: the path contains \"..\", so where it \
                 actually lands cannot be decided by reading it. Resolve the \
                 path first, then ask again.",
                target.display()
            ),
        )));
    }

    for dir in DataDir::ALL.iter().copied().filter(|d| d.is_user_data()) {
        let protected = data_root.join(dir.relative());
        let inside = starts_with_ci(target, &protected);
        let ancestor = starts_with_ci(&protected, target);
        if inside || ancestor {
            let relation = if inside { "is inside" } else { "contains" };
            return Err(Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "refusing to delete {}: it {relation} {}, which holds files \
                     the user supplied and the launcher cannot fetch again. \
                     Delete {} instead, which is derived and rebuilds itself.",
                    target.display(),
                    protected.display(),
                    DataDir::LibraryWork.relative()
                ),
            )));
        }
    }
    Ok(())
}

/// Whether `path` is at or below `prefix`, comparing whole components and
/// ignoring ASCII case.
fn starts_with_ci(path: &Path, prefix: &Path) -> bool {
    let mut walk = path.components();
    for wanted in prefix.components() {
        match walk.next() {
            Some(got) if component_eq_ci(got, wanted) => {}
            _ => return false,
        }
    }
    true
}

fn component_eq_ci(a: PathComponent<'_>, b: PathComponent<'_>) -> bool {
    let a = a.as_os_str().to_string_lossy();
    let b = b.as_os_str().to_string_lossy();
    a.eq_ignore_ascii_case(&b)
}

/// Whether a recorded relative path stays inside the directory it is relative
/// to. The same rule as [`crate::manifest::Component::traversal_offenders`],
/// applied to a record the launcher wrote itself, because a record on disk can
/// be edited by anything.
fn is_contained_relative(relative: &str) -> bool {
    !(relative.is_empty()
        || relative.starts_with('/')
        || relative.starts_with('\\')
        || relative.contains(':')
        || relative.split(['/', '\\']).any(|seg| seg == ".."))
}

fn resolve(root: &Path, relative: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for segment in relative.split(['/', '\\']).filter(|s| !s.is_empty()) {
        path.push(segment);
    }
    path
}

// ---------------------------------------------------------------------------
// Imported sources
// ---------------------------------------------------------------------------

/// A file the user dropped into `music/sources`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSource {
    /// Path relative to `music/sources`, always with forward slashes so the
    /// same record reads the same way on every platform.
    pub relative_path: String,
    pub bytes: u64,
    /// Always absent from a scan.
    ///
    /// Length needs a decoder, which is the bundled FFmpeg and not this crate.
    /// The field exists so a caller that has measured one can fill it in, and it
    /// is `None` rather than `0` so nothing renders a running time nobody
    /// measured.
    pub duration: Option<DurationMs>,
}

/// Whether this looks like audio the chain can open.
pub fn is_audio_source(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let lower = ext.to_ascii_lowercase();
            AUDIO_EXTENSIONS.contains(&lower.as_str())
        }
        None => false,
    }
}

/// List the user's imported audio, recursively, sorted by path.
///
/// An absent directory is an empty list rather than an error: nothing imported
/// yet is the normal state of a fresh install, and failing here would make the
/// Music page unopenable before it has ever been used.
pub fn scan_sources(sources: &Path) -> Result<Vec<ImportedSource>> {
    let mut found = Vec::new();
    let mut stack = vec![(sources.to_path_buf(), String::new())];

    while let Some((dir, prefix)) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(Error::Io { path: dir, source }),
        };
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io {
                path: dir.clone(),
                source,
            })?;
            let name = entry.file_name().to_string_lossy().to_string();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let meta = entry.metadata().map_err(|source| Error::Io {
                path: entry.path(),
                source,
            })?;
            if meta.is_dir() {
                stack.push((entry.path(), relative));
            } else if is_audio_source(&entry.path()) {
                found.push(ImportedSource {
                    relative_path: relative,
                    bytes: meta.len(),
                    duration: None,
                });
            }
        }
    }

    found.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(found)
}

// ---------------------------------------------------------------------------
// The soundtrack record
// ---------------------------------------------------------------------------

/// What the launcher applied, to what, and what came out.
///
/// This is the record that makes a customized install distinguishable from a
/// corrupted one. Without it, a modified offline image is only "not the shipped
/// hash", which is indistinguishable from a bad download, and repair re-fetches
/// over the user's soundtrack. See [`crate::library`].
///
/// It also survives a content update, which is why the order is stored as
/// intents (a slot, and where its audio lives) rather than as byte diffs.
/// Intents replay against a new base. Byte diffs would not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundtrackState {
    /// The `offline-content` version the order was applied to.
    pub base_version: String,
    /// The artifact the work tree was extracted from, so a base change is
    /// detected rather than assumed.
    pub base_artifact_sha256: Sha256Hex,
    pub order: TrackOrder,
    /// SHA-256 of the image the last rebuild produced.
    ///
    /// Absent until a rebuild has actually finished. Absent is the honest state
    /// for "edited but not built yet", and it is what stops a half-finished
    /// edit claiming an image that does not exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebuilt_sha256: Option<Sha256Hex>,
}

impl SoundtrackState {
    /// A record with an order and no rebuild yet.
    pub fn new(base_version: impl Into<String>, base_artifact_sha256: Sha256Hex) -> Self {
        Self {
            base_version: base_version.into(),
            base_artifact_sha256,
            order: TrackOrder::new(),
            rebuilt_sha256: None,
        }
    }

    /// Load the record, or `None` when there is not one.
    ///
    /// `None` means no custom soundtrack, which is a state and not a failure. A
    /// malformed file *is* a failure and is reported: quietly treating it as
    /// absent would leave a customized image with nothing to explain it, and the
    /// next repair would overwrite the soundtrack. Better to stop and say so.
    pub fn load(path: &Path) -> Result<Option<Self>> {
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
        let state: Self = serde_json::from_slice(&bytes).map_err(|e| Error::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;
        state.order.check().map_err(refused)?;
        Ok(Some(state))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.order.check().map_err(refused)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(e),
        })?;
        std::fs::write(path, bytes).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Whether this record was applied to the artifact now installed.
    ///
    /// A `false` here does not mean anything is wrong. It means the base moved
    /// under the soundtrack, so the order needs replaying, which is a background
    /// task and not an error.
    pub fn matches_base(&self, artifact: &Sha256Hex) -> bool {
        self.base_artifact_sha256 == *artifact
    }

    /// Tell [`crate::library`] that the modified image is the user's work, not
    /// damage.
    ///
    /// This is the handshake that makes `FileVerdict::UserModified` reachable at
    /// all. A rebuild that skips it leaves an image nothing can explain, and the
    /// next repair re-fetches over the soundtrack.
    ///
    /// Refuses when no rebuild has finished. Recording a hash the launcher did
    /// not produce would tell repair to trust a file nothing verified, which
    /// turns this safety mechanism into a way to hide real corruption.
    pub fn record_into(&self, rebuild: &mut RebuildState, image_relative: &str) -> Result<()> {
        let Some(hash) = self.rebuilt_sha256.clone() else {
            return Err(refused(MusicRefusal::NothingRebuilt));
        };
        rebuild.record(image_relative, hash);
        Ok(())
    }

    /// Check every slot's backing file, without touching any of them.
    pub fn verify(&self, data_root: &Path) -> Result<MusicReport> {
        let sources = sources_dir(data_root);
        let packs = packs_dir(data_root);
        let mut verdicts = BTreeMap::new();
        for (slot, track) in self.order.tracks() {
            verdicts.insert(slot, verify_source(&sources, &packs, &track.source)?);
        }
        Ok(MusicReport { verdicts })
    }
}

fn verify_source(sources: &Path, packs: &Path, source: &TrackSource) -> Result<SlotVerdict> {
    let (root, relative) = match source {
        TrackSource::Stock => return Ok(SlotVerdict::Stock),
        TrackSource::UserFile { relative_path } => (sources, relative_path),
        TrackSource::PackStream { pack_file, .. } => (packs, pack_file),
    };

    if !is_contained_relative(relative) {
        return Ok(SlotVerdict::UnsafePath {
            relative_path: relative.clone(),
        });
    }

    let path = resolve(root, relative);
    match std::fs::metadata(&path) {
        Ok(_) => Ok(SlotVerdict::Present {
            source: source.clone(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SlotVerdict::SourceMissing {
            relative_path: relative.clone(),
        }),
        Err(source) => Err(Error::Io { path, source }),
    }
}

/// What verify decided about one slot.
///
/// Nothing here is a verdict to delete anything, and there is deliberately no
/// variant that could be read as one. The worst thing a missing file produces is
/// a slot that is skipped and named: the file the user has misplaced is the file
/// they may still find, and a rebuild that quietly drops the row destroys the
/// only record of what they wanted there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotVerdict {
    /// Reset to the disc song on purpose.
    Stock,
    /// The backing file is where the record says it is.
    Present { source: TrackSource },
    /// The record points at a file that is not there.
    SourceMissing { relative_path: String },
    /// The record points outside the music directory, so it was not followed.
    UnsafePath { relative_path: String },
}

impl SlotVerdict {
    /// Whether this slot can be applied to a fresh base.
    pub fn is_replayable(&self) -> bool {
        matches!(self, SlotVerdict::Stock | SlotVerdict::Present { .. })
    }

    /// Why this slot will be skipped, if it will be.
    pub fn skip_reason(&self) -> Option<String> {
        match self {
            SlotVerdict::Stock | SlotVerdict::Present { .. } => None,
            SlotVerdict::SourceMissing { relative_path } => Some(format!(
                "{relative_path} is not in {}. The slot keeps the song it has \
                 until that file is put back.",
                DataDir::MusicSources.relative()
            )),
            SlotVerdict::UnsafePath { relative_path } => Some(format!(
                "{relative_path} points outside {}, so it was not read. Import \
                 the file again.",
                DataDir::MusicSources.relative()
            )),
        }
    }
}

/// The outcome of verifying a whole soundtrack record.
#[derive(Debug, Clone)]
pub struct MusicReport {
    pub verdicts: BTreeMap<u8, SlotVerdict>,
}

impl MusicReport {
    /// Whether every slot can be replayed onto a new base.
    pub fn is_fully_replayable(&self) -> bool {
        self.verdicts.values().all(SlotVerdict::is_replayable)
    }

    /// Slots that will be skipped, with the reason for each.
    ///
    /// Returned so the caller can list them. A skipped slot is listed and never
    /// silently dropped, which is the difference between a user who knows one
    /// song is missing and a user who thinks the launcher lost their soundtrack.
    pub fn skipped(&self) -> Vec<(u8, String)> {
        self.verdicts
            .iter()
            .filter_map(|(slot, v)| v.skip_reason().map(|why| (*slot, why)))
            .collect()
    }

    /// Slots whose audio is present and belongs to the user.
    pub fn present_user_slots(&self) -> Vec<u8> {
        self.verdicts
            .iter()
            .filter(|(_, v)| match v {
                SlotVerdict::Present { source } => source.is_user_owned(),
                _ => false,
            })
            .map(|(slot, _)| *slot)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// The two typed gaps
// ---------------------------------------------------------------------------

/// Named as a constant so the error and its test read the same string.
const WRITER_GAP: &str =
    "writing the pathfind.dat stream table natively. Three things are missing \
     and none of them is recorded anywhere in this repository: the per-stream \
     position entry layout inside pathfind.dat, the EA-exact sample accounting \
     that extend_slot_40big.py performs when a slot grows, and the title, artist \
     and album record layout in template.dat. Guessing any of them yields a disc \
     that boots and plays silence, which is worse than a crash because it looks \
     like success. Until they are documented, drive the proven chain instead: \
     extend_slot_40big.py, then fix_pfdx_toc.py, then edit_music_meta.py, then \
     one repack. This";

/// Named as a constant so the error and its test read the same string.
const READER_GAP: &str = "parsing a .nsmp pack. The format 1 container is known here only by its \
     magic, NSMP, and by its 40-stream validation; the stream table offsets, the \
     per-stream headers and the metadata block have not been read. A misparse \
     splices audio at the wrong offset and produces noise rather than an error. \
     Until the layout is read out of music_pack.py, import a pack by calling \
     that tool and building a MusicPack from what it reports. This";

/// Write an order into an extracted game tree.
///
/// Always refuses, and the message names what is missing and what to do
/// instead. The signature exists so callers can be written against it and so
/// the gap is visible in the API rather than absent from it: a missing writer
/// that is simply not there becomes a caller that quietly does nothing.
///
/// On success this would return the SHA-256 of the image the rebuild produced,
/// which is the value [`SoundtrackState::rebuilt_sha256`] needs. It never
/// returns a placeholder hash, because a placeholder recorded through
/// [`SoundtrackState::record_into`] would excuse a file nothing wrote.
pub fn write_soundtrack(_work_tree: &Path, _order: &TrackOrder) -> Result<Sha256Hex> {
    Err(Error::NotImplemented {
        operation: WRITER_GAP,
        platform: crate::platform::CURRENT_PLATFORM,
    })
}

/// Read a `.nsmp` pack from disk.
///
/// Always refuses. See [`READER_GAP`] for what is missing.
pub fn read_pack(_path: &Path) -> Result<MusicPack> {
    Err(Error::NotImplemented {
        operation: READER_GAP,
        platform: crate::platform::CURRENT_PLATFORM,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{FileVerdict, Verifier};
    use crate::manifest::{Component, ManifestFile, COMPONENT_SCHEMA};

    fn user_track(name: &str) -> Track {
        Track::from_source(TrackSource::UserFile {
            relative_path: name.to_string(),
        })
    }

    // -- the order ---------------------------------------------------------

    /// A slot the table cannot hold would be written into pathfind.dat as a
    /// position past the end of the table, which plays garbage instead of
    /// failing. It is refused at the only door in, and never clamped, because a
    /// clamped slot silently overwrites a different song.
    #[test]
    fn a_slot_outside_the_table_is_refused_and_never_clamped() {
        let mut order = TrackOrder::new();
        assert!(order
            .assign(STREAM_SLOTS - 1, user_track("last.mp3"))
            .is_ok());

        let err = order
            .assign(STREAM_SLOTS, user_track("past-the-end.mp3"))
            .expect_err("slot 40 does not exist");
        assert_eq!(err, MusicRefusal::SlotOutOfRange { slot: STREAM_SLOTS });
        assert_eq!(order.len(), 1, "a refused assign must not land anywhere");
        assert!(order.get(STREAM_SLOTS).is_none());
        assert_eq!(
            order
                .get(STREAM_SLOTS - 1)
                .and_then(|t| t.source.relative_path()),
            Some("last.mp3"),
            "and must not have overwritten a real slot"
        );
    }

    /// The same rule for a pack's own stream index. A pack built for a later
    /// format must be refused on import, not spliced at an offset nothing has.
    #[test]
    fn a_pack_stream_outside_the_table_is_refused() {
        let mut order = TrackOrder::new();
        let err = order
            .assign(
                0,
                Track::from_source(TrackSource::PackStream {
                    pack_file: "future.nsmp".into(),
                    stream: STREAM_SLOTS,
                }),
            )
            .expect_err("stream 40 does not exist");
        assert!(matches!(err, MusicRefusal::StreamOutOfRange { .. }));
        assert!(order.is_empty());
    }

    /// Slot order is play order, and a reorder must move both tracks rather
    /// than overwrite one of them.
    #[test]
    fn a_swap_moves_both_tracks_and_loses_neither() {
        let mut order = TrackOrder::new();
        order.assign(0, user_track("a.mp3")).unwrap();
        order.assign(3, user_track("b.mp3")).unwrap();
        order.swap(0, 3).unwrap();

        let names: Vec<&str> = order
            .tracks()
            .filter_map(|(_, t)| t.source.relative_path())
            .collect();
        assert_eq!(names, vec!["b.mp3", "a.mp3"], "play order is slot order");
        assert_eq!(order.len(), 2, "a swap must not drop a track");
    }

    /// Swapping into an empty slot has to move the track, not leave a copy of
    /// it behind in both places.
    #[test]
    fn a_swap_with_an_empty_slot_moves_rather_than_copies() {
        let mut order = TrackOrder::new();
        order.assign(1, user_track("only.mp3")).unwrap();
        order.swap(1, 2).unwrap();

        assert!(order.get(1).is_none(), "the source slot must be emptied");
        assert_eq!(
            order.get(2).and_then(|t| t.source.relative_path()),
            Some("only.mp3")
        );
        assert_eq!(order.len(), 1);
    }

    // -- duration ----------------------------------------------------------

    /// An unmeasured track must not contribute zero to a total, because a total
    /// that silently omits three songs still renders as a fact.
    #[test]
    fn a_total_never_invents_a_length_for_an_unmeasured_track() {
        let mut order = TrackOrder::new();
        order
            .assign(
                0,
                user_track("measured.mp3").with_duration(DurationMs::from_millis(185_000)),
            )
            .unwrap();
        order.assign(1, user_track("unknown-a.mp3")).unwrap();
        order.assign(2, user_track("unknown-b.mp3")).unwrap();

        let total = order.total_duration();
        assert_eq!(total.known.as_millis(), 185_000);
        assert_eq!(total.unmeasured_tracks, 2);
        assert!(!total.is_exact());

        let text = total.describe();
        assert!(text.contains("3:05"), "measured part missing: {text}");
        assert!(text.contains("unknown"), "the gap is hidden: {text}");
        assert!(text.contains('2'), "the count is missing: {text}");
    }

    #[test]
    fn a_fully_measured_total_reads_as_a_plain_running_time() {
        let mut order = TrackOrder::new();
        order
            .assign(
                0,
                user_track("a.mp3").with_duration(DurationMs::from_millis(3_600_000)),
            )
            .unwrap();
        let total = order.total_duration();
        assert!(total.is_exact());
        assert_eq!(total.describe(), "1:00:00");
    }

    // -- imported sources --------------------------------------------------

    /// A scan cannot measure a length, so it must report absence. A zero here
    /// would appear in the interface as a real running time.
    #[test]
    fn a_scan_reports_no_duration_rather_than_zero() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("song.mp3"), b"not really audio").unwrap();
        std::fs::create_dir_all(dir.path().join("album")).unwrap();
        std::fs::write(dir.path().join("album").join("two.flac"), b"x").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();

        let found = scan_sources(dir.path()).unwrap();
        let paths: Vec<&str> = found.iter().map(|s| s.relative_path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["album/two.flac", "song.mp3"],
            "a non-audio file must not be offered as a track"
        );
        assert!(
            found.iter().all(|s| s.duration.is_none()),
            "a scan has decoded nothing and must not claim a length"
        );
        assert_eq!(found[1].bytes, 16);
    }

    #[test]
    fn an_absent_sources_directory_is_an_empty_list_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("never-created");
        assert!(scan_sources(&missing).unwrap().is_empty());
    }

    // -- the deletion guard ------------------------------------------------

    /// The classification decides what may be purged, not a list written here.
    #[test]
    fn repair_may_never_purge_a_directory_that_holds_user_files() {
        assert!(!repair_may_purge(DataDir::MusicSources));
        assert!(!repair_may_purge(DataDir::MusicPacks));
        assert!(
            repair_may_purge(DataDir::LibraryWork),
            "the derived work tree must be purgeable or a base change deadlocks"
        );
        for dir in DataDir::ALL.iter().copied() {
            if repair_may_purge(dir) {
                assert!(
                    !dir.is_user_data(),
                    "{dir:?} is purgeable and holds user data"
                );
            }
        }
        for dir in MUSIC_DIRS.iter().copied() {
            assert!(
                dir.is_user_data() || dir.is_derived(),
                "{dir:?} is classed as neither, so nothing knows if it is safe"
            );
        }
    }

    #[test]
    fn a_deletion_inside_the_users_music_is_refused() {
        let root = Path::new("/data");
        for dir in [DataDir::MusicSources, DataDir::MusicPacks] {
            let target = root.join(dir.relative()).join("song.mp3");
            let err = guard_deletion(root, &target)
                .expect_err("deleting the user's own music must be refused");
            let msg = err.to_string();
            assert!(msg.contains("refusing to delete"), "{msg}");
            assert!(
                msg.contains(DataDir::LibraryWork.relative()),
                "no way forward given: {msg}"
            );
        }
    }

    /// The sneaky one. A prefix check on the text never sees this coming, and
    /// the delete lands on the user's music anyway.
    #[test]
    fn a_deletion_that_escapes_through_dot_dot_is_refused() {
        let root = Path::new("/data");
        let target = root
            .join(DataDir::LibraryWork.relative())
            .join("..")
            .join("..")
            .join("music")
            .join("sources")
            .join("song.mp3");
        let err = guard_deletion(root, &target).expect_err("a traversal must be refused");
        assert!(err.to_string().contains(".."), "{err}");
    }

    /// Windows would delete the real file. A case sensitive comparison waves
    /// this straight through.
    #[test]
    fn a_case_different_spelling_of_the_music_path_is_still_refused() {
        let root = Path::new("/data");
        assert!(
            guard_deletion(root, Path::new("/data/MUSIC/SOURCES/song.mp3")).is_err(),
            "on Windows this is the same file as music/sources/song.mp3"
        );
        assert!(guard_deletion(root, Path::new("/data/Music/Packs")).is_err());
    }

    /// The 2.1 uninstaller bug, exactly: a recursive delete of an ancestor of
    /// the directory holding the player's own files.
    #[test]
    fn deleting_an_ancestor_of_user_data_is_refused() {
        let root = Path::new("/data");
        assert!(
            guard_deletion(root, root).is_err(),
            "deleting the data root takes the memory cards with it"
        );
        assert!(
            guard_deletion(root, &music_root(root)).is_err(),
            "deleting music/ takes sources and packs with it"
        );
        assert!(
            guard_deletion(root, &root.join(DataDir::Saves.relative())).is_err(),
            "the guard covers every user-data directory, not only music"
        );
    }

    /// A positive control. A guard that refuses everything is not a guard, it is
    /// a broken purge, and the work tree has to stay deletable or a base change
    /// can never be resolved.
    #[test]
    fn the_derived_work_tree_may_still_be_deleted() {
        let root = Path::new("/data");
        guard_deletion(root, &root.join(DataDir::LibraryWork.relative()))
            .expect("library/work is derived and must be purgeable");
        guard_deletion(
            root,
            &root.join(DataDir::LibraryWork.relative()).join("files"),
        )
        .expect("and so must anything inside it");
    }

    // -- the handshake with library repair ---------------------------------

    fn offline_component(file: ManifestFile) -> Component {
        Component {
            schema: COMPONENT_SCHEMA.to_string(),
            component: "offline-content".into(),
            version: "2.4.0".into(),
            sequence: 7,
            install_root: DataDir::LibraryOffline.relative().into(),
            customizable: true,
            files: vec![file],
        }
    }

    /// THE ONE THAT MATTERS, and the reason this module exists at all.
    ///
    /// A rebuilt image never matches the shipped hash. If music does not record
    /// what it produced, library repair classes it Damaged and re-fetches 1.5 GB
    /// over the user's soundtrack.
    #[test]
    fn a_recorded_rebuild_reads_as_user_modified_to_library_repair() {
        let dir = tempfile::tempdir().unwrap();
        let shipped = b"the shipped offline image";
        std::fs::write(dir.path().join("NSR.iso"), shipped).unwrap();
        let manifest_file = ManifestFile {
            path: "NSR.iso".into(),
            bytes: shipped.len() as u64,
            sha256: Sha256Hex::from_bytes(shipped),
        };

        // The user changes a song, and the launcher rebuilds.
        let rebuilt = b"the offline image with my music in it";
        std::fs::write(dir.path().join("NSR.iso"), rebuilt).unwrap();

        let mut state = SoundtrackState::new("2.4.0", Sha256Hex::from_bytes(shipped));
        state.order.assign(0, user_track("intro.mp3")).unwrap();
        state.rebuilt_sha256 = Some(Sha256Hex::from_bytes(rebuilt));

        let mut rebuild = RebuildState::default();
        state.record_into(&mut rebuild, "NSR.iso").unwrap();

        let component = offline_component(manifest_file);
        let report = Verifier::new(dir.path(), &rebuild)
            .verify(&component)
            .unwrap();

        assert_eq!(report.verdicts["NSR.iso"], FileVerdict::UserModified);
        assert!(report.is_healthy(), "custom music is not a fault");
        assert!(
            report.to_repair().is_empty(),
            "repair must not touch a rebuilt image"
        );
        assert_eq!(report.repair_bytes(&component), 0);
    }

    /// Recording a hash nothing produced would tell repair to trust a file
    /// nothing verified, which turns the exemption into a way to hide real
    /// corruption.
    #[test]
    fn recording_a_rebuild_that_never_finished_is_refused() {
        let state = SoundtrackState::new("2.4.0", Sha256Hex::from_bytes(b"base"));
        assert!(state.rebuilt_sha256.is_none());

        let mut rebuild = RebuildState::default();
        let err = state
            .record_into(&mut rebuild, "NSR.iso")
            .expect_err("there is no hash to record");
        assert!(err.to_string().contains("Run the rebuild first"), "{err}");
        assert!(
            rebuild.rebuilt.is_empty(),
            "a refused record must not leave a half-written exemption"
        );
    }

    /// The writer is the only thing that could produce that hash, and it does
    /// not exist yet, so nothing can accidentally have recorded one.
    #[test]
    fn the_byte_level_writer_never_hands_back_a_hash() {
        let err = write_soundtrack(Path::new("/work"), &TrackOrder::new())
            .expect_err("the table layout is not known");
        assert!(matches!(err, Error::NotImplemented { .. }), "{err:?}");
        let msg = err.to_string();
        for expected in [
            "pathfind.dat",
            "template.dat",
            "extend_slot_40big.py",
            "fix_pfdx_toc.py",
            "edit_music_meta.py",
        ] {
            assert!(msg.contains(expected), "{expected} not named in: {msg}");
        }
    }

    #[test]
    fn the_pack_reader_refuses_and_names_the_missing_format() {
        let err = read_pack(Path::new("/packs/x.nsmp")).expect_err("the container is not parsed");
        assert!(matches!(err, Error::NotImplemented { .. }), "{err:?}");
        let msg = err.to_string();
        assert!(msg.contains("NSMP"), "{msg}");
        assert!(msg.contains("music_pack.py"), "{msg}");
    }

    // -- verify never deletes ---------------------------------------------

    /// A missing source is named and skipped. The row stays in the record,
    /// because the file the user has misplaced is the file they may still find,
    /// and dropping the row destroys the only note of what they wanted there.
    #[test]
    fn a_missing_source_is_listed_and_skipped_and_never_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let sources = sources_dir(dir.path());
        std::fs::create_dir_all(&sources).unwrap();
        std::fs::write(sources.join("here.mp3"), b"audio").unwrap();

        let mut state = SoundtrackState::new("2.4.0", Sha256Hex::from_bytes(b"base"));
        state.order.assign(0, user_track("here.mp3")).unwrap();
        state.order.assign(1, user_track("gone.mp3")).unwrap();

        let report = state.verify(dir.path()).unwrap();
        assert_eq!(
            report.verdicts[&0],
            SlotVerdict::Present {
                source: TrackSource::UserFile {
                    relative_path: "here.mp3".into()
                }
            }
        );
        assert!(matches!(
            report.verdicts[&1],
            SlotVerdict::SourceMissing { .. }
        ));
        assert!(!report.is_fully_replayable());

        let skipped = report.skipped();
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].0, 1);
        assert!(skipped[0].1.contains("gone.mp3"), "{:?}", skipped[0]);

        assert_eq!(state.order.len(), 2, "verify must not edit the record");
        assert!(
            state.order.get(1).is_some(),
            "the slot whose file is lost must survive, or the intent is gone"
        );
        assert!(
            sources.join("here.mp3").exists(),
            "verify must never delete a user file"
        );
        assert_eq!(report.present_user_slots(), vec![0]);
    }

    /// A record that points outside the music directory is not followed. It is
    /// reported, and the file it names is neither read nor touched.
    #[test]
    fn a_recorded_path_that_escapes_the_music_directory_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(sources_dir(dir.path())).unwrap();

        let mut state = SoundtrackState::new("2.4.0", Sha256Hex::from_bytes(b"base"));
        state
            .order
            .assign(0, user_track("../../saves/MemoryCardA.USA.raw"))
            .unwrap();

        let report = state.verify(dir.path()).unwrap();
        assert!(matches!(
            report.verdicts[&0],
            SlotVerdict::UnsafePath { .. }
        ));
        assert!(!report.is_fully_replayable());
        assert_eq!(report.skipped().len(), 1);
    }

    // -- the record on disk ------------------------------------------------

    #[test]
    fn the_record_round_trips_and_absence_means_no_custom_music() {
        let dir = tempfile::tempdir().unwrap();
        let path = soundtrack_path(dir.path());
        assert!(
            SoundtrackState::load(&path).unwrap().is_none(),
            "no record means no custom soundtrack, not a failure"
        );

        let mut state = SoundtrackState::new("2.4.0", Sha256Hex::from_bytes(b"base"));
        state
            .order
            .assign(
                7,
                user_track("track.mp3")
                    .with_duration(DurationMs::from_millis(200_000))
                    .with_metadata(Some("Title".into()), Some("Artist".into()), None),
            )
            .unwrap();
        state.rebuilt_sha256 = Some(Sha256Hex::from_bytes(b"built"));
        state.save(&path).unwrap();

        let loaded = SoundtrackState::load(&path).unwrap().expect("a record");
        assert_eq!(loaded, state, "the order is the thing that must survive");
        assert_eq!(
            loaded.order.get(7).unwrap().album,
            None,
            "an absent field must not come back as an empty string"
        );
        assert!(loaded.matches_base(&Sha256Hex::from_bytes(b"base")));
        assert!(
            !loaded.matches_base(&Sha256Hex::from_bytes(b"a newer artifact")),
            "a moved base must be detectable, or the order replays onto the wrong image"
        );
    }

    /// Deserialization does not go through `assign`, so a hand-edited or
    /// version-skewed file is where an impossible slot arrives from. It has to
    /// be refused at load rather than carried into a rebuild.
    #[test]
    fn a_record_naming_a_slot_outside_the_table_is_refused_at_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SOUNDTRACK_FILE);

        let mut raw = serde_json::json!({
            "base_version": "2.4.0",
            "base_artifact_sha256": Sha256Hex::from_bytes(b"base").as_str(),
            "order": {},
        });
        raw["order"][STREAM_SLOTS.to_string()] = serde_json::json!({
            "source": {"kind": "user_file", "relative_path": "x.mp3"}
        });
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();

        let err = SoundtrackState::load(&path).expect_err("slot 40 does not exist");
        assert!(err.to_string().contains("does not exist"), "{err}");
    }

    /// `music/state.json` belongs to `library::RebuildState`, whose `save`
    /// writes only its own field. Sharing one file means whichever subsystem
    /// saves second silently truncates the other's record, and losing the
    /// soundtrack record loses the proof that a modified image is the user's.
    #[test]
    fn the_soundtrack_record_is_not_written_to_the_file_the_library_owns() {
        let dir = tempfile::tempdir().unwrap();
        let ours = soundtrack_path(dir.path());
        let theirs = music_root(dir.path()).join("state.json");
        assert_ne!(ours, theirs, "the two records must not share a file");

        let mut state = SoundtrackState::new("2.4.0", Sha256Hex::from_bytes(b"base"));
        state.order.assign(0, user_track("mine.mp3")).unwrap();
        state.save(&ours).unwrap();

        let mut rebuild = RebuildState::default();
        rebuild.record("NSR.iso", Sha256Hex::from_bytes(b"built"));
        rebuild.save(&theirs).unwrap();

        let reloaded = SoundtrackState::load(&ours).unwrap().expect("still there");
        assert_eq!(reloaded.order.len(), 1, "the order survived the other save");
        assert_eq!(
            RebuildState::load(&theirs).unwrap().rebuilt.len(),
            1,
            "and the library's own record is intact"
        );
    }

    // -- online ------------------------------------------------------------

    /// Music is offline only, and that is what lets matchmaking trust the online
    /// image. Refusing here rather than in each caller means one place can
    /// forget instead of every place.
    #[test]
    fn music_is_refused_on_the_online_build() {
        permitted_on(GameBuild::Offline).expect("the offline build is customizable");

        let refusal = permitted_on(GameBuild::Online).expect_err("online must be refused");
        assert_eq!(refusal, MusicRefusal::OnlineBuild);
        let msg = refusal.message();
        assert!(msg.contains("offline"), "{msg}");
        assert!(msg.contains("matchmaking"), "{msg}");
        assert!(
            !GameBuild::Online.is_customizable(),
            "this module's rule is the runtime's rule, not a second copy of it"
        );
    }

    #[test]
    fn every_refusal_says_something_actionable() {
        for r in [
            MusicRefusal::OnlineBuild,
            MusicRefusal::SlotOutOfRange { slot: 99 },
            MusicRefusal::StreamOutOfRange {
                stream: 99,
                pack_file: "p.nsmp".into(),
            },
            MusicRefusal::NothingRebuilt,
        ] {
            let m = r.message();
            assert!(m.len() > 20, "too terse to act on: {m}");
            assert!(m.ends_with('.'), "not a sentence: {m}");
            assert!(!m.contains('\u{2014}'), "no em dashes: {m}");
        }
    }

    // -- ownership ---------------------------------------------------------

    /// A pack the user built is not replaceable from the network, and nothing
    /// may treat it as though it were.
    #[test]
    fn a_user_imported_pack_is_never_treated_as_replaceable() {
        let mut mine = MusicPack::user_imported("mine", "My Mix");
        mine.order.assign(0, user_track("a.mp3")).unwrap();
        assert!(mine.origin.is_user_owned());
        assert!(mine.contains_user_audio());
        mine.check()
            .expect("a pack built through assign is in range");

        let shipped = MusicPack {
            id: "reborn-2026".into(),
            name: "Reborn Selection".into(),
            origin: PackOrigin::Shipped {
                component: "music-pack".into(),
                version: "1.0.0".into(),
            },
            order: TrackOrder::new(),
        };
        assert!(!shipped.origin.is_user_owned());
        assert!(!shipped.contains_user_audio());
    }

    /// Both music directories are user data, so a stream out of a pack counts as
    /// the user's audio just as much as a loose file does.
    #[test]
    fn a_pack_stream_counts_as_the_users_audio_just_as_a_loose_file_does() {
        assert!(TrackSource::UserFile {
            relative_path: "a.mp3".into()
        }
        .is_user_owned());
        assert!(TrackSource::PackStream {
            pack_file: "p.nsmp".into(),
            stream: 3
        }
        .is_user_owned());
        assert!(!TrackSource::Stock.is_user_owned());
        assert!(DataDir::MusicSources.is_user_data());
        assert!(DataDir::MusicPacks.is_user_data());
    }

    /// The slot count is a documented fact with two independent sources in this
    /// repository. Everything that writes the table depends on it, so it is
    /// pinned rather than left to drift.
    #[test]
    fn the_table_holds_forty_slots() {
        assert_eq!(STREAM_SLOTS, 40);
        assert!(is_valid_slot(0));
        assert!(is_valid_slot(39));
        assert!(!is_valid_slot(40));
    }
}
