//! `soundtrack.json`: the switch, the base, the 21 slots, and the paths.
//!
//! Everything under `music/` is user data that repair never deletes (see
//! `DataDir::is_user_data`). The record is kept as intents (a slot, and where
//! its audio lives), not byte diffs, so it replays against a new base after a
//! roster or game update.
//!
//! `music/state.json` is a different file: `crate::library::RebuildState`,
//! shared with the roster, which records the hash a rebuild produced so
//! repair calls the image customized rather than damaged.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::platform::{Error, Result};
use crate::soundtrack::{pack, Slot};

pub const SCHEMA: &str = "nsr.soundtrack.v2";
pub const SOUNDTRACK_FILE: &str = "soundtrack.json";

pub fn music_dir(root: &Path) -> PathBuf {
    root.join("music")
}
pub fn sources_dir(root: &Path) -> PathBuf {
    music_dir(root).join("sources")
}
pub fn packs_dir(root: &Path) -> PathBuf {
    music_dir(root).join("packs")
}
pub fn encoded_dir(root: &Path) -> PathBuf {
    music_dir(root).join("encoded")
}
pub fn soundtrack_path(root: &Path) -> PathBuf {
    music_dir(root).join(SOUNDTRACK_FILE)
}
/// The RebuildState record, shared with the roster.
pub fn rebuild_state_path(root: &Path) -> PathBuf {
    music_dir(root).join("state.json")
}
pub fn encoded_blob_path(root: &Path, track_id: &str, slot: Slot) -> PathBuf {
    encoded_dir(root).join(format!("{track_id}-s{slot}.asf"))
}

/// First 16 hex of the SHA-256 of a source file's bytes.
pub fn track_id(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))[..16].to_string()
}

/// Which soundtrack the slots layer on top of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Base {
    Stock,
    /// A soundtrack mod package; `pack` is relative to the data root.
    Package { id: String, version: String, pack: String },
}

/// Where one slot's audio comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// The disc's own song, as an explicit reset that overrides the base.
    Stock,
    /// A file the user added; `file` is relative to the data root, and the
    /// encoded blob lives at `encoded_blob_path(root, track, slot)`.
    UserFile { track: String, file: String },
    /// A song inside a pack; `pack` is relative to the data root.
    PackStream { pack: String, slot: u8 },
    /// The disc's own song from another slot, moved here by a reorder. The
    /// audio is read from the pristine image's slot `slot`; the entry's
    /// text is that song's banner unless the player renamed it.
    StockFrom { slot: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub source: Source,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

impl Entry {
    pub fn stock() -> Self {
        Self { source: Source::Stock, title: String::new(), artist: String::new(), album: String::new(), duration_ms: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Soundtrack {
    pub schema: String,
    pub enabled: bool,
    pub base: Base,
    /// What the booted disc has, keyed by slot.
    #[serde(default)]
    pub slots: BTreeMap<u8, Entry>,
    /// Staged, not built.
    #[serde(default)]
    pub pending: BTreeMap<u8, Entry>,
    /// The pristine image the last apply started from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha256: Option<String>,
    /// The image the last apply produced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebuilt_sha256: Option<String>,
}

impl Default for Soundtrack {
    fn default() -> Self {
        Self {
            schema: SCHEMA.to_string(),
            enabled: false,
            base: Base::Stock,
            slots: BTreeMap::new(),
            pending: BTreeMap::new(),
            base_sha256: None,
            rebuilt_sha256: None,
        }
    }
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.to_path_buf(), source }
}

impl Soundtrack {
    /// Absent file = default. A malformed file is an error: treating it as
    /// absent would leave a customized image with nothing to explain it.
    pub fn load(root: &Path) -> Result<Soundtrack> {
        let path = soundtrack_path(root);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Soundtrack::default()),
            Err(e) => return Err(io(&path, e)),
        };
        let s: Soundtrack = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Io { path: path.clone(), source: std::io::Error::new(std::io::ErrorKind::InvalidData, e) })?;
        for slot in s.slots.keys().chain(s.pending.keys()) {
            Slot::new(*slot)?;
        }
        Ok(s)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = soundtrack_path(root);
        let dir = music_dir(root);
        std::fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::PlainIo(std::io::Error::other(e)))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes).map_err(|e| io(&tmp, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| io(&path, e))
    }

    pub fn stage(&mut self, slot: Slot, entry: Entry) {
        self.pending.insert(slot.get(), entry);
    }

    pub fn unstage(&mut self, slot: Slot) -> Option<Entry> {
        self.pending.remove(&slot.get())
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Pending becomes applied. Called after a successful swap only.
    pub fn commit_pending(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for (slot, entry) in pending {
            self.slots.insert(slot, entry);
        }
    }

    /// Effective slot n: pending (when included) > slots > base pack song >
    /// the disc's own song. Every slot 1..21 is present in the result.
    pub fn effective(&self, root: &Path, base_pack: Option<&pack::Pack>, include_pending: bool) -> BTreeMap<Slot, Effective> {
        let mut out = BTreeMap::new();
        for slot in Slot::all() {
            let n = slot.get();
            let entry = if include_pending { self.pending.get(&n) } else { None }.or_else(|| self.slots.get(&n));
            let eff = match entry {
                Some(e) => match &e.source {
                    // The disc's own song, with the player's banner text
                    // when they renamed it (an edit of an original song
                    // keeps its audio and changes only what the game shows).
                    Source::Stock => Effective::Stock { text: e.text() },
                    Source::StockFrom { slot: s } => Effective::Blob {
                        blob: BlobRef::Stock(Slot::new(*s).unwrap_or(slot)),
                        title: e.title.clone(),
                        artist: e.artist.clone(),
                        album: e.album.clone(),
                        origin: Origin::Original,
                    },
                    Source::UserFile { track, .. } => Effective::Blob {
                        blob: BlobRef::Encoded(encoded_blob_path(root, track, slot)),
                        title: e.title.clone(),
                        artist: e.artist.clone(),
                        album: e.album.clone(),
                        origin: Origin::UserFile,
                    },
                    Source::PackStream { pack, slot: s } => Effective::Blob {
                        blob: BlobRef::Pack { pack: root.join(pack), slot: Slot::new(*s).unwrap_or(slot) },
                        title: e.title.clone(),
                        artist: e.artist.clone(),
                        album: e.album.clone(),
                        origin: Origin::Pack,
                    },
                },
                None => match (&self.base, base_pack) {
                    (Base::Package { .. }, Some(p)) => match p.manifest.songs.get(&n) {
                        Some(song) => Effective::Blob {
                            blob: BlobRef::Pack { pack: p.path.clone(), slot },
                            title: song.title.clone(),
                            artist: song.artist.clone(),
                            album: song.album.clone(),
                            origin: Origin::BasePackage,
                        },
                        None => Effective::Stock { text: None },
                    },
                    _ => Effective::Stock { text: None },
                },
            };
            out.insert(slot, eff);
        }
        out
    }

    /// Whether the effective order is the disc's own soundtrack, banners
    /// included: a renamed original song still needs a rebuild.
    pub fn is_all_stock(effective: &BTreeMap<Slot, Effective>) -> bool {
        effective.values().all(|e| matches!(e, Effective::Stock { text: None }))
    }

    /// The song each slot plays, as an entry that can be put in another
    /// slot: pending (when included) > applied > the base package's song >
    /// the disc's own. Every slot 1..21 is present.
    pub fn portable(&self, base_pack: Option<&pack::Pack>, include_pending: bool) -> BTreeMap<u8, Entry> {
        let mut out = BTreeMap::new();
        for slot in Slot::all() {
            let n = slot.get();
            let entry = if include_pending { self.pending.get(&n) } else { None }.or_else(|| self.slots.get(&n)).cloned();
            let entry = entry.unwrap_or_else(|| match (&self.base, base_pack) {
                (Base::Package { pack: rel, .. }, Some(p)) => match p.manifest.songs.get(&n) {
                    Some(song) => Entry {
                        source: Source::PackStream { pack: rel.clone(), slot: n },
                        title: song.title.clone(),
                        artist: song.artist.clone(),
                        album: song.album.clone(),
                        duration_ms: song.duration_ms,
                    },
                    None => Entry::stock(),
                },
                _ => Entry::stock(),
            });
            out.insert(n, entry);
        }
        out
    }
}

/// `from` moved to `to` the way a list reorders: the songs between them
/// shift by one, nothing is lost. Returns (the slot each song came from,
/// the song) in the new order, position k being slot k+1.
pub fn reordered(entries: &BTreeMap<u8, Entry>, from: Slot, to: Slot) -> Vec<(u8, Entry)> {
    let mut list: Vec<(u8, Entry)> = entries.iter().map(|(n, e)| (*n, e.clone())).collect();
    if from == to {
        return list;
    }
    let Some(i) = list.iter().position(|(n, _)| *n == from.get()) else {
        return list;
    };
    let moved = list.remove(i);
    let j = match list.iter().position(|(n, _)| *n == to.get()) {
        Some(j) if to.get() > from.get() => j + 1,
        Some(j) => j,
        None => list.len(),
    };
    list.insert(j.min(list.len()), moved);
    list
}

/// Banner text a player gave a slot: what the game shows for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotText {
    pub title: String,
    pub artist: String,
    pub album: String,
}

impl Entry {
    /// The entry's banner text, or `None` when every field is empty (the
    /// disc's own text applies then).
    pub fn text(&self) -> Option<SlotText> {
        if self.title.is_empty() && self.artist.is_empty() && self.album.is_empty() {
            None
        } else {
            Some(SlotText { title: self.title.clone(), artist: self.artist.clone(), album: self.album.clone() })
        }
    }

}

impl Soundtrack {
    /// The base pack, when the base is a package. `None` for stock, and an
    /// error when the package's pack file is gone.
    pub fn base_pack(&self, root: &Path) -> Result<Option<pack::Pack>> {
        match &self.base {
            Base::Stock => Ok(None),
            Base::Package { pack: rel, .. } => pack::read_pack(&root.join(rel)).map(Some),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    UserFile,
    Pack,
    BasePackage,
    /// The disc's own song, moved here from another slot.
    Original,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobRef {
    Encoded(PathBuf),
    Pack { pack: PathBuf, slot: Slot },
    /// The pristine image's own streams for this slot.
    Stock(Slot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effective {
    /// The disc's own song in its own slot; `text` when the player renamed it.
    Stock { text: Option<SlotText> },
    Blob { blob: BlobRef, title: String, artist: String, album: String, origin: Origin },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::{synthetic_stream, SYN_PT};

    fn user(track: &str) -> Entry {
        Entry {
            source: Source::UserFile { track: track.into(), file: format!("music/sources/{track}.mp3") },
            title: "T".into(),
            artist: "A".into(),
            album: String::new(),
            duration_ms: Some(1000),
        }
    }

    #[test]
    fn default_saves_and_loads_back_and_absent_is_default() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!Soundtrack::load(dir.path()).unwrap().enabled, "Custom music is opt-in");
        assert_eq!(Soundtrack::load(dir.path()).unwrap(), Soundtrack::default());
        let mut s = Soundtrack::default();
        s.stage(Slot::new(3).unwrap(), user("abc"));
        s.save(dir.path()).unwrap();
        let back = Soundtrack::load(dir.path()).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.schema, SCHEMA);
        assert!(back.has_pending());
    }

    #[test]
    fn malformed_is_an_error_and_a_bad_slot_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(music_dir(dir.path())).unwrap();
        std::fs::write(soundtrack_path(dir.path()), b"{not json").unwrap();
        assert!(Soundtrack::load(dir.path()).is_err());
        std::fs::write(
            soundtrack_path(dir.path()),
            br#"{"schema":"nsr.soundtrack.v2","enabled":true,"base":{"kind":"stock"},"slots":{"22":{"source":{"kind":"stock"}}}}"#,
        )
        .unwrap();
        assert!(Soundtrack::load(dir.path()).unwrap_err().to_string().contains("22"));
    }

    #[test]
    fn effective_prefers_pending_then_slots_then_base_then_stock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // a base pack with songs in slots 1 and 2
        let blob: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 2400, 256)).collect();
        let pack_path = root.join("mods/packages/reborn.soundtrack.x/1.0.0/files/x.nsrmusic");
        pack::write_pack(
            &pack_path,
            "X",
            "",
            "",
            &[
                pack::SongInput { slot: Slot::new(1).unwrap(), blob: blob.clone(), title: "One".into(), artist: "".into(), album: "".into() },
                pack::SongInput { slot: Slot::new(2).unwrap(), blob, title: "Two".into(), artist: "".into(), album: "".into() },
            ],
        )
        .unwrap();
        let mut s = Soundtrack {
            base: Base::Package {
                id: "reborn.soundtrack.x".into(),
                version: "1.0.0".into(),
                pack: "mods/packages/reborn.soundtrack.x/1.0.0/files/x.nsrmusic".into(),
            },
            ..Soundtrack::default()
        };
        s.slots.insert(2, Entry::stock()); // explicit reset overrides the base
        s.slots.insert(3, user("aaa"));
        s.stage(Slot::new(3).unwrap(), user("bbb"));
        let base = s.base_pack(root).unwrap();
        let eff = s.effective(root, base.as_ref(), true);
        assert_eq!(eff.len(), 21);
        match &eff[&Slot::new(1).unwrap()] {
            Effective::Blob { origin: Origin::BasePackage, title, .. } => assert_eq!(title, "One"),
            other => panic!("{other:?}"),
        }
        assert_eq!(eff[&Slot::new(2).unwrap()], Effective::Stock { text: None });
        match &eff[&Slot::new(3).unwrap()] {
            Effective::Blob { blob: BlobRef::Encoded(p), origin: Origin::UserFile, .. } => {
                assert_eq!(p, &encoded_blob_path(root, "bbb", Slot::new(3).unwrap()))
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(eff[&Slot::new(4).unwrap()], Effective::Stock { text: None });
        // without pending, slot 3 is the applied one
        let eff = s.effective(root, base.as_ref(), false);
        match &eff[&Slot::new(3).unwrap()] {
            Effective::Blob { blob: BlobRef::Encoded(p), .. } => assert!(p.ends_with("aaa-s3.asf")),
            other => panic!("{other:?}"),
        }
        assert!(!Soundtrack::is_all_stock(&eff));
        s.commit_pending();
        assert!(!s.has_pending());
        assert_eq!(s.slots[&3], user("bbb"));
        assert!(Soundtrack::is_all_stock(&Soundtrack::default().effective(root, None, true)));
    }

    #[test]
    fn a_renamed_original_song_keeps_its_audio_and_carries_its_text() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Soundtrack::default();
        s.stage(Slot::new(5).unwrap(), Entry { title: "My name".into(), ..Entry::stock() });
        let eff = s.effective(dir.path(), None, true);
        assert_eq!(
            eff[&Slot::new(5).unwrap()],
            Effective::Stock { text: Some(SlotText { title: "My name".into(), artist: String::new(), album: String::new() }) }
        );
        assert!(!Soundtrack::is_all_stock(&eff), "a renamed banner needs a rebuild");
        s.stage(Slot::new(5).unwrap(), Entry::stock());
        assert!(Soundtrack::is_all_stock(&s.effective(dir.path(), None, true)));
    }

    #[test]
    fn reordering_moves_one_song_and_shifts_the_ones_between() {
        let s = Soundtrack::default();
        let portable = s.portable(None, true);
        assert_eq!(portable.len(), 21);
        let order = reordered(&portable, Slot::new(5).unwrap(), Slot::new(2).unwrap());
        let origins: Vec<u8> = order.iter().map(|(n, _)| *n).collect();
        assert_eq!(&origins[..6], &[1, 5, 2, 3, 4, 6], "5 lands on 2, 2..4 move down one");
        let order = reordered(&portable, Slot::new(2).unwrap(), Slot::new(5).unwrap());
        let origins: Vec<u8> = order.iter().map(|(n, _)| *n).collect();
        assert_eq!(&origins[..6], &[1, 3, 4, 5, 2, 6], "2 lands on 5, 3..5 move up one");
        assert_eq!(origins.len(), 21);
        let same = reordered(&portable, Slot::new(7).unwrap(), Slot::new(7).unwrap());
        assert_eq!(same.iter().map(|(n, _)| *n).collect::<Vec<_>>(), (1..=21).collect::<Vec<u8>>());
    }

    #[test]
    fn track_ids_are_sixteen_hex_of_sha256() {
        assert_eq!(track_id(b"abc"), "ba7816bf8f01cfea");
        assert_eq!(encoded_blob_path(Path::new("r"), "ba7816bf8f01cfea", Slot::new(21).unwrap()), Path::new("r/music/encoded/ba7816bf8f01cfea-s21.asf"));
    }
}
