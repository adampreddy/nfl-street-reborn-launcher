# Custom Soundtrack Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Players replace, share, and reset the 21 songs on the NFL Street 2 disc from the launcher, with banner text filled automatically, and pick the original or NFL Street 1 soundtrack from the mod browser.

**Architecture:** A new `reborn_core::soundtrack` module family ports the proven Python chain to Rust: read the booted disc's FST, splice 40-segment EA-XA blobs into `pathfind.dat`, rewrite the PFDx position table and the EATX banner records, stream a new ISO with a regenerated flat FST, swap it over the booted image, and record the hash. The encoder is a byte-exact port of `encode_ea_xa.py` and `extend_slot_40big.py`. The launcher gets a Music page and commands; soundtracks are mod packages in an exclusive group.

**Tech Stack:** Rust 1.88+ (`forbid(unsafe_code)` in reborn-core), symphonia 0.5 (decode), lofty 0.22 (tags), zip 2 (packs), serde_json, ureq via the existing `Http` trait, Tauri 2 static HTML/JS, Node checkers, Python oracle tools in `C:\Users\Adam\nflstreet2_clean\tools`.

**Spec:** `docs/superpowers/specs/2026-09-08-music-editor-design.md`

## Global Constraints

- Work in a git worktree: `git worktree add target/music-tree -b music` from the launcher repo. Another agent edits the main tree; never `git add -A` there.
- `reborn-core` keeps `#![forbid(unsafe_code)]`; new dependencies must be pure Rust.
- Core reads no environment variables except in tests. Tests that need Adam's files read `NSR_CLEAN` (path to `nflstreet2_clean`) and return early when it is unset, printing `skipped: NSR_CLEAN unset`.
- Every user-visible sentence: US English, no em-dashes, no error popups; refusals are states with a next step.
- Slots are 1..21. Segments per song are 40. The old `music.rs` constant `STREAM_SLOTS = 40` is wrong and is removed.
- One disc rebuild per Apply. Never write the pristine image. Never patch an ISO in place: always write `.part` then rename.
- Verification before any claim: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `node Tools/check-shell-pages.mjs`, `node Tools/check-music-page.mjs`, `Tools/test-all.ps1`.
- Byte oracles: (1) repack unchanged == source disc; (2) Rust pack apply == `music_pack.py import`; (3) Rust encode == `extend_slot_40big.py` from a 24 kHz stereo WAV.
- Multi-line patch scripts are written with the Write tool, never heredocs (Adam's rule).

## File structure

reborn-core (`crates/reborn-core/src/`):

| File | Responsibility |
|---|---|
| `soundtrack/mod.rs` | constants, `Slot`, `slot_first_stream`, `Refusal`, submodule declarations |
| `soundtrack/gcm.rs` | read a GameCube disc's FST, read one file, repack with regenerated FST |
| `soundtrack/asf.rs` | SCHl scan, slot ranges, blob validation, splice, PT tag read and patch |
| `soundtrack/pathfind.rs` | TERF sizes, ASF extraction and replacement, PFDx table rewrite |
| `soundtrack/eatx.rs` | locate, read, write the 21 banner records in template.dat |
| `soundtrack/ea_xa.rs` | EA-XA frame and block decoder and encoder, stream builder |
| `soundtrack/segments.rs` | 40-segment slot encoder (the extend_slot_40big recipe), stock PT headers |
| `soundtrack/resample.rs` | windowed-sinc resampler to 24 kHz stereo |
| `soundtrack/decode.rs` | symphonia file decode to stereo PCM at 24 kHz |
| `soundtrack/tags.rs` | embedded tags, file-name cleaning |
| `soundtrack/lookup.rs` | iTunes and MusicBrainz candidates through `Http` |
| `soundtrack/pack.rs` | `.nsrmusic` format 2 read and write, `.nsmp` format 1 read |
| `soundtrack/state.rs` | `soundtrack.json`, paths, effective slots |
| `soundtrack/apply.rs` | staging, apply pipeline, revert, export, replay |
| `soundtrack/street1.rs` | build the NFL Street 1 pack from extracted disc files |
| `soundtrack/testing.rs` | `#[cfg(test)]` synthetic disc, pathfind, template fixtures |
| `music.rs` | shrinks to `music_root` and the RebuildState path; wrong model removed |
| `content/pipeline.rs` | replay the soundtrack after a roster lands |

reborn-cli: `src/main.rs` gains `MusicCommand` with the verbs in the spec.

reborn-ui: `src/music_page.rs` (commands), `src/main.rs` (register, soundtrack branch in `mods_toggle`), `ui/music.html`, `ui/music.js`, nav in the seven shell pages, `ui/mods.js` rebuild state.

Tools: `Tools/check-music-page.mjs`, `Tools/check-shell-pages.mjs` (add music.html), `Tools/test-all.ps1` (add the checker), `Tools/oracle-encode.py`, `Tools/oracle-pack-import.py`.

Docs: `docs/RUNNING_REBORNCTL.md` (music verbs), `docs/architecture/CONTENT_AND_UPDATE_MODEL.md` (section 15 note: no work tree).

---

### Task 1: Worktree, dependencies, module skeleton

**Files:**
- Create: `crates/reborn-core/src/soundtrack/mod.rs`
- Modify: `crates/reborn-core/src/lib.rs` (add `pub mod soundtrack;` after `pub mod music;`)
- Modify: `crates/reborn-core/Cargo.toml`

**Interfaces:**
- Produces: `pub const SONG_SLOTS: u8 = 21`, `pub const SEGMENTS_PER_SONG: usize = 40`, `pub const STREAM_COUNT: usize = 876`, `pub struct Slot(u8)` with `Slot::new(u8) -> Result<Slot, Refusal>`, `Slot::get(self) -> u8`, `Slot::all() -> impl Iterator<Item = Slot>`, `pub fn slot_first_stream(slot: Slot) -> usize`, `pub enum Refusal` with `message(&self) -> String`.

- [ ] **Step 1: Create the worktree**

```bash
cd "C:/Users/Adam/Desktop/Reborn Installer/NFL Street Reborn Launcher"
git worktree add target/music-tree -b music
cd target/music-tree
```

- [ ] **Step 2: Add dependencies to `crates/reborn-core/Cargo.toml`**

Under `[dependencies]`:

```toml
# Custom soundtrack. All pure Rust: reborn-core forbids unsafe and the
# launcher ships no ffmpeg (SECURITY_PRIVACY_LICENSING.md section 24).
symphonia = { version = "0.5", default-features = false, features = ["mp3", "aac", "isomp4", "flac", "vorbis", "ogg", "wav", "aiff", "pcm"] }
lofty = "0.22"
zip = { version = "2", default-features = false, features = ["deflate"] }
```

- [ ] **Step 3: Write the failing test in `soundtrack/mod.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_one_to_twenty_one() {
        assert!(Slot::new(0).is_err());
        assert!(Slot::new(22).is_err());
        assert_eq!(Slot::new(1).unwrap().get(), 1);
        assert_eq!(Slot::all().count(), 21);
    }

    #[test]
    fn first_stream_matches_the_python_tools() {
        // slot_first(song) = 3 if song == 1 else 44 + (song - 2) * 40
        assert_eq!(slot_first_stream(Slot::new(1).unwrap()), 3);
        assert_eq!(slot_first_stream(Slot::new(2).unwrap()), 44);
        assert_eq!(slot_first_stream(Slot::new(21).unwrap()), 804);
        assert_eq!(slot_first_stream(Slot::new(21).unwrap()) + SEGMENTS_PER_SONG, 844);
    }
}
```

- [ ] **Step 4: Run it to see it fail**

Run: `cargo test -p reborn-core soundtrack::tests -- --nocapture`
Expected: compile error, module missing.

- [ ] **Step 5: Write `soundtrack/mod.rs`**

```rust
//! Custom soundtrack: the 21 songs on the NFL Street 2 disc, replaced,
//! shared, and reset from the launcher.
//!
//! Design: docs/superpowers/specs/2026-09-08-music-editor-design.md.
//! Every byte layout here was reverse-engineered and confirmed in game; the
//! Python tools under nflstreet2_clean/tools are the oracle the tests
//! compare against.

pub mod apply;
pub mod asf;
pub mod decode;
pub mod ea_xa;
pub mod eatx;
pub mod gcm;
pub mod lookup;
pub mod pack;
pub mod pathfind;
pub mod resample;
pub mod segments;
pub mod state;
pub mod street1;
pub mod tags;
#[cfg(test)]
pub(crate) mod testing;

/// Songs on the disc. The banner table, the playlist, and the ASF all agree.
pub const SONG_SLOTS: u8 = 21;
/// Streams per song in the ASF. The engine bounds each song to its 40 streams.
pub const SEGMENTS_PER_SONG: usize = 40;
/// SCHl streams in the shipped ASF: 3 leading, 21 x 40, 33 trailing.
pub const STREAM_COUNT: usize = 876;
/// Per-segment byte ceiling: the stream ring buffer is 256 KiB.
pub const SEGMENT_CEILING: usize = 0x3E000;

/// A song slot, 1..=21. Constructed only through `new`, so nothing downstream
/// re-checks the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot(u8);

impl Slot {
    pub fn new(n: u8) -> Result<Self, Refusal> {
        if (1..=SONG_SLOTS).contains(&n) {
            Ok(Self(n))
        } else {
            Err(Refusal::SlotOutOfRange { slot: n })
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
    pub fn index(self) -> usize {
        usize::from(self.0 - 1)
    }
    pub fn all() -> impl Iterator<Item = Slot> {
        (1..=SONG_SLOTS).map(Slot)
    }
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Index of the first of a slot's 40 streams in the ASF.
pub fn slot_first_stream(slot: Slot) -> usize {
    match slot.get() {
        1 => 3,
        n => 44 + (usize::from(n) - 2) * SEGMENTS_PER_SONG,
    }
}

/// A soundtrack operation that will not happen, and is not a fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    SlotOutOfRange { slot: u8 },
    SessionActive,
    ImageInUse { path: std::path::PathBuf },
    NoPristine,
    NoBooted,
    SongTooLong { seconds: u64, limit_seconds: u64 },
    UnsupportedFormat { extension: String },
    InvalidPack { path: std::path::PathBuf, reason: String },
    SourceMissing { slot: u8, what: String },
    TooLongText { field: &'static str, len: usize },
    OnlineBuild,
    NothingStaged,
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::SlotOutOfRange { slot } => format!("Slot {slot} does not exist. The soundtrack has slots 1 to {SONG_SLOTS}."),
            Refusal::SessionActive => "The game is running. Close it, then apply your soundtrack.".to_string(),
            Refusal::ImageInUse { path } => format!("{} is open in another program, so it was not replaced. Close the game and try again.", path.display()),
            Refusal::NoPristine => "There is no imported game image to build from. Import the game first.".to_string(),
            Refusal::NoBooted => "There is no game image at library/offline. Import the game first.".to_string(),
            Refusal::SongTooLong { seconds, limit_seconds } => format!("This song is {}:{:02} long. Songs up to {}:{:02} fit a slot. Trim it in an audio editor and add it again.", seconds / 60, seconds % 60, limit_seconds / 60, limit_seconds % 60),
            Refusal::UnsupportedFormat { extension } => format!("Files ending in .{extension} cannot be read here. Convert it to MP3 first."),
            Refusal::InvalidPack { path, reason } => format!("{} is not a usable soundtrack pack: {reason}", path.display()),
            Refusal::SourceMissing { slot, what } => format!("Slot {slot} points at {what}, which is no longer there. Add the song again or reset the slot."),
            Refusal::TooLongText { field, len } => format!("The {field} is {len} characters. The game shows at most 49. Shorten it."),
            Refusal::OnlineBuild => "Custom music applies to the offline game only.".to_string(),
            Refusal::NothingStaged => "Nothing is staged. Add or reset a song first.".to_string(),
        }
    }
}

impl From<Refusal> for crate::platform::Error {
    fn from(r: Refusal) -> Self {
        crate::platform::Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidInput, r.message()))
    }
}
```

Create empty files for every submodule listed (`apply.rs`, `asf.rs`, ... `testing.rs`) each containing only a `//!` line naming its responsibility, so the crate compiles.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p reborn-core soundtrack::tests`
Expected: 2 passed. Also `cargo build -p reborn-core` fetches the three crates; if the registry is unreachable, stop and say so.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-core/Cargo.toml Cargo.lock crates/reborn-core/src/lib.rs crates/reborn-core/src/soundtrack
git commit -m "soundtrack: module skeleton, Slot type, refusals, decode deps"
```

---

### Task 2: GameCube disc read and repack (`gcm.rs`)

**Files:**
- Create: `crates/reborn-core/src/soundtrack/gcm.rs`
- Create: `crates/reborn-core/src/soundtrack/testing.rs` (the synthetic disc builder)

**Interfaces:**
- Produces:
  - `pub struct FstFile { pub name: String, pub offset: u64, pub size: u64 }`
  - `pub struct Disc { pub path: PathBuf, pub dol_offset: u64, pub dol_len: u64, pub fst_offset: u64, pub fst_size: u64, pub files: Vec<FstFile> }`
  - `impl Disc { pub fn open(path: &Path) -> Result<Disc>; pub fn file(&self, name: &str) -> Option<&FstFile>; pub fn read_file(&self, name: &str) -> Result<Vec<u8>> }`
  - `pub const FILES_START: u64 = 0x328000; pub const ALIGN: u64 = 0x800; pub const DISC_SIZE: u64 = 0x57058000;`
  - `pub fn repack(disc: &Disc, replacements: &BTreeMap<String, Vec<u8>>, out: &Path, progress: &mut dyn FnMut(u64, u64)) -> Result<Sha256Hex>` writes `out` (the caller names a `.part` and renames), hashing as it writes.
  - `testing::synthetic_disc(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf` builds a tiny disc with the same layout rules (small `DISC_SIZE` override via `repack_with_size`).

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::synthetic_disc;

    #[test]
    fn reads_the_fst_of_a_synthetic_disc() {
        let dir = tempfile::tempdir().unwrap();
        let iso = synthetic_disc(dir.path(), &[("a.dat", b"aaaa"), ("pathfind.dat", b"pppppppp"), ("z.dat", b"z")]);
        let disc = Disc::open(&iso).unwrap();
        let names: Vec<&str> = disc.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a.dat", "pathfind.dat", "z.dat"]);
        assert_eq!(disc.read_file("pathfind.dat").unwrap(), b"pppppppp");
        assert_eq!(disc.file("a.dat").unwrap().offset % ALIGN, 0);
    }

    #[test]
    fn repack_with_no_changes_is_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let iso = synthetic_disc(dir.path(), &[("a.dat", b"aaaa"), ("pathfind.dat", b"pppppppp"), ("z.dat", b"z")]);
        let disc = Disc::open(&iso).unwrap();
        let out = dir.path().join("out.iso");
        let mut seen = 0;
        let hash = repack_with_size(&disc, &BTreeMap::new(), &out, disc_size_of(&iso), &mut |done, _| seen = done).unwrap();
        assert_eq!(std::fs::read(&iso).unwrap(), std::fs::read(&out).unwrap());
        assert_eq!(hash, crate::library::hash_file(&out).unwrap());
        assert!(seen > 0);
    }

    #[test]
    fn a_grown_file_shifts_its_followers_and_keeps_order() {
        let dir = tempfile::tempdir().unwrap();
        let iso = synthetic_disc(dir.path(), &[("a.dat", b"aaaa"), ("pathfind.dat", b"pppppppp"), ("z.dat", b"z")]);
        let disc = Disc::open(&iso).unwrap();
        let big = vec![0x42u8; 0x1801];
        let mut rep = BTreeMap::new();
        rep.insert("pathfind.dat".to_string(), big.clone());
        let out = dir.path().join("out.iso");
        repack_with_size(&disc, &rep, &out, disc_size_of(&iso) + 0x2000, &mut |_, _| {}).unwrap();
        let re = Disc::open(&out).unwrap();
        assert_eq!(re.read_file("pathfind.dat").unwrap(), big);
        assert_eq!(re.read_file("z.dat").unwrap(), b"z");
        let p = re.file("pathfind.dat").unwrap();
        let z = re.file("z.dat").unwrap();
        assert_eq!(z.offset, align(p.offset + p.size));
        assert_eq!(re.files.iter().map(|f| f.name.clone()).collect::<Vec<_>>(), ["a.dat", "pathfind.dat", "z.dat"]);
    }

    /// Oracle 1: the shipped disc repacks to itself.
    #[test]
    fn oracle_repack_of_the_fields_disc_is_identical() {
        let Some(clean) = std::env::var_os("NSR_CLEAN") else { eprintln!("skipped: NSR_CLEAN unset"); return };
        let games = std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("NFL Street Reborn").join("Games");
        let iso = games.join("NFL Street 2 FIELDS 12-19.iso");
        let _ = clean;
        let disc = Disc::open(&iso).unwrap();
        let out = std::env::temp_dir().join("nsr-oracle-repack.iso");
        let hash = repack(&disc, &BTreeMap::new(), &out, &mut |_, _| {}).unwrap();
        assert_eq!(hash, crate::library::hash_file(&iso).unwrap(), "repack changed bytes");
        std::fs::remove_file(&out).unwrap();
    }
}
```

`disc_size_of(&iso)` is `std::fs::metadata(iso).len()`.

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p reborn-core soundtrack::gcm`
Expected: compile errors for `Disc`, `repack`.

- [ ] **Step 3: Write `testing.rs` synthetic disc builder**

```rust
//! Synthetic fixtures: a tiny disc, a tiny pathfind.dat, a tiny template.dat.
use std::path::{Path, PathBuf};

pub const SMALL_FILES_START: u64 = 0x4000;

/// A disc with the real layout rules at toy size: header 0x440, apploader at
/// 0x2440 (16 bytes), DOL at 0x2800 (a valid 0x100 header + 0x20 text), FST
/// after it, files from SMALL_FILES_START, padded to files end + 0x800.
pub fn synthetic_disc(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
    let dol_off: u64 = 0x2800;
    let mut dol = vec![0u8; 0x120];
    dol[0..4].copy_from_slice(&0x100u32.to_be_bytes());          // text0 offset
    dol[0x90..0x94].copy_from_slice(&0x20u32.to_be_bytes());      // text0 size
    let dol_len = 0x120u64;
    let fst_off = align(dol_off + dol_len);
    let total = files.len() as u32 + 1;
    let mut strtab = Vec::new();
    let mut name_off = Vec::new();
    for (n, _) in files { name_off.push(strtab.len() as u32); strtab.extend_from_slice(n.as_bytes()); strtab.push(0); }
    let fst_size = 12 * total as u64 + strtab.len() as u64;
    let files_start = SMALL_FILES_START.max(align(fst_off + fst_size));
    let mut layout = Vec::new();
    let mut cur = files_start;
    for (_, data) in files { cur = align(cur); layout.push((cur, data.len() as u64)); cur += data.len() as u64; }
    let end = align(cur) + 0x800;
    let mut fst = Vec::new();
    fst.push(1u8); fst.extend_from_slice(&[0, 0, 0]); fst.extend_from_slice(&0u32.to_be_bytes()); fst.extend_from_slice(&total.to_be_bytes());
    for ((off, sz), noff) in layout.iter().zip(&name_off) {
        fst.push(0); fst.extend_from_slice(&noff.to_be_bytes()[1..]);
        fst.extend_from_slice(&(*off as u32).to_be_bytes()); fst.extend_from_slice(&(*sz as u32).to_be_bytes());
    }
    fst.extend_from_slice(&strtab);
    let mut img = vec![0u8; end as usize];
    img[0x420..0x424].copy_from_slice(&(dol_off as u32).to_be_bytes());
    img[0x424..0x428].copy_from_slice(&(fst_off as u32).to_be_bytes());
    img[0x428..0x42C].copy_from_slice(&(fst_size as u32).to_be_bytes());
    img[0x42C..0x430].copy_from_slice(&(fst_size as u32).to_be_bytes());
    img[0x2440..0x2450].copy_from_slice(b"APPLOADER-STUB!!");
    img[dol_off as usize..(dol_off + dol_len) as usize].copy_from_slice(&dol);
    img[fst_off as usize..fst_off as usize + fst.len()].copy_from_slice(&fst);
    for ((off, _), (_, data)) in layout.iter().zip(files) { img[*off as usize..*off as usize + data.len()].copy_from_slice(data); }
    let path = dir.join("synthetic.iso");
    std::fs::write(&path, img).unwrap();
    path
}

fn align(x: u64) -> u64 { (x + 0x7FF) & !0x7FF }
```

- [ ] **Step 4: Write `gcm.rs`**

```rust
//! GameCube disc image: FST read, one-file read, and a repack that
//! regenerates the flat FST with build_iso_fields.py's layout rules. A repack
//! with no replacements reproduces the source byte for byte (oracle 1).
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::manifest::Sha256Hex;
use crate::platform::{Error, Result};

pub const FILES_START: u64 = 0x328000;
pub const ALIGN: u64 = 0x800;
pub const DISC_SIZE: u64 = 0x57058000;
const CHUNK: usize = 4 << 20;

pub fn align(x: u64) -> u64 { (x + ALIGN - 1) & !(ALIGN - 1) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FstFile { pub name: String, pub offset: u64, pub size: u64 }

#[derive(Debug, Clone)]
pub struct Disc {
    pub path: PathBuf,
    pub dol_offset: u64,
    pub dol_len: u64,
    pub fst_offset: u64,
    pub fst_size: u64,
    /// FST order, which build_iso_fields.py also uses as disc order.
    pub files: Vec<FstFile>,
}

fn io(path: &Path, source: std::io::Error) -> Error { Error::Io { path: path.to_path_buf(), source } }
fn bad(path: &Path, what: &str) -> Error { Error::Io { path: path.to_path_buf(), source: std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_string()) } }
fn be32(b: &[u8], o: usize) -> u32 { u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }

impl Disc {
    pub fn open(path: &Path) -> Result<Disc> {
        let mut f = std::fs::File::open(path).map_err(|e| io(path, e))?;
        let mut hdr = [0u8; 0x440];
        f.read_exact(&mut hdr).map_err(|e| io(path, e))?;
        let dol_offset = u64::from(be32(&hdr, 0x420));
        let fst_offset = u64::from(be32(&hdr, 0x424));
        let fst_size = u64::from(be32(&hdr, 0x428));
        let mut dh = [0u8; 0x100];
        f.seek(SeekFrom::Start(dol_offset)).map_err(|e| io(path, e))?;
        f.read_exact(&mut dh).map_err(|e| io(path, e))?;
        let mut dol_len = 0u64;
        for i in 0..7 { dol_len = dol_len.max(u64::from(be32(&dh, i * 4)) + u64::from(be32(&dh, 0x90 + i * 4))); }
        for i in 0..11 { dol_len = dol_len.max(u64::from(be32(&dh, 0x1C + i * 4)) + u64::from(be32(&dh, 0xAC + i * 4))); }
        let mut fst = vec![0u8; fst_size as usize];
        f.seek(SeekFrom::Start(fst_offset)).map_err(|e| io(path, e))?;
        f.read_exact(&mut fst).map_err(|e| io(path, e))?;
        if fst.len() < 12 || fst[0] != 1 { return Err(bad(path, "no FST root entry")); }
        let n = be32(&fst, 8) as usize;
        if fst.len() < n * 12 { return Err(bad(path, "FST shorter than its entry count")); }
        let strings = &fst[n * 12..];
        let mut files = Vec::with_capacity(n - 1);
        for i in 1..n {
            let e = i * 12;
            if fst[e] != 0 { return Err(bad(path, "the FST has a directory entry; this packer expects a flat FST")); }
            let noff = (be32(&fst, e) & 0xFF_FFFF) as usize;
            let end = strings[noff..].iter().position(|&b| b == 0).ok_or_else(|| bad(path, "unterminated FST name"))?;
            let name = String::from_utf8_lossy(&strings[noff..noff + end]).into_owned();
            files.push(FstFile { name, offset: u64::from(be32(&fst, e + 4)), size: u64::from(be32(&fst, e + 8)) });
        }
        Ok(Disc { path: path.to_path_buf(), dol_offset, dol_len, fst_offset, fst_size, files })
    }

    pub fn file(&self, name: &str) -> Option<&FstFile> { self.files.iter().find(|f| f.name == name) }

    pub fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let f = self.file(name).ok_or_else(|| bad(&self.path, &format!("the disc has no file named {name}")))?;
        let mut h = std::fs::File::open(&self.path).map_err(|e| io(&self.path, e))?;
        h.seek(SeekFrom::Start(f.offset)).map_err(|e| io(&self.path, e))?;
        let mut buf = vec![0u8; f.size as usize];
        h.read_exact(&mut buf).map_err(|e| io(&self.path, e))?;
        Ok(buf)
    }
}

/// Repack to a full-size disc. See `repack_with_size`.
pub fn repack(disc: &Disc, replacements: &BTreeMap<String, Vec<u8>>, out: &Path, progress: &mut dyn FnMut(u64, u64)) -> Result<Sha256Hex> {
    repack_with_size(disc, replacements, out, DISC_SIZE, progress)
}

/// Write `out`: verbatim head through the DOL, a regenerated FST, every file
/// in FST order (replacements substituted), zero padding to `disc_size`.
pub fn repack_with_size(disc: &Disc, replacements: &BTreeMap<String, Vec<u8>>, out: &Path, disc_size: u64, progress: &mut dyn FnMut(u64, u64)) -> Result<Sha256Hex> {
    for name in replacements.keys() {
        if disc.file(name).is_none() { return Err(bad(&disc.path, &format!("cannot replace {name}: not on the disc"))); }
    }
    let fst_off = align(disc.dol_offset + disc.dol_len);
    let total = disc.files.len() as u32 + 1;
    let mut strtab = Vec::new();
    let mut name_off = Vec::with_capacity(disc.files.len());
    for f in &disc.files { name_off.push(strtab.len() as u32); strtab.extend_from_slice(f.name.as_bytes()); strtab.push(0); }
    let fst_size = 12 * u64::from(total) + strtab.len() as u64;
    let files_start = FILES_START.max(align(fst_off + fst_size));
    let mut layout: Vec<(u64, u64)> = Vec::with_capacity(disc.files.len());
    let mut cur = files_start;
    for f in &disc.files {
        let size = replacements.get(&f.name).map(|d| d.len() as u64).unwrap_or(f.size);
        cur = align(cur);
        layout.push((cur, size));
        cur += size;
    }
    if cur > disc_size { return Err(bad(&disc.path, &format!("files overflow the disc: 0x{cur:X} > 0x{disc_size:X}"))); }
    let mut fst = Vec::with_capacity(fst_size as usize);
    fst.push(1); fst.extend_from_slice(&[0, 0, 0]); fst.extend_from_slice(&0u32.to_be_bytes()); fst.extend_from_slice(&total.to_be_bytes());
    for ((off, size), noff) in layout.iter().zip(&name_off) {
        fst.push(0); fst.extend_from_slice(&noff.to_be_bytes()[1..]);
        fst.extend_from_slice(&(*off as u32).to_be_bytes()); fst.extend_from_slice(&(*size as u32).to_be_bytes());
    }
    fst.extend_from_slice(&strtab);
    debug_assert_eq!(fst.len() as u64, fst_size);

    let mut src = std::fs::File::open(&disc.path).map_err(|e| io(&disc.path, e))?;
    let mut dst = std::io::BufWriter::with_capacity(CHUNK, std::fs::File::create(out).map_err(|e| io(out, e))?);
    let mut hasher = Sha256::new();
    let mut written: u64 = 0;
    let mut emit = |dst: &mut std::io::BufWriter<std::fs::File>, bytes: &[u8]| -> Result<()> {
        dst.write_all(bytes).map_err(|e| io(out, e))?;
        hasher.update(bytes);
        written += bytes.len() as u64;
        Ok(())
    };
    // Head: boot.bin, bi2, apploader, DOL, verbatim, then the three FST fields.
    let head_len = (disc.dol_offset + disc.dol_len) as usize;
    let mut head = vec![0u8; head_len];
    src.read_exact(&mut head).map_err(|e| io(&disc.path, e))?;
    head[0x424..0x428].copy_from_slice(&(fst_off as u32).to_be_bytes());
    head[0x428..0x42C].copy_from_slice(&(fst_size as u32).to_be_bytes());
    head[0x42C..0x430].copy_from_slice(&(fst_size as u32).to_be_bytes());
    emit(&mut dst, &head)?;
    emit(&mut dst, &vec![0u8; (fst_off - written) as usize])?;
    emit(&mut dst, &fst)?;
    let mut buf = vec![0u8; CHUNK];
    for (f, (off, size)) in disc.files.iter().zip(&layout) {
        emit(&mut dst, &vec![0u8; (off - written) as usize])?;
        if let Some(data) = replacements.get(&f.name) {
            emit(&mut dst, data)?;
        } else {
            src.seek(SeekFrom::Start(f.offset)).map_err(|e| io(&disc.path, e))?;
            let mut left = f.size as usize;
            while left > 0 {
                let n = left.min(CHUNK);
                src.read_exact(&mut buf[..n]).map_err(|e| io(&disc.path, e))?;
                emit(&mut dst, &buf[..n])?;
                left -= n;
            }
        }
        progress(written, disc_size);
        let _ = size;
    }
    let zeros = vec![0u8; CHUNK];
    while written < disc_size {
        let n = ((disc_size - written) as usize).min(CHUNK);
        emit(&mut dst, &zeros[..n])?;
    }
    dst.flush().map_err(|e| io(out, e))?;
    progress(written, disc_size);
    Ok(Sha256Hex::parse(&hex::encode(hasher.finalize())).expect("hex is a hash"))
}
```

Note the closure borrows `hasher` and `written` mutably; if the borrow checker objects, turn `emit` into a small struct `Sink { dst, hasher, written }` with a method `put(&mut self, &[u8])`. Do that rather than fight it.

- [ ] **Step 5: Run the tests, then the oracle**

Run: `cargo test -p reborn-core soundtrack::gcm`
Expected: 3 passed, oracle skipped.
Run: `$env:NSR_CLEAN="C:\Users\Adam\nflstreet2_clean"; cargo test -p reborn-core --release oracle_repack -- --nocapture`
Expected: passes. A 1.46 GB write, about 10 s. If it fails, diff the first mismatching offset with a small Python loop before changing anything.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-core/src/soundtrack/gcm.rs crates/reborn-core/src/soundtrack/testing.rs
git commit -m "soundtrack: disc FST read and byte-identical repack"
```

---

### Task 3: ASF streams and pathfind.dat (`asf.rs`, `pathfind.rs`)

**Files:**
- Create: `crates/reborn-core/src/soundtrack/asf.rs`
- Create: `crates/reborn-core/src/soundtrack/pathfind.rs`
- Modify: `crates/reborn-core/src/soundtrack/testing.rs` (add `synthetic_stream`, `synthetic_pathfind`)

**Interfaces:**
- `asf::scan_schl(asf: &[u8]) -> Vec<usize>` offsets of every `SCHl`.
- `asf::stream_len(asf: &[u8], start: usize) -> Option<usize>` walks chunks by their u32LE sizes until after `SCEl`.
- `asf::slot_range(positions: &[usize], asf_len: usize, slot: Slot) -> (usize, usize)`.
- `asf::slot_blob(asf: &[u8], slot: Slot) -> Result<Vec<u8>>` (requires 876 streams).
- `asf::splice(asf: &[u8], slot: Slot, blob: &[u8]) -> Result<Vec<u8>>` pads to 0x800.
- `asf::validate_blob(blob: &[u8]) -> std::result::Result<BlobInfo, String>` where `BlobInfo { streams: usize, samples: u64, duration_ms: u64 }`; requires exactly 40 SCHl, each stream start at a 128-byte multiple within the blob, each stream ends with SCEl, PT 0x85 present.
- `asf::pt_bytes(stream: &[u8]) -> &[u8]` (the PT payload after the 8-byte SCHl header).
- `asf::pt_tag(pt: &[u8], tag: u8) -> Option<u64>`; `asf::patch_pt_tag(pt: &[u8], tag: u8, value: u64) -> std::result::Result<Vec<u8>, String>` (keeps field length; tag stream starts at the first 0x80 byte).
- `asf::scdl_sample_sum(stream: &[u8]) -> u64`.
- `pathfind::Pathfind { pub asf_offset: usize, pub asf_size: usize, pub toc_offset: usize }` with `Pathfind::parse(bytes: &[u8]) -> Result<Pathfind>`; `pathfind::asf(bytes) -> &[u8]`; `pathfind::with_asf(bytes: &[u8], new_asf: &[u8]) -> Result<Vec<u8>>` (fix DIR1 size at 0x81C and INFO_SIZE at 0x1004); `pathfind::rewrite_toc(bytes: &mut Vec<u8>, changed: &[Slot]) -> Result<TocReport { w0_rewritten: usize }>`.

- [ ] **Step 1: Write the failing tests**

```rust
// asf.rs tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::{synthetic_stream, synthetic_pathfind, SYN_PT};
    use crate::soundtrack::{pathfind, Slot, STREAM_COUNT};

    #[test]
    fn scans_every_stream_and_finds_slot_ranges() {
        let pf = synthetic_pathfind(256);
        let a = pathfind::asf(&pf);
        let pos = scan_schl(a);
        assert_eq!(pos.len(), STREAM_COUNT);
        assert!(pos.iter().all(|p| p % 128 == 0));
        let (s, e) = slot_range(&pos, a.len(), Slot::new(1).unwrap());
        assert_eq!(s, pos[3]); assert_eq!(e, pos[43]);
        let (s, e) = slot_range(&pos, a.len(), Slot::new(21).unwrap());
        assert_eq!(s, pos[804]); assert_eq!(e, pos[844]);
    }

    #[test]
    fn a_slot_blob_validates_and_splices_back_unchanged() {
        let pf = synthetic_pathfind(256);
        let a = pathfind::asf(&pf);
        let blob = slot_blob(a, Slot::new(5).unwrap()).unwrap();
        let info = validate_blob(&blob).unwrap();
        assert_eq!(info.streams, 40);
        assert_eq!(splice(a, Slot::new(5).unwrap(), &blob).unwrap(), a);
    }

    #[test]
    fn splicing_a_bigger_blob_grows_and_pads() {
        let pf = synthetic_pathfind(256);
        let a = pathfind::asf(&pf);
        let big: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 1000, 512)).collect();
        let out = splice(a, Slot::new(2).unwrap(), &big).unwrap();
        assert_eq!(out.len() % 0x800, 0);
        assert_eq!(scan_schl(&out).len(), STREAM_COUNT);
        assert_eq!(slot_blob(&out, Slot::new(2).unwrap()).unwrap(), big);
        assert_eq!(slot_blob(&out, Slot::new(3).unwrap()).unwrap(), slot_blob(a, Slot::new(3).unwrap()).unwrap());
    }

    #[test]
    fn pt_tags_read_and_patch_in_place() {
        let s = synthetic_stream(SYN_PT, 1234, 256);
        let pt = pt_bytes(&s);
        assert_eq!(pt_tag(pt, 0x85), Some(1234));
        let p2 = patch_pt_tag(pt, 0x85, 99).unwrap();
        assert_eq!(p2.len(), pt.len());
        assert_eq!(pt_tag(&p2, 0x85), Some(99));
        assert_eq!(scdl_sample_sum(&s), 1234);
    }

    #[test]
    fn validate_rejects_wrong_stream_counts() {
        let bad: Vec<u8> = (0..39).flat_map(|_| synthetic_stream(SYN_PT, 10, 128)).collect();
        assert!(validate_blob(&bad).unwrap_err().contains("39"));
    }
}

// pathfind.rs tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::{synthetic_pathfind, synthetic_stream, SYN_PT};
    use crate::soundtrack::{asf, Slot};

    #[test]
    fn parses_the_terf_fields() {
        let pf = synthetic_pathfind(256);
        let p = Pathfind::parse(&pf).unwrap();
        assert_eq!(p.asf_offset, 0x13000);
        assert_eq!(p.toc_offset, 0x1138C);
        assert_eq!(p.asf_size, pf.len() - 0x13000);
    }

    #[test]
    fn replacing_the_asf_fixes_both_sizes_and_the_toc_follows() {
        let pf = synthetic_pathfind(256);
        let a = asf(&pf).to_vec();
        let big: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 2400, 512)).collect();
        let new_asf = asf::splice(&a, Slot::new(4).unwrap(), &big).unwrap();
        let mut out = with_asf(&pf, &new_asf).unwrap();
        let p = Pathfind::parse(&out).unwrap();
        assert_eq!(p.asf_size, new_asf.len());
        let info_before = u32::from_be_bytes(pf[0x1004..0x1008].try_into().unwrap());
        let info_after = u32::from_be_bytes(out[0x1004..0x1008].try_into().unwrap());
        assert_eq!(info_after as usize - info_before as usize, new_asf.len() - a.len());
        let rep = rewrite_toc(&mut out, &[Slot::new(4).unwrap()]).unwrap();
        assert!(rep.w0_rewritten > 0);
        // every w0 now points at a real SCHl, and slot 4's w1 is 100 ms
        let pos = asf::scan_schl(asf(&out));
        for (k, p) in pos.iter().enumerate() {
            let o = 0x1138C + 8 * k;
            let w0 = u32::from_be_bytes(out[o..o + 4].try_into().unwrap()) as usize;
            assert_eq!(w0 << 7, *p);
        }
        let w1 = u32::from_be_bytes(out[0x1138C + 8 * 124 + 4..0x1138C + 8 * 124 + 8].try_into().unwrap());
        assert_eq!(w1, 100);
    }

    #[test]
    fn rewrite_toc_on_stock_layout_changes_nothing() {
        let mut pf = synthetic_pathfind(256);
        let before = pf.clone();
        let rep = rewrite_toc(&mut pf, &[]).unwrap();
        assert_eq!(rep.w0_rewritten, 0);
        assert_eq!(pf, before);
    }
}
```

- [ ] **Step 2: Add the fixtures to `testing.rs`**

```rust
/// A PT header shaped like the disc's: preamble with 0xFD before the tag
/// stream, then 0x80=3, 0x85 (3 bytes), 0x82=2, 0xA0=0x0A, 0x8C=4, 0xFF.
pub const SYN_PT: &[u8] = &[0x50, 0x54, 0x06, 0x00, 0x06, 0x01, 0x65, 0x13, 0x01, 0x7F, 0xFD,
    0x80, 0x01, 0x03, 0x85, 0x03, 0x00, 0x00, 0x00, 0x82, 0x01, 0x02, 0xA0, 0x01, 0x0A, 0x8C, 0x01, 0x04, 0xFF, 0x00];

/// One stream: SCHl + PT (0x85 patched to `samples`), SCCl count 1, one SCDl
/// whose data is `data_per_ch` zero bytes per channel, SCEl, padded with
/// zeros to a 128-byte multiple.
pub fn synthetic_stream(pt: &[u8], samples: u32, data_per_ch: usize) -> Vec<u8> {
    let pt = crate::soundtrack::asf::patch_pt_tag(pt, 0x85, u64::from(samples)).unwrap();
    let mut s = Vec::new();
    s.extend_from_slice(b"SCHl"); s.extend_from_slice(&((8 + pt.len()) as u32).to_le_bytes()); s.extend_from_slice(&pt);
    s.extend_from_slice(b"SCCl"); s.extend_from_slice(&12u32.to_le_bytes()); s.extend_from_slice(&1u32.to_be_bytes());
    s.extend_from_slice(b"SCDl"); s.extend_from_slice(&((20 + 2 * data_per_ch) as u32).to_le_bytes());
    s.extend_from_slice(&samples.to_be_bytes()); s.extend_from_slice(&0u32.to_be_bytes()); s.extend_from_slice(&(data_per_ch as u32).to_be_bytes());
    s.extend(std::iter::repeat(0u8).take(2 * data_per_ch));
    s.extend_from_slice(b"SCEl"); s.extend_from_slice(&8u32.to_le_bytes());
    while s.len() % 128 != 0 { s.push(0); }
    s
}

/// pathfind.dat at toy size: TERF preamble, DIR1 with three entries, PFDx at
/// 0x2000 whose table pointer lands the TOC at 0x1138C, ASF at 0x13000 with
/// 876 streams of `data_per_ch` bytes per channel, TOC filled correctly.
pub fn synthetic_pathfind(data_per_ch: usize) -> Vec<u8> {
    let streams: Vec<Vec<u8>> = (0..crate::soundtrack::STREAM_COUNT).map(|k| synthetic_stream(SYN_PT, 2400 + k as u32 % 7, data_per_ch)).collect();
    let mut asf: Vec<u8> = streams.concat();
    while asf.len() % 0x800 != 0 { asf.push(0); }
    let mut d = vec![0u8; 0x13000];
    d[0..4].copy_from_slice(b"TERF");
    d[0x0E..0x10].copy_from_slice(&3u16.to_be_bytes());
    // DIR1 entries {offset, size}: file0 at 0x0, file1 at 0x1000, file2 (ASF) at 0x12000 -> data 0x13000
    d[0x808..0x80C].copy_from_slice(&0u32.to_be_bytes());       d[0x80C..0x810].copy_from_slice(&0x1000u32.to_be_bytes());
    d[0x810..0x814].copy_from_slice(&0x1000u32.to_be_bytes());  d[0x814..0x818].copy_from_slice(&0x11000u32.to_be_bytes());
    d[0x818..0x81C].copy_from_slice(&0x12000u32.to_be_bytes()); d[0x81C..0x820].copy_from_slice(&(asf.len() as u32).to_be_bytes());
    d[0x1004..0x1008].copy_from_slice(&((0x12000 + asf.len()) as u32).to_be_bytes());
    d[0x2000..0x2004].copy_from_slice(b"PFDx");
    d[0x2034..0x2038].copy_from_slice(&0xF38Cu32.to_be_bytes());
    let mut off = 0usize;
    for (k, s) in streams.iter().enumerate() {
        let o = 0x1138C + 8 * k;
        d[o..o + 4].copy_from_slice(&((off >> 7) as u32).to_be_bytes());
        let samples = crate::soundtrack::asf::pt_tag(crate::soundtrack::asf::pt_bytes(s), 0x85).unwrap();
        d[o + 4..o + 8].copy_from_slice(&(((samples as f64) / 24.0).round() as u32).to_be_bytes());
        off += s.len();
    }
    d.extend_from_slice(&asf);
    d
}
```

- [ ] **Step 3: Write `asf.rs`**

```rust
//! EA SCHl ASF: pure concatenation of SCHl..SCCl..SCDl*..SCEl streams, no
//! table of contents. Slot s owns 40 consecutive streams (see slot_first_stream).
use crate::soundtrack::{slot_first_stream, Slot, SEGMENTS_PER_SONG, STREAM_COUNT};
use crate::platform::{Error, Result};

fn invalid(msg: String) -> Error { Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg)) }

pub fn scan_schl(asf: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= asf.len() {
        match asf[i..].windows(4).position(|w| w == b"SCHl") {
            Some(p) => { out.push(i + p); i += p + 4; }
            None => break,
        }
    }
    out
}

fn le32(b: &[u8], o: usize) -> Option<u32> { b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]])) }
fn be32(b: &[u8], o: usize) -> Option<u32> { b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]])) }

/// Bytes from `start` through the SCEl chunk, walking chunk sizes.
pub fn stream_len(asf: &[u8], start: usize) -> Option<usize> {
    let mut p = start;
    loop {
        let tag = asf.get(p..p + 4)?;
        let size = le32(asf, p + 4)? as usize;
        if size < 8 { return None; }
        p += size;
        if tag == b"SCEl" { return Some(p - start); }
    }
}

pub fn slot_range(positions: &[usize], asf_len: usize, slot: Slot) -> (usize, usize) {
    let first = slot_first_stream(slot);
    let end = first + SEGMENTS_PER_SONG;
    (positions[first], if end < positions.len() { positions[end] } else { asf_len })
}

fn positions_checked(asf: &[u8]) -> Result<Vec<usize>> {
    let pos = scan_schl(asf);
    if pos.len() != STREAM_COUNT { return Err(invalid(format!("the ASF has {} streams, expected {STREAM_COUNT}", pos.len()))); }
    Ok(pos)
}

pub fn slot_blob(asf: &[u8], slot: Slot) -> Result<Vec<u8>> {
    let pos = positions_checked(asf)?;
    let (a, b) = slot_range(&pos, asf.len(), slot);
    Ok(asf[a..b].to_vec())
}

/// Replace a slot's 40 streams; the result is padded to a 0x800 multiple.
pub fn splice(asf: &[u8], slot: Slot, blob: &[u8]) -> Result<Vec<u8>> {
    validate_blob(blob).map_err(invalid)?;
    let pos = positions_checked(asf)?;
    let (a, b) = slot_range(&pos, asf.len(), slot);
    let mut out = Vec::with_capacity(asf.len() + blob.len());
    out.extend_from_slice(&asf[..a]);
    out.extend_from_slice(blob);
    out.extend_from_slice(&asf[b..]);
    while out.len() % 0x800 != 0 { out.push(0); }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobInfo { pub streams: usize, pub samples: u64, pub duration_ms: u64 }

/// A game-ready slot blob: exactly 40 streams, each starting on a 128-byte
/// boundary of the blob, each closed by SCEl, each with a PT 0x85 count.
pub fn validate_blob(blob: &[u8]) -> std::result::Result<BlobInfo, String> {
    let pos = scan_schl(blob);
    if pos.len() != SEGMENTS_PER_SONG { return Err(format!("{} streams, expected {SEGMENTS_PER_SONG}", pos.len())); }
    if pos[0] != 0 { return Err("the blob does not start with SCHl".into()); }
    let mut samples = 0u64;
    for (k, &p) in pos.iter().enumerate() {
        if p % 128 != 0 { return Err(format!("stream {k} starts at 0x{p:X}, not 128-byte aligned")); }
        let len = stream_len(blob, p).ok_or_else(|| format!("stream {k} has no SCEl"))?;
        let next = pos.get(k + 1).copied().unwrap_or(blob.len());
        if p + len > next { return Err(format!("stream {k} runs into stream {}", k + 1)); }
        samples += pt_tag(pt_bytes(&blob[p..]), 0x85).ok_or_else(|| format!("stream {k} has no PT 0x85 sample count"))?;
    }
    Ok(BlobInfo { streams: pos.len(), samples, duration_ms: samples * 1000 / 24000 })
}

/// The PT payload of a stream (after the 8-byte SCHl header).
pub fn pt_bytes(stream: &[u8]) -> &[u8] {
    let size = le32(stream, 4).unwrap_or(8) as usize;
    &stream[8..size.min(stream.len())]
}

/// Tag stream begins at the first 0x80 byte; the preamble can contain 0xFD.
fn tag_start(pt: &[u8]) -> Option<usize> { pt.iter().position(|&b| b == 0x80) }

pub fn pt_tag(pt: &[u8], tag: u8) -> Option<u64> {
    let mut i = tag_start(pt)?;
    while i + 1 < pt.len() {
        let t = pt[i];
        if t == 0xFF { return None; }
        let ln = pt[i + 1] as usize;
        if t == tag {
            let v = pt.get(i + 2..i + 2 + ln)?;
            return Some(v.iter().fold(0u64, |acc, &b| (acc << 8) | u64::from(b)));
        }
        i += 2 + ln;
    }
    None
}

pub fn patch_pt_tag(pt: &[u8], tag: u8, value: u64) -> std::result::Result<Vec<u8>, String> {
    let mut out = pt.to_vec();
    let mut i = tag_start(pt).ok_or("no tag stream in PT")?;
    while i + 1 < out.len() {
        let t = out[i];
        if t == 0xFF { break; }
        let ln = out[i + 1] as usize;
        if t == tag {
            if ln == 0 || ln > 8 || (ln < 8 && value >= 1u64 << (8 * ln)) { return Err(format!("value {value} does not fit a {ln}-byte field")); }
            let bytes = value.to_be_bytes();
            out[i + 2..i + 2 + ln].copy_from_slice(&bytes[8 - ln..]);
            return Ok(out);
        }
        i += 2 + ln;
    }
    Err(format!("no 0x{tag:02X} field in PT"))
}

/// Sum of the SCDl per-block sample fields (the engine credits these).
pub fn scdl_sample_sum(stream: &[u8]) -> u64 {
    let mut p = 0; let mut total = 0u64;
    while p + 8 <= stream.len() {
        let tag = &stream[p..p + 4];
        let Some(size) = le32(stream, p + 4) else { break };
        if size == 0 { break; }
        if tag == b"SCDl" { total += u64::from(be32(stream, p + 8).unwrap_or(0)); }
        if tag == b"SCEl" { break; }
        p += size as usize;
    }
    total
}
```

- [ ] **Step 4: Write `pathfind.rs`**

```rust
//! pathfind.dat: EA TERF with three raw sub-files; sub-file 2 is the ASF and
//! runs to end of file. The PFDx blob at 0x2000 holds the 876-entry position
//! table the streamer seeks by.
use crate::platform::{Error, Result};
use crate::soundtrack::{asf, slot_first_stream, Slot, SEGMENTS_PER_SONG, STREAM_COUNT};

const DIR1_FILE2_OFFSET: usize = 0x818;
const DIR1_FILE2_SIZE: usize = 0x81C;
const DATA_INFO_SIZE: usize = 0x1004;
const PFDX: usize = 0x2000;

fn invalid(msg: String) -> Error { Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg)) }
fn be32(b: &[u8], o: usize) -> Result<u32> { b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]])).ok_or_else(|| invalid(format!("pathfind.dat is shorter than 0x{o:X}"))) }
fn put32(b: &mut [u8], o: usize, v: u32) { b[o..o + 4].copy_from_slice(&v.to_be_bytes()); }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pathfind { pub asf_offset: usize, pub asf_size: usize, pub toc_offset: usize }

impl Pathfind {
    pub fn parse(bytes: &[u8]) -> Result<Pathfind> {
        if bytes.get(0..4) != Some(b"TERF") { return Err(invalid("pathfind.dat does not start with TERF".into())); }
        if bytes.get(PFDX..PFDX + 4) != Some(b"PFDx") { return Err(invalid("no PFDx blob at 0x2000".into())); }
        let asf_offset = 0x1000 + be32(bytes, DIR1_FILE2_OFFSET)? as usize;
        let asf_size = be32(bytes, DIR1_FILE2_SIZE)? as usize;
        let toc_offset = PFDX + be32(bytes, PFDX + 0x34)? as usize;
        if asf_offset + asf_size != bytes.len() { return Err(invalid(format!("ASF 0x{asf_offset:X}+0x{asf_size:X} does not end at EOF 0x{:X}", bytes.len()))); }
        if toc_offset + 8 * STREAM_COUNT > asf_offset { return Err(invalid("the PFDx table overlaps the ASF".into())); }
        Ok(Pathfind { asf_offset, asf_size, toc_offset })
    }
}

pub fn asf(bytes: &[u8]) -> &[u8] {
    match Pathfind::parse(bytes) { Ok(p) => &bytes[p.asf_offset..], Err(_) => &[] }
}

/// The file with a new ASF: DIR1 size and DATA INFO_SIZE move by the delta.
pub fn with_asf(bytes: &[u8], new_asf: &[u8]) -> Result<Vec<u8>> {
    let p = Pathfind::parse(bytes)?;
    let mut out = bytes[..p.asf_offset].to_vec();
    out.extend_from_slice(new_asf);
    let delta = new_asf.len() as i64 - p.asf_size as i64;
    put32(&mut out, DIR1_FILE2_SIZE, new_asf.len() as u32);
    let info = be32(bytes, DATA_INFO_SIZE)? as i64 + delta;
    put32(&mut out, DATA_INFO_SIZE, info as u32);
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TocReport { pub w0_rewritten: usize }

/// Rewrite every w0 from a real SCHl scan; recompute w1 for `changed` slots
/// from each segment's PT 0x85 (ms = round(samples / 24)).
pub fn rewrite_toc(bytes: &mut Vec<u8>, changed: &[Slot]) -> Result<TocReport> {
    let p = Pathfind::parse(bytes)?;
    let pos = asf::scan_schl(&bytes[p.asf_offset..]);
    if pos.len() != STREAM_COUNT { return Err(invalid(format!("{} streams, expected {STREAM_COUNT}; the layout must keep 40 segments per song", pos.len()))); }
    let mut rewritten = 0;
    for (k, &rel) in pos.iter().enumerate() {
        if rel % 128 != 0 { return Err(invalid(format!("stream {k} at 0x{rel:X} is not 128-byte aligned; the engine cannot seek to it"))); }
        let o = p.toc_offset + 8 * k;
        let w0 = (rel >> 7) as u32;
        if be32(bytes, o)? != w0 { put32(bytes, o, w0); rewritten += 1; }
    }
    for &slot in changed {
        let first = slot_first_stream(slot);
        for k in first..first + SEGMENTS_PER_SONG {
            let stream = &bytes[p.asf_offset + pos[k]..];
            let samples = asf::pt_tag(asf::pt_bytes(stream), 0x85).ok_or_else(|| invalid(format!("no PT 0x85 in stream {k}")))?;
            let w1 = ((samples as f64) / 24.0).round() as u32;
            put32(bytes, p.toc_offset + 8 * k + 4, w1);
        }
    }
    Ok(TocReport { w0_rewritten: rewritten })
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reborn-core soundtrack::asf soundtrack::pathfind`
Expected: all pass.

- [ ] **Step 6: Oracle check against the stock file (env-gated test in pathfind.rs)**

```rust
#[test]
fn oracle_stock_pathfind_parses_and_toc_is_already_right() {
    let Some(clean) = std::env::var_os("NSR_CLEAN") else { eprintln!("skipped: NSR_CLEAN unset"); return };
    let path = std::path::PathBuf::from(clean).join("GameCube Version").join("root").join("pathfind.dat");
    let mut bytes = std::fs::read(&path).unwrap();
    let p = Pathfind::parse(&bytes).unwrap();
    assert_eq!(p.asf_offset, 0x13000);
    assert_eq!(p.toc_offset, 0x1138C);
    let before = bytes.clone();
    let rep = rewrite_toc(&mut bytes, &[]).unwrap();
    assert_eq!(rep.w0_rewritten, 0, "stock w0 must already match the scan");
    assert_eq!(bytes, before);
}
```

Run: `$env:NSR_CLEAN="C:\Users\Adam\nflstreet2_clean"; cargo test -p reborn-core --release oracle_stock_pathfind`
Expected: pass. Note: root/pathfind.dat may carry Adam's custom songs; w0 still matches by construction.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-core/src/soundtrack
git commit -m "soundtrack: ASF slot splice, PT tags, pathfind TERF and PFDx table"
```

---

### Task 4: Banner records (`eatx.rs`)

**Files:**
- Create: `crates/reborn-core/src/soundtrack/eatx.rs`
- Modify: `testing.rs` (add `synthetic_template`)

**Interfaces:**
- `pub const RECORD_STRIDE: usize = 156; pub const FIELD_LEN: usize = 50; pub const MAX_TEXT: usize = 49;`
- `pub struct Record { pub title: String, pub album: String, pub artist: String }`
- `pub fn locate(template: &[u8]) -> Result<usize>` offset of record 0: find `DINS` such that `ITNS` at +16, `LANS` at +32, `RANS` at +48, `XINS` at +64; record 0 at +72; verify u32BE SNID of record k == k + 1 for all 21.
- `pub fn read_records(template: &[u8]) -> Result<Vec<Record>>` (21 entries).
- `pub fn write_record(template: &mut [u8], slot: Slot, record: &Record) -> Result<()>`; refuses text over 49 bytes in latin1 with `Refusal::TooLongText`; non-latin1 characters become `?`.

- [ ] **Step 1: Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::synthetic_template;
    use crate::soundtrack::Slot;

    #[test]
    fn locates_reads_and_writes_records() {
        let mut t = synthetic_template();
        assert_eq!(locate(&t).unwrap(), 0x1799C);
        let recs = read_records(&t).unwrap();
        assert_eq!(recs.len(), 21);
        assert_eq!(recs[0].title, "Song 1");
        write_record(&mut t, Slot::new(7).unwrap(), &Record { title: "Unthinkable".into(), album: "The Element of Freedom".into(), artist: "Alicia Keys".into() }).unwrap();
        let recs = read_records(&t).unwrap();
        assert_eq!(recs[6].artist, "Alicia Keys");
        assert_eq!(recs[5].title, "Song 6", "neighbors untouched");
        assert_eq!(&t[0x1799C + 6 * 156..0x1799C + 6 * 156 + 4], &7u32.to_be_bytes(), "SNID kept");
    }

    #[test]
    fn refuses_text_that_does_not_fit() {
        let mut t = synthetic_template();
        let long = "x".repeat(50);
        let err = write_record(&mut t, Slot::new(1).unwrap(), &Record { title: long, album: String::new(), artist: String::new() }).unwrap_err();
        assert!(err.to_string().contains("49"));
    }

    #[test]
    fn a_moved_table_is_still_found() {
        let t = synthetic_template();
        let mut moved = vec![0u8; 0x100];
        moved.extend_from_slice(&t);
        assert_eq!(locate(&moved).unwrap(), 0x1799C + 0x100);
    }

    #[test]
    fn oracle_stock_template_lists_scarred_first() {
        let Some(clean) = std::env::var_os("NSR_CLEAN") else { eprintln!("skipped: NSR_CLEAN unset"); return };
        let t = std::fs::read(std::path::PathBuf::from(clean).join("GameCube Version").join("root").join("template.dat")).unwrap();
        assert_eq!(locate(&t).unwrap(), 0x1799C);
        let recs = read_records(&t).unwrap();
        assert_eq!(recs[0].title, "Scarred");
    }
}
```

`synthetic_template()`: a 0x18700-byte zero buffer with the five field definitions at 0x17954 copied from the dump in the spec (`44 49 4E 53 00 00 00 20 00 00 00 00 00 00 00 20 | 49 54 4E 53 00 00 01 90 00 00 00 00 00 00 01 B0 | 4C 41 4E 53 00 00 01 90 00 00 00 00 00 00 03 40 | 52 41 4E 53 00 00 01 90 00 00 00 03 00 00 04 D0 | 58 49 4E 53 00 00 00 05`) and 21 records at 0x1799C: SNID k+1, title `Song k+1`, album `Album k+1`, artist `Artist k+1`, SNIX byte = (k+1) << 3.

- [ ] **Step 2: Implement `eatx.rs`**

```rust
//! The EATX banner table in template.dat: 21 inline records, 156 bytes each.
use crate::platform::{Error, Result};
use crate::soundtrack::{Refusal, Slot, SONG_SLOTS};

pub const RECORD_STRIDE: usize = 156;
pub const FIELD_LEN: usize = 50;
pub const MAX_TEXT: usize = 49;
const F_TITLE: usize = 4; const F_ALBUM: usize = 54; const F_ARTIST: usize = 104;
const DEFS: [(&[u8; 4], usize); 5] = [(b"DINS", 0), (b"ITNS", 16), (b"LANS", 32), (b"RANS", 48), (b"XINS", 64)];
const RECORDS_AFTER_DEFS: usize = 72;

fn invalid(msg: String) -> Error { Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg)) }

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record { pub title: String, pub album: String, pub artist: String }

pub fn locate(template: &[u8]) -> Result<usize> {
    let mut i = 0;
    while let Some(p) = template[i..].windows(4).position(|w| w == b"DINS") {
        let base = i + p;
        let defs_ok = DEFS.iter().all(|(tag, off)| template.get(base + off..base + off + 4) == Some(&tag[..]));
        if defs_ok {
            let rec0 = base + RECORDS_AFTER_DEFS;
            let ids_ok = (0..usize::from(SONG_SLOTS)).all(|k| {
                let o = rec0 + k * RECORD_STRIDE;
                template.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) == Some(k as u32 + 1)
            });
            if ids_ok { return Ok(rec0); }
        }
        i = base + 4;
    }
    Err(invalid("the EATX music table was not found in template.dat".into()))
}

fn read_str(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}

pub fn read_records(template: &[u8]) -> Result<Vec<Record>> {
    let rec0 = locate(template)?;
    Ok((0..usize::from(SONG_SLOTS)).map(|k| {
        let o = rec0 + k * RECORD_STRIDE;
        Record {
            title: read_str(&template[o + F_TITLE..o + F_TITLE + FIELD_LEN]),
            album: read_str(&template[o + F_ALBUM..o + F_ALBUM + FIELD_LEN]),
            artist: read_str(&template[o + F_ARTIST..o + F_ARTIST + FIELD_LEN]),
        }
    }).collect())
}

fn encode(field: &'static str, s: &str) -> Result<Vec<u8>> {
    let bytes: Vec<u8> = s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect();
    if bytes.len() > MAX_TEXT { return Err(Refusal::TooLongText { field, len: bytes.len() }.into()); }
    Ok(bytes)
}

pub fn write_record(template: &mut [u8], slot: Slot, record: &Record) -> Result<()> {
    let rec0 = locate(template)?;
    let o = rec0 + slot.index() * RECORD_STRIDE;
    for (off, field, text) in [(F_TITLE, "title", &record.title), (F_ALBUM, "album", &record.album), (F_ARTIST, "artist", &record.artist)] {
        let enc = encode(field, text)?;
        let dst = &mut template[o + off..o + off + FIELD_LEN];
        dst.fill(0);
        dst[..enc.len()].copy_from_slice(&enc);
    }
    Ok(())
}
```

- [ ] **Step 3: Run tests, including the oracle**

Run: `cargo test -p reborn-core soundtrack::eatx` and with `NSR_CLEAN` set.
Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add crates/reborn-core/src/soundtrack
git commit -m "soundtrack: EATX banner records located by field tags"
```

---

### Task 5: Pack format (`pack.rs`)

**Files:**
- Create: `crates/reborn-core/src/soundtrack/pack.rs`

**Interfaces:**
- `pub const EXT_V2: &str = "nsrmusic"; pub const EXT_V1: &str = "nsmp";`
- `pub struct Song { pub blob: String, pub sha256: Option<String>, pub bytes: u64, pub duration_ms: Option<u64>, pub title: String, pub artist: String, pub album: String }`
- `pub struct Manifest { pub magic: String, pub format: u32, pub id: String, pub name: String, pub author: String, pub description: String, pub created: String, pub game: String, pub songs: BTreeMap<u8, Song> }` (serde; format 1 files have `slots` instead of `songs` and `seconds` instead of `duration_ms`: the reader maps them).
- `pub struct Pack { pub path: PathBuf, pub manifest: Manifest }`
- `pub fn read_pack(path: &Path) -> Result<Pack>` validates magic `NSRMUSIC` (format 2) or `NSMP` (format 1), format <= 2, every song slot 1..21, blob entry present and, when a hash is given, matching.
- `pub fn read_blob(pack: &Pack, slot: Slot) -> Result<Vec<u8>>` (validated by `asf::validate_blob`).
- `pub struct SongInput { pub slot: Slot, pub blob: Vec<u8>, pub title: String, pub artist: String, pub album: String }`
- `pub fn write_pack(out: &Path, name: &str, author: &str, description: &str, songs: &[SongInput]) -> Result<Manifest>` (stored entries, `pack.json` pretty, `id` = first 16 hex of the SHA-256 over all blobs in slot order, `created` = today `YYYY-MM-DD`).

- [ ] **Step 1: Tests**

```rust
#[test]
fn write_then_read_roundtrips_format_2() {
    let dir = tempfile::tempdir().unwrap();
    let blob: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 2400, 256)).collect();
    let out = dir.path().join("mine.nsrmusic");
    let man = write_pack(&out, "Mine", "adam", "test", &[SongInput { slot: Slot::new(3).unwrap(), blob: blob.clone(), title: "T".into(), artist: "A".into(), album: "L".into() }]).unwrap();
    assert_eq!(man.format, 2);
    let pack = read_pack(&out).unwrap();
    assert_eq!(pack.manifest.songs[&3].title, "T");
    assert_eq!(pack.manifest.songs[&3].duration_ms, Some(4000));
    assert_eq!(read_blob(&pack, Slot::new(3).unwrap()).unwrap(), blob);
}

#[test]
fn reads_a_format_1_nsmp() {
    // pack.json as music_pack.py writes it
    let dir = tempfile::tempdir().unwrap();
    let blob: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 2400, 256)).collect();
    let out = dir.path().join("old.nsmp");
    let f = std::fs::File::create(&out).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("slot21.asf", opts).unwrap(); std::io::Write::write_all(&mut z, &blob).unwrap();
    z.start_file("pack.json", opts).unwrap();
    std::io::Write::write_all(&mut z, br#"{"magic":"NSMP","format":1,"name":"Old","author":"x","description":"","created":"2026-07-02","game":"NFL Street 2 (GC)","slots":{"21":{"title":"Unthinkable","artist":"Alicia Keys","album":"","blob":"slot21.asf","bytes":1,"seconds":282}}}"#).unwrap();
    z.finish().unwrap();
    let pack = read_pack(&out).unwrap();
    assert_eq!(pack.manifest.format, 1);
    assert_eq!(pack.manifest.songs[&21].title, "Unthinkable");
    assert_eq!(pack.manifest.songs[&21].duration_ms, Some(282_000));
    assert_eq!(read_blob(&pack, Slot::new(21).unwrap()).unwrap(), blob);
}

#[test]
fn rejects_bad_magic_wrong_slot_and_hash_mismatch() { /* three small packs, each expect Err with the reason word */ }
```

- [ ] **Step 2: Implement** with `zip::ZipArchive::new(File)`, `by_name`, serde_json for `pack.json`; for format 1 parse into a private `V1Manifest { slots: BTreeMap<String, V1Slot { title, artist, album, blob, bytes, seconds }> }` and convert. Hash check: `sha2` over the blob, compare hex lowercase. Refusals use `Refusal::InvalidPack { path, reason }`.

- [ ] **Step 3: Run, commit**

```bash
git add crates/reborn-core/src/soundtrack/pack.rs
git commit -m "soundtrack: .nsrmusic format 2 read and write, .nsmp format 1 read"
```

---

### Task 6: State file (`state.rs`)

**Files:**
- Create: `crates/reborn-core/src/soundtrack/state.rs`

**Interfaces:**
- Paths (all under the data root): `music_dir(root)`, `sources_dir(root)` = `music/sources`, `packs_dir(root)` = `music/packs`, `encoded_dir(root)` = `music/encoded`, `soundtrack_path(root)` = `music/soundtrack.json`, `rebuild_state_path(root)` = `music/state.json`, `encoded_blob_path(root, track_id, slot)` = `music/encoded/<id>-s<slot>.asf`.
- `pub fn track_id(bytes: &[u8]) -> String` first 16 hex of SHA-256.
- Types (serde, `#[serde(tag = "kind", rename_all = "snake_case")]` on the enums):

```rust
pub enum Base { Stock, Package { id: String, version: String, pack: String } }
pub enum Source { Stock, UserFile { track: String, file: String }, PackStream { pack: String, slot: u8 } }
pub struct Entry { pub source: Source, pub title: String, pub artist: String, pub album: String, pub duration_ms: Option<u64> }
pub struct Soundtrack { pub schema: String, pub enabled: bool, pub base: Base, pub slots: BTreeMap<u8, Entry>, pub pending: BTreeMap<u8, Entry>, pub base_sha256: Option<String>, pub rebuilt_sha256: Option<String> }
```

- `Soundtrack::default()` = schema `nsr.soundtrack.v2`, enabled true, base Stock, empty maps.
- `Soundtrack::load(root) -> Result<Soundtrack>` (absent file = default; malformed = error); `save(&self, root) -> Result<()>` (create dir, pretty JSON, write to `.tmp` then rename).
- `Soundtrack::stage(&mut self, slot: Slot, entry: Entry)`, `unstage(&mut self, slot)`, `has_pending()`.
- `pub enum Effective { Stock, Blob { blob: BlobRef, title: String, artist: String, album: String } }` with `pub enum BlobRef { Encoded(PathBuf), Pack { pack: PathBuf, slot: Slot } }`.
- `Soundtrack::effective(&self, root: &Path, base_pack: Option<&pack::Pack>, include_pending: bool) -> BTreeMap<Slot, Effective>`: for each slot 1..21: pending (if included) > slots > base pack song > Stock. A `Source::Stock` entry yields `Effective::Stock`. A `UserFile` yields `Encoded(encoded_blob_path(root, track, slot))`; a `PackStream` yields `Pack { root.join(pack), slot }`.
- `Soundtrack::commit_pending(&mut self)` moves pending into slots.
- `Soundtrack::is_all_stock(effective) -> bool`.

- [ ] **Step 1: Tests**: default saves and loads back; stage then effective prefers pending; base package song shows when no slot entry; explicit Stock entry overrides base; commit_pending moves entries; malformed JSON is an error, absent file is default.

- [ ] **Step 2: Implement.** Keep it plain serde; `effective` builds the map for all 21 slots so callers never see a hole.

- [ ] **Step 3: Run, commit**

```bash
git add crates/reborn-core/src/soundtrack/state.rs
git commit -m "soundtrack: soundtrack.json state, paths, effective slots"
```

---

### Task 7: Apply pipeline for packs and resets (`apply.rs`, part 1)

**Files:**
- Create: `crates/reborn-core/src/soundtrack/apply.rs`
- Create: `Tools/oracle-pack-import.py`

**Interfaces:**
- `pub enum Progress { Reading, Splicing { slot: u8 }, Building { done: u64, total: u64 }, Hashing }`
- `pub struct Applied { pub image: PathBuf, pub before: Option<Sha256Hex>, pub after: Sha256Hex, pub slots_changed: Vec<u8> }`
- `pub enum Outcome { Applied(Applied), Unchanged { image: PathBuf, sha256: Sha256Hex }, Refused(Refusal) }`
- `pub fn images(data_root: &Path) -> Result<(PathBuf /*pristine*/, PathBuf /*booted*/)>`: pristine from `game_image::find_pristine`, booted from `game_image::booted_path`; `Refusal::NoPristine` / `NoBooted` when missing.
- `pub fn build_files(pristine_pathfind: &[u8], pristine_template: &[u8], booted_template: &[u8], effective: &BTreeMap<Slot, Effective>, on: &mut dyn FnMut(Progress)) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)>` returns (new pathfind, new template, changed slots as bytes). Pure, no disk beyond reading blobs named in `effective`. Stock slots take the pristine blob and the pristine banner record.
- `pub fn apply(platform: &Platform, data_root: &Path, on: &mut dyn FnMut(Progress)) -> Result<Outcome>`: loads state, resolves base pack, computes effective with pending, refuses if not enabled or all stock (callers use `revert` then), refuses when the booted image is locked, builds files, repacks to `staging/music/NSR.iso.part` on the same volume, hashes, records into `RebuildState` at `rebuild_state_path` under key `NSR.iso` before the swap and undoes on failure (same order as `roster::apply`), swaps with `platform.files.atomic_replace`, then `commit_pending`, sets `rebuilt_sha256` and `base_sha256` (pristine folder hash), saves.
- `pub fn stage_pack(data_root: &Path, path: &Path) -> Result<Vec<u8>>`: copies the pack into `music/packs/<file name>` (if a different file with that name exists, suffix `-2`, `-3`), stages every song as `Source::PackStream`, returns the slots staged.
- `pub fn stage_stock(data_root: &Path, slot: Slot) -> Result<()>`.
- `pub fn stage_details(data_root: &Path, slot: Slot, title, artist, album) -> Result<()>` (edits pending if present, else copies the slot entry into pending with the new text; for a slot with no entry, stages `Source::Stock` with text, which the writer applies as stock audio with custom text).
- `pub fn export(data_root: &Path, pristine_pathfind: &[u8], slots: &[Slot], out: &Path, name, author, description) -> Result<pack::Manifest>`: blob per slot from effective (cached, pack, or pristine).

- [ ] **Step 1: Tests with synthetic disc + synthetic pathfind/template**

```rust
#[test]
fn build_files_swaps_one_slot_and_leaves_the_rest_stock() {
    let pf = synthetic_pathfind(256);
    let tpl = synthetic_template();
    let dir = tempfile::tempdir().unwrap();
    let blob: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 4800, 512)).collect();
    let blob_path = dir.path().join("x.asf");
    std::fs::write(&blob_path, &blob).unwrap();
    let mut eff: BTreeMap<Slot, Effective> = Slot::all().map(|s| (s, Effective::Stock)).collect();
    eff.insert(Slot::new(9).unwrap(), Effective::Blob { blob: BlobRef::Encoded(blob_path), title: "New".into(), artist: "Me".into(), album: "".into() });
    let (new_pf, new_tpl, changed) = build_files(&pf, &tpl, &tpl, &eff, &mut |_| {}).unwrap();
    assert_eq!(changed, vec![9]);
    assert_eq!(asf::slot_blob(pathfind::asf(&new_pf), Slot::new(9).unwrap()).unwrap(), blob);
    assert_eq!(asf::slot_blob(pathfind::asf(&new_pf), Slot::new(10).unwrap()).unwrap(), asf::slot_blob(pathfind::asf(&pf), Slot::new(10).unwrap()).unwrap());
    let recs = eatx::read_records(&new_tpl).unwrap();
    assert_eq!(recs[8].title, "New");
    assert_eq!(recs[9].title, "Song 10");
    pathfind::Pathfind::parse(&new_pf).unwrap();
}

#[test]
fn apply_rebuilds_the_booted_image_records_the_hash_and_commits_pending() {
    // data root with library/imported/<sha>/NSR.iso (synthetic disc holding synthetic pathfind+template)
    // and library/offline/NSR.iso as a copy; stage a pack; apply with repack_with_size via a test seam
    // (apply takes `disc_size: Option<u64>` internally through `apply_with_size`); assert:
    // booted changed, pristine unchanged, RebuildState has NSR.iso == returned hash,
    // soundtrack.json pending empty, slots has the pack slot, rebuilt_sha256 set.
}

#[test]
fn apply_refuses_when_everything_is_stock() { /* Outcome::Refused(Refusal::NothingStaged) */ }
```

- [ ] **Step 2: Implement `build_files`**

```rust
pub fn build_files(pristine_pathfind: &[u8], pristine_template: &[u8], booted_template: &[u8], effective: &BTreeMap<Slot, Effective>, on: &mut dyn FnMut(Progress)) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    on(Progress::Reading);
    let stock_records = eatx::read_records(pristine_template)?;
    let mut asf_bytes = pathfind::asf(pristine_pathfind).to_vec();
    let mut template = booted_template.to_vec();
    let mut changed = Vec::new();
    for (slot, eff) in effective {
        match eff {
            Effective::Stock => {
                // pristine ASF already carries the stock blob; restore the stock text
                eatx::write_record(&mut template, *slot, &stock_records[slot.index()])?;
            }
            Effective::Blob { blob, title, artist, album } => {
                on(Progress::Splicing { slot: slot.get() });
                let bytes = match blob {
                    BlobRef::Encoded(p) => std::fs::read(p).map_err(|source| Error::Io { path: p.clone(), source })?,
                    BlobRef::Pack { pack, slot: s } => pack::read_blob(&pack::read_pack(pack)?, *s)?,
                };
                asf_bytes = asf::splice(&asf_bytes, *slot, &bytes)?;
                let rec = eatx::Record { title: title.clone(), album: album.clone(), artist: artist.clone() };
                eatx::write_record(&mut template, *slot, &rec)?;
                changed.push(slot.get());
            }
        }
    }
    let mut new_pf = pathfind::with_asf(pristine_pathfind, &asf_bytes)?;
    let slots: Vec<Slot> = changed.iter().map(|&n| Slot::new(n).expect("from a Slot")).collect();
    pathfind::rewrite_toc(&mut new_pf, &slots)?;
    Ok((new_pf, template, changed))
}
```

Splicing slots in ascending order is safe because each splice re-scans; positions before the slot are unchanged.

- [ ] **Step 3: Implement `apply`** following `roster::apply` line by line for staging dir (`staging/music`), same-volume check, lock check, record-then-swap. Repack with `gcm::repack` on the booted `Disc`, replacements `{ "pathfind.dat": new_pf, "template.dat": new_tpl }`. Verify the written size equals `DISC_SIZE`. Before the swap, re-open the `.part` with `gcm::Disc::open` and check `read_file("pathfind.dat")` length equals `new_pf.len()` (a cheap readback gate).

- [ ] **Step 4: Oracle 2 script `Tools/oracle-pack-import.py`**

```python
# Apply a pack with the proven Python chain onto COPIES of the stock files, so
# the Rust build_files output can be compared byte for byte.
#   py -3 Tools/oracle-pack-import.py <pack> <pathfind.dat> <template.dat> <out_dir>
import os, shutil, sys
CLEAN = os.environ.get('NSR_CLEAN', r'C:\Users\Adam\nflstreet2_clean')
sys.path.insert(0, os.path.join(CLEAN, 'tools'))
import music_pack as P
pack, pf, tpl, out = sys.argv[1:5]
os.makedirs(out, exist_ok=True)
pf2 = os.path.join(out, 'pathfind.dat'); tpl2 = os.path.join(out, 'template.dat')
shutil.copy2(pf, pf2); shutil.copy2(tpl, tpl2)
P.import_pack(pack, log=print, do_build=False, pathfind=pf2, template=tpl2)
print('wrote', pf2, tpl2)
```

Oracle test (env-gated, `#[ignore]`-free but skipping without `NSR_CLEAN`): export slot 21 from the pristine pathfind with `pack::write_pack` into a temp `.nsrmusic`, but with slot 21's blob replaced by slot 3's blob (a real different song, same 40-stream shape) so the import changes bytes; run the script; run `build_files` with the same effective map; assert both pathfind.dat and template.dat equal. Note `music_pack.import_pack` calls `fix_pfdx_toc.py` with `--slot` for each slot, which recomputes w1 for that slot only; `build_files` does the same set.

Run: `$env:NSR_CLEAN=...; cargo test -p reborn-core --release oracle_pack_import -- --nocapture`

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/soundtrack/apply.rs Tools/oracle-pack-import.py
git commit -m "soundtrack: apply pipeline for packs and resets, oracle against music_pack.py"
```

---

### Task 8: Revert to stock, enabled switch, CLI verbs (day 1 close)

**Files:**
- Modify: `crates/reborn-core/src/soundtrack/apply.rs` (add `revert`, `set_enabled`, `apply_or_revert`)
- Modify: `crates/reborn-cli/src/main.rs` (`Command::Music { what: MusicCommand }`)
- Modify: `docs/RUNNING_REBORNCTL.md`

**Interfaces:**
- `pub fn revert(platform: &Platform, data_root: &Path, hook: &content::pipeline::ImageHook, source: &dyn origin::ReleaseSource) -> Result<RevertOutcome>`: `game_image::establish(platform, data_root, &pristine.path)`, then `Updater::new(platform, data_root, source, ...)`.`apply_current_roster(hook)`; clears `rebuilt_sha256`, keeps `slots` (so turning the switch back on rebuilds the same order). Look at how `content_updates.rs` constructs an `Updater` (`run_check`) and copy the constructor arguments; if the constructor needs a live origin only for check/fetch, pass `DEFAULT_ORIGIN`.
- `pub fn apply_or_revert(platform, data_root, hook, source, on) -> Result<Outcome>`: enabled and any non-stock effective slot → `apply`; otherwise `revert` (reported as `Outcome::Applied` with `slots_changed` empty, or `Unchanged` when the booted image already equals pristine+roster, which `establish` cannot tell, so report `Applied`).
- `pub fn set_enabled(data_root, on: bool) -> Result<()>` saves the flag only; the caller then runs `apply_or_revert`.

- [ ] **Step 1: CLI**

```rust
/// Custom soundtrack: stage songs, apply, share packs.
Music {
    #[command(subcommand)]
    what: MusicCommand,
},

#[derive(Subcommand)]
enum MusicCommand {
    /// The 21 slots as the disc has them, what is staged, and the switch.
    Status,
    /// Stage a pack (.nsrmusic or .nsmp): every song it carries goes to its slot.
    Import { pack: PathBuf },
    /// Stage a slot back to the disc's own song.
    Reset { slot: u8 },
    /// Stage new banner text for a slot.
    Details { slot: u8, #[arg(long)] title: Option<String>, #[arg(long)] artist: Option<String>, #[arg(long)] album: Option<String> },
    /// Drop a staged change.
    Unstage { slot: u8 },
    /// Write a pack from slots as they are now.
    Export { out: PathBuf, #[arg(long, value_delimiter = ',')] slots: Vec<u8>, #[arg(long, default_value = "")] name: String, #[arg(long, default_value = "")] author: String, #[arg(long, default_value = "")] description: String },
    /// Turn the custom soundtrack on or off (rebuilds the disc).
    Enable, Disable,
    /// Build the disc from the staged slots. One rebuild.
    Apply,
    /// Write the staged slots into a dev root tree (pathfind.dat, template.dat) and stop.
    WriteTree { #[arg(long)] tree: PathBuf },
}
```

Every verb takes the global `--data-root` the CLI already has (check how `update content --data-root` is declared and reuse it). `Status` prints: switch, base, then 21 lines `slot  title , artist  [source]  (pending: …)`. `Apply` prints progress lines from `Progress`. `WriteTree` reads `<tree>/pathfind.dat` and `<tree>/template.dat` as both pristine and booted inputs (a dev tree is its own pristine), writes `.bak_pre_music_<date>` copies only if absent, writes the two files, and prints "now run tools/build_iso_fields.py".

- [ ] **Step 2: Manual check on a scratch data root**

```powershell
cargo build --workspace
$r = "$env:TEMP\nsr-music-root"; Remove-Item -Recurse -Force $r -ErrorAction SilentlyContinue
# lay out library/imported/<sha>/NSR.iso as a hardlink to the FIELDS disc and library/offline/NSR.iso as another hardlink
.\target\debug\rebornctl.exe music status --data-root $r
.\target\debug\rebornctl.exe music import "<a .nsmp exported with music_pack.py from root/, slot 21>" --data-root $r
.\target\debug\rebornctl.exe music apply --data-root $r
```

Expected: apply prints Building progress, ends with the new hash; `music status` shows slot 21 from the pack; pristine unchanged (compare hashes); `library/offline/NSR.iso` is a real file now. Then `music disable` → booted equals pristine again (no roster installed on the scratch root).

- [ ] **Step 3: Commit**

```bash
git add crates/reborn-core/src/soundtrack/apply.rs crates/reborn-cli/src/main.rs docs/RUNNING_REBORNCTL.md
git commit -m "soundtrack: revert to stock, switch, rebornctl music verbs"
```

---

### Task 9: EA-XA decoder and encoder (`ea_xa.rs`)

**Files:**
- Create: `crates/reborn-core/src/soundtrack/ea_xa.rs`

**Interfaces:**
- `pub const COEF1: [i32; 4] = [0, 240, 460, 392]; pub const COEF2: [i32; 4] = [0, 0, -208, -220];`
- `pub fn encode_adpcm_frame(samples: &[i16; 28], h1: i32, h2: i32) -> ([u8; 15], i32, i32)`
- `pub fn encode_pcm_frame(samples: &[i16; 28], h1: i32, h2: i32) -> ([u8; 61], i32, i32)`
- `pub fn encode_channel_blocks(samples: &[i16]) -> Vec<(Vec<u8>, u32)>`
- `pub fn encode_stereo_stream(left: &[i16], right: &[i16], pt: &[u8], target_size: Option<usize>) -> Vec<u8>`
- `pub struct Decoded { pub left: Vec<i16>, pub right: Vec<i16>, pub sample_rate: u32 }`
- `pub fn decode_stream(stream: &[u8]) -> Result<Decoded>` (sample rate: PT 0x84 if present else 24000; channels: PT 0x82, default 2; data of a block: bytes_per_ch of L then R; frames of 15 bytes, or 61 when byte 0 is 0xEE; stop each channel after the block's sample count).
- `pub fn decode_streams(concat: &[u8]) -> Result<Decoded>` decodes every stream in a blob in order, concatenating.

Port rules that decide byte-exactness (the Python is the reference):
- Python `round()` is half-to-even: use `f64::round_ties_even`.
- `int(round(ideal))` likewise; `ideal = 28 - log2(max(max_residual/7, 1))`, `center = clamp(round_ties_even(ideal), 8, 23)`, shifts tried `max(8, center-2) .. min(24, center+3)` exclusive upper.
- `nib = round_ties_even(target_val / (1 << (28 - shift)))` clamped to -8..7, stored `& 0xF`.
- `val = (nib_u << 28) as i32 >> shift` (arithmetic); `decoded = ((val + c1*h1 + c2*h2) >> 8).clamp(-32768, 32767)`; error sums in i64; strict `<` keeps the first best.
- `h1_est` in the residual pass is `clamp(s)`, not the decoded value.
- Block 0 loop: 58 frames, PCM for f < 3; after each frame `pos += 28`; break when `pos >= total && f >= 2`. Other blocks: 57 frames (+1 pad byte) except every 7th block (cycle_pos 6) has 58 and no pad; break when `pos >= total` before encoding a frame.
- `encode_stereo_stream` pads channels to equal block counts with `57 x silence + pad`, computes overhead `8 + pt.len() + 12 + 8`, and when `target_size` is given adds silent blocks exactly as the Python does, including the final partial block rule (`remaining >= 30`, `per_ch = remaining / 2`, `num_frames = per_ch / 15`, leftover zero bytes).
- Silence frame is `0x0C` followed by 14 zero bytes.

- [ ] **Step 1: Tests**

```rust
#[test] fn silence_encodes_to_the_silence_frame() { let (f, _, _) = encode_adpcm_frame(&[0; 28], 0, 0); assert_eq!(f[0] & 0xF0, 0); assert!(f[1..].iter().all(|&b| b == 0)); }
#[test] fn block_sizes_follow_the_cycle() { let s = vec![1000i16; 28 * 58 * 9]; let b = encode_channel_blocks(&s); assert_eq!(b[0].0.len(), 1008); assert_eq!(b[0].1, 1624); assert_eq!(b[1].0.len(), 856); assert_eq!(b[1].1, 1596); assert_eq!(b[7].0.len(), 870); assert_eq!(b[7].1, 1624); }
#[test] fn a_stream_decodes_back_close_to_its_input() { let n = 24000; let left: Vec<i16> = (0..n).map(|i| ((i as f64 * 0.05).sin() * 12000.0) as i16).collect(); let right: Vec<i16> = left.iter().map(|s| s / 2).collect(); let st = encode_stereo_stream(&left, &right, SYN_PT, None); let d = decode_stream(&st).unwrap(); assert_eq!(d.left.len() >= n, true); let err: f64 = left.iter().zip(&d.left).map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2)).sum::<f64>() / n as f64; assert!(err.sqrt() < 200.0, "rms error {}", err.sqrt()); }
#[test] fn padded_stream_hits_the_target_exactly() { let st = encode_stereo_stream(&[100; 3000], &[100; 3000], SYN_PT, Some(0x4000)); assert_eq!(st.len(), 0x4000); assert!(st.ends_with(b"SCEl\x08\x00\x00\x00")); }
```

- [ ] **Step 2: Implement, run, commit**

```bash
git add crates/reborn-core/src/soundtrack/ea_xa.rs
git commit -m "soundtrack: EA-XA v2 encoder and decoder ported from encode_ea_xa.py"
```

---

### Task 10: 40-segment slot encoder and oracle 3 (`segments.rs`)

**Files:**
- Create: `crates/reborn-core/src/soundtrack/segments.rs`
- Create: `Tools/oracle-encode.py`

**Interfaces:**
- `pub fn stock_pt_headers(pristine_asf: &[u8], slot: Slot) -> Result<Vec<Vec<u8>>>` (40 PT payloads).
- `pub fn encode_slot(left: &[i16], right: &[i16], pt_headers: &[Vec<u8>], on: &mut dyn FnMut(usize, usize)) -> Result<Vec<u8>>`: the extend_slot_40big recipe; returns the blob or `Refusal::SongTooLong` (seconds = total / 24000, limit derived as `floor(40 * seg_samples_max / 24000)` where seg_samples_max is found by trial: report 380 s as the limit constant `LIMIT_SECONDS: u64 = 380` for the message; the actual refusal comes from `target >= SEGMENT_CEILING`).
- `pub fn slot_limit_hint() -> u64 { 380 }`

- [ ] **Step 1: `Tools/oracle-encode.py`**

```python
# Run extend_slot_40big.py unchanged, but redirect its output ASF to a file we
# name, then cut the slot's 40-stream blob out of it.
#   py -3 Tools/oracle-encode.py <wav24k.wav> <slot> <base pathfind.dat> <out.asf-blob>
import os, sys
CLEAN = os.environ.get('NSR_CLEAN', r'C:\Users\Adam\nflstreet2_clean')
sys.path.insert(0, os.path.join(CLEAN, 'tools'))
import extend_song_slot as X
wav, slot, base, out = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
X.ASF = out + '.full.asf'
import extend_slot_40big as S
sys.argv = ['extend_slot_40big.py', wav, str(slot), base]
S.main()
import music_pack as P
d = open(X.ASF, 'rb').read()
first = P.slot_first(slot)
asf, pos = P._asf_pos(b'\x00' * P.ASF_OFF + d)
open(out, 'wb').write(bytes(asf[pos[first]:pos[first + 40]]))
print('blob', out, os.path.getsize(out))
```

- [ ] **Step 2: Oracle test**

Generate a 24 kHz stereo WAV in the test (60 s of two sine tones with a slow amplitude envelope, written with a minimal RIFF writer in `testing.rs`: `write_wav_24k(path, left, right)`). Run the script on it with the pristine pathfind, load the blob, and assert `encode_slot(...)` equals it byte for byte. Also assert `asf::validate_blob` accepts it.

- [ ] **Step 3: Implement `encode_slot`**

```rust
pub fn encode_slot(left: &[i16], right: &[i16], pt_headers: &[Vec<u8>], on: &mut dyn FnMut(usize, usize)) -> Result<Vec<u8>> {
    let total = left.len();
    let seg_samples = total.div_ceil(SEGMENTS_PER_SONG);
    let trial = ea_xa::encode_stereo_stream(&left[..seg_samples.min(total)], &right[..seg_samples.min(total)], &pt_headers[0], None);
    let raw = trial.len();
    let target = raw.div_ceil(0x800) * 0x800 + 0x800;
    if target >= SEGMENT_CEILING {
        return Err(Refusal::SongTooLong { seconds: (total / 24000) as u64, limit_seconds: slot_limit_hint() }.into());
    }
    let mut blob = Vec::with_capacity(target * SEGMENTS_PER_SONG);
    let silence = vec![0i16; seg_samples];
    for k in 0..SEGMENTS_PER_SONG {
        on(k, SEGMENTS_PER_SONG);
        let a = k * seg_samples;
        let b = ((k + 1) * seg_samples).min(total);
        let nsamp = b.saturating_sub(a);
        let pt = asf::patch_pt_tag(&pt_headers[k], 0x85, if nsamp > 0 { nsamp as u64 } else { 1 }).map_err(invalid)?;
        let mut st = if nsamp > 0 {
            ea_xa::encode_stereo_stream(&left[a..b], &right[a..b], &pt, Some(target))
        } else {
            ea_xa::encode_stereo_stream(&silence, &silence, &pt, Some(target))
        };
        if st.len() != target { return Err(invalid(format!("segment {k}: {} != target {target}", st.len()))); }
        let actual = asf::scdl_sample_sum(&st);
        if actual != nsamp as u64 {
            let schl_size = u32::from_le_bytes([st[4], st[5], st[6], st[7]]) as usize;
            let patched = asf::patch_pt_tag(&st[8..schl_size], 0x85, actual).map_err(invalid)?;
            st[8..schl_size].copy_from_slice(&patched);
        }
        blob.extend_from_slice(&st);
    }
    Ok(blob)
}
```

- [ ] **Step 4: Run the oracle**

Run: `$env:NSR_CLEAN=...; cargo test -p reborn-core --release oracle_encode_slot -- --nocapture`
Expected: identical blob. If it differs, find the first differing byte, map it to a segment and frame, and compare that frame's Python and Rust search (print both candidate lists). Do not "fix" by changing the Python.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/soundtrack/segments.rs Tools/oracle-encode.py crates/reborn-core/src/soundtrack/testing.rs
git commit -m "soundtrack: 40-segment slot encoder, byte-identical to extend_slot_40big.py"
```

---

### Task 11: File decode, resample, tags (`decode.rs`, `resample.rs`, `tags.rs`)

**Interfaces:**
- `resample::to_24k(channels: &[Vec<f32>], rate: u32) -> (Vec<i16>, Vec<i16>)`: mono duplicates, more than 2 channels averages into stereo; windowed sinc, 32 taps each side, Hann window, unity gain; identity when rate == 24000 (just convert with clamp and round half away from zero).
- `decode::decode_file(path: &Path) -> Result<decode::Audio { left: Vec<i16>, right: Vec<i16>, duration_ms: u64 }>` via symphonia probe; unsupported extension → `Refusal::UnsupportedFormat`; decode error → `Error::Io` with the file path and symphonia's message.
- `tags::Tags { title: Option<String>, artist: Option<String>, album: Option<String>, duration_ms: Option<u64> }`, `tags::read(path) -> Tags` (never errors; empty on failure), `tags::clean_query(file_stem: &str) -> String`.

`clean_query` rules, tested one per line: strip a leading track number (`01`, `01.`, `01 -`, `1-`); strip `[...]` and `(...)` groups; drop the words official, video, audio, lyrics, lyric, hd, hq, remaster, remastered, 4k, feat. is kept; replace `_` and multiple spaces with one space; keep ` - ` as a space; trim. `"03. Alicia Keys - Unthinkable (Official Video) [HD]"` → `"Alicia Keys Unthinkable"`.

- [ ] **Step 1: Tests**: WAV written by `write_wav_24k` decodes to the same samples; a 48 kHz WAV decodes to half the sample count (±1); `clean_query` table; `tags::read` on a WAV returns duration and no title; env-gated: if `%USERPROFILE%\Music` has any `.mp3`, decode it and assert duration > 0 and tags read without panic.

- [ ] **Step 2: Implement.** symphonia: `get_probe().format(&hint, MediaSourceStream::new(Box::new(File)), &FormatOptions::default(), &MetadataOptions::default())`, pick the first audio track, `make_decoder`, loop `next_packet`, `decode`, `SampleBuffer<f32>` copy interleaved, split channels. Use `f32` accumulation, then resample.

- [ ] **Step 3: Commit**

```bash
git add crates/reborn-core/src/soundtrack
git commit -m "soundtrack: file decode, resample to 24 kHz, embedded tags and file-name cleaning"
```

---

### Task 12: Online lookup (`lookup.rs`)

**Interfaces:**
- `pub struct Candidate { pub title: String, pub artist: String, pub album: String, pub duration_ms: Option<u64>, pub source: &'static str /* "iTunes" | "MusicBrainz" */ }`
- `pub fn itunes_url(query: &str) -> String` = `https://itunes.apple.com/search?media=music&entity=song&limit=8&term=<percent-encoded>`
- `pub fn musicbrainz_url(query: &str) -> String` = `https://musicbrainz.org/ws/2/recording?fmt=json&limit=8&query=<percent-encoded>`
- `pub fn parse_itunes(json: &str) -> Vec<Candidate>` (results[].trackName, artistName, collectionName, trackTimeMillis)
- `pub fn parse_musicbrainz(json: &str) -> Vec<Candidate>` (recordings[].title, artist-credit[0].name, releases[0].title, length)
- `pub fn lookup(http: &dyn Http, query: &str) -> Vec<Candidate>`: iTunes; if empty or failed, MusicBrainz; never errors, empty on failure. Percent-encode with a local function (no new crate): unreserved bytes pass, space → `%20`, everything else `%XX`.

- [ ] **Step 1: Tests with `net::tests::FakeHttp`**: iTunes JSON fixture yields candidates in order; empty results fall through to MusicBrainz; both failing yields empty; URL encoding of `"Alicia Keys Unthinkable (I'm Ready)"`.

- [ ] **Step 2: Implement, commit**

```bash
git add crates/reborn-core/src/soundtrack/lookup.rs
git commit -m "soundtrack: iTunes and MusicBrainz song lookup through the Http trait"
```

---

### Task 13: Staging a file (`apply.rs` part 2) and CLI `add`, `inspect`, `lookup`

**Interfaces:**
- `pub struct Inspection { pub tags: tags::Tags, pub query: String, pub duration_ms: u64, pub candidates: Vec<lookup::Candidate>, pub too_long: bool }`
- `pub fn inspect(http: &dyn Http, path: &Path) -> Result<Inspection>`: tags; if title or artist missing, `lookup(clean_query(stem))`; duration from decode (decoding once here is acceptable: it is the same decode `stage_file` repeats; to avoid double work cache the decoded PCM in `music/encoded/<track>.pcm`? No: decode twice, it is seconds. YAGNI.)
- `pub fn stage_file(data_root: &Path, pristine_pathfind: &[u8], path: &Path, slot: Slot, title: &str, artist: &str, album: &str, on: &mut dyn FnMut(usize, usize)) -> Result<state::Entry>`: hash file → track id; copy to `music/sources/<id>.<ext>` if absent; decode; `stock_pt_headers`; `encode_slot`; write `music/encoded/<id>-s<slot>.asf` (`.tmp` then rename); stage `Source::UserFile`.
- CLI: `music inspect <file>` prints tags, cleaned query, candidates; `music lookup <query>`; `music add <file> --slot N [--title --artist --album]` (defaults from inspection: tags, else first candidate, else file stem; prints what it chose).

- [ ] **Step 1: Test** `stage_file` on the synthetic pathfind with a 3 s WAV: encoded blob exists and validates, `soundtrack.json` pending has the slot with the given text, source copied; staging the same file twice does not copy again.

- [ ] **Step 2: Implement, run, commit**

```bash
git add crates/reborn-core/src/soundtrack/apply.rs crates/reborn-cli/src/main.rs
git commit -m "soundtrack: stage a song from a file with tags and lookup; rebornctl music add"
```

---

### Task 14: NFL Street 1 pack (`street1.rs`) and the two packages

**Files:**
- Create: `crates/reborn-core/src/soundtrack/street1.rs`
- Modify: `crates/reborn-cli/src/main.rs` (`MusicCommand::PackFromStreet1 { disc_files: PathBuf, pristine_pathfind: PathBuf, out: PathBuf }`, `MusicCommand::SoundtrackPackages { pack: PathBuf, out_dir: PathBuf, version: String }`)

**Interfaces:**
- `pub fn read_street1(files_dir: &Path) -> Result<Vec<Street1Song { stream: Vec<u8>, title: String, artist: String, album: String }>>`: `femusic.dat` split by `asf::scan_schl` and `stream_len` (14 streams), titles from `eatx`-like read of `template.dat` (same field tags; write `eatx::locate_any(template, count)` that accepts a record count parameter, and have `locate` call it with 21).
- `pub fn build_pack(files_dir, pristine_pathfind: &[u8], out: &Path, on) -> Result<pack::Manifest>`: for each song k in 0..14: `ea_xa::decode_stream`, `resample::to_24k`, `segments::encode_slot` with slot k+1 PT headers, `pack::write_pack` name "NFL Street Soundtrack", author "NFL Street Reborn", description "The 14 songs from the first NFL Street, in slots 1 to 14."
- `pub fn write_packages(pack: &Path, out_dir: &Path, version: &str) -> Result<()>`: writes
  - `out_dir/reborn.soundtrack.original/<version>/package.json` + `files/pack.json` (`{"magic":"NSRMUSIC","format":2,"id":"original","name":"NFL Street 2 Original Soundtrack","songs":{}}`)
  - `out_dir/reborn.soundtrack.nfl-street-1/<version>/package.json` + `files/nfl-street-1.nsrmusic`
  - package.json fields: `schema nsr.mod.v1`, `magic NSMD`, `id`, `name` ("NFL Street 2 Original Soundtrack" / "NFL Street 1 Soundtrack"), `version`, `targets: []`, `conflict_group: "soundtrack"`, `kind: "soundtrack"`, `category: "music"`, `description`, `files: [{path, sha256, bytes}]`.
  Confirm `ModPackage::load` accepts `targets: []` and the extra fields (it ignores unknown fields; check `GameId`-typed `targets` deserializes an empty list) with a unit test that loads the written manifest.

- [ ] **Step 1: Test** `read_street1` against the extracted files (env-gated on `NSR_STREET1` = the extracted `files` dir): 14 songs, first title "Quick To Back Down", first artist contains "Bravehearts". Decode stream 0: sample_rate 32000, left.len() == PT 0x85.

- [ ] **Step 2: Build the real pack**

```powershell
$dt = "$env:LOCALAPPDATA\Programs\NFL Street Reborn Launcher Beta\runtime\DolphinTool.exe"
& $dt extract -i "$env:APPDATA\NFL Street Reborn\Games\NFL Street (USA).ciso" -o target\street1
$env:NSR_STREET1 = "target\street1\files"
cargo run --release -p reborn-cli -- music pack-from-street1 --disc-files target\street1\files --pristine-pathfind "C:\Users\Adam\nflstreet2_clean\GameCube Version\root\pathfind.dat.bak_good_2026-07-02" --out target\nfl-street-1.nsrmusic
cargo run --release -p reborn-cli -- music soundtrack-packages --pack target\nfl-street-1.nsrmusic --out-dir target\soundtrack-packages --version 1.0.0
```

Note the pristine pathfind for PT headers must be the STOCK one (`.bak_good_2026-07-02` or the pristine FIELDS disc's pathfind.dat read with `gcm`); root/pathfind.dat may carry custom slots with patched PT counts, which the encoder overwrites anyway, but stock is the honest base. Prefer reading it from the FIELDS disc via `Disc::read_file`.

Expected: 14 songs, every song under the ceiling (print each length), pack about 80 MB. Any song over 6:20 is reported and skipped, not truncated; say so in the summary.

- [ ] **Step 3: Commit** (code only; the pack and packages are build outputs under target/)

```bash
git add crates/reborn-core/src/soundtrack/street1.rs crates/reborn-core/src/soundtrack/eatx.rs crates/reborn-cli/src/main.rs
git commit -m "soundtrack: NFL Street 1 pack builder and the two soundtrack packages"
```

---

### Task 15: Replay after a roster or game update; `write-tree`

**Files:**
- Modify: `crates/reborn-core/src/content/pipeline.rs`
- Modify: `crates/reborn-core/src/soundtrack/apply.rs` (`replay`)

**Interfaces:**
- `apply::replay(platform, data_root, on) -> Result<Option<Outcome>>`: `None` when the switch is off or every effective slot is stock; otherwise `apply` (skipping slots whose blob or pack is missing, collecting them into `Outcome::Applied.skipped: Vec<(u8, String)>`; add that field to `Applied`).
- `Updater::apply_current_soundtrack(&self, hook) -> Result<Option<SoundtrackResult>>` where `pub enum SoundtrackResult { Replayed { slots: Vec<u8>, skipped: Vec<(u8, String)> }, Refused(String) }`.
- In `promote`, after `apply_current_roster` returns (both the Roster and GamePatch arms), call `apply_current_soundtrack` and carry the result in `PromoteState::Applied { soundtrack: Option<SoundtrackResult> }`. Update the UI's `InstalledSummary` to append "Custom soundtrack reapplied" or the skipped list to `problems`.

- [ ] **Step 1: Test** in pipeline tests: a data root with a staged and applied synthetic soundtrack, then a fake roster promotion → the booted image carries the soundtrack again (use the synthetic disc; the roster Builder is faked already in pipeline tests, follow that fake).

- [ ] **Step 2: Implement `write-tree`** (Task 8 declared it; implement now if not done): reads `<tree>/pathfind.dat` and `<tree>/template.dat`, uses `build_files` with the tree's files as both pristine and booted, writes back.

- [ ] **Step 3: Commit**

```bash
git add crates/reborn-core/src/content/pipeline.rs crates/reborn-core/src/soundtrack/apply.rs crates/reborn-ui/src/content_updates.rs crates/reborn-cli/src/main.rs
git commit -m "soundtrack: replay after roster and game updates; write-tree for the dev root"
```

---

### Task 16: Launcher commands (`music_page.rs`)

**Files:**
- Create: `crates/reborn-ui/src/music_page.rs`
- Modify: `crates/reborn-ui/src/main.rs` (`mod music_page;`, register commands in `generate_handler!`)

**Interfaces (all `#[tauri::command]`, all `spawn_blocking`):**
- `music_status() -> MusicStatusView { phase: &'static str /* idle, encoding, building, done, problem */, progress: Option<ProgressView { label: String, done: u64, total: u64 }>, enabled: bool, base: BaseView { kind: String, name: Option<String> }, slots: Vec<SlotView { slot: u8, title: String, artist: String, album: String, duration_ms: Option<u64>, source: String /* original, your_file, pack, soundtrack_mod */, source_name: Option<String>, pending: Option<PendingView { kind: String, title, artist, album }> }>, note: String, image_ok: bool }`
- `music_inspect(path: String) -> InspectView { title, artist, album (Option<String>), duration_ms, too_long: bool, query: String, candidates: Vec<CandidateView>, note: String }`
- `music_lookup(query: String) -> Vec<CandidateView>`
- `music_stage_file(path, slot, title, artist, album) -> MusicStatusView` (encodes on a background thread; phase `encoding` with per-segment progress; returns immediately with the status, page polls)
- `music_stage_stock(slot)`, `music_stage_details(slot, title, artist, album)`, `music_unstage(slot)`, `music_pack_preview(path) -> PackPreviewView { name, author, songs: Vec<{slot, title, artist, replaces: String}> }`, `music_stage_pack(path)`, `music_export(path, slots, name, author, description) -> String`
- `music_set_enabled(on: bool) -> MusicStatusView` and `music_apply(confirm: bool) -> MusicStatusView`: acquire `LaunchGuard` (refusal text when active), run `apply_or_revert` on a background thread with progress into the static state, page polls `music_status`.
- The `Http` for lookups: `reborn_core::net::UreqHttp::new(<launcher user agent used elsewhere>)`; find the constant `update.rs` uses.

- [ ] **Step 1: Write the module** mirroring `content_updates.rs`: `static HELD: Mutex<Held>`, `Phase`, views with `Serialize`, background thread via `std::thread::spawn` that updates `HELD` and drops the guard at the end. Add a `#[cfg(test)]` test that `MusicStatusView` serializes with every field present (the checkers' NULLS answer relies on the field names).

- [ ] **Step 2: Register** in `generate_handler!` next to the content commands, and add every command's argument names to `Tools/check-harness-args.mjs`'s expectations if the harness lists commands (read how `content_check` was added and do the same).

- [ ] **Step 3: `cargo build -p reborn-ui`, `node Tools/check-harness-args.mjs`, commit**

```bash
git add crates/reborn-ui/src/music_page.rs crates/reborn-ui/src/main.rs Tools/check-harness-args.mjs
git commit -m "launcher: music commands (status, inspect, lookup, stage, pack, export, apply)"
```

---

### Task 17: Music page, nav, checker

**Files:**
- Create: `crates/reborn-ui/ui/music.html`, `crates/reborn-ui/ui/music.js`
- Modify: nav in `index.html`, `mods.html`, `controls.html`, `online.html`, `settings.html`, `recovery.html`, `onboarding.html` (add a Music link with a note icon between Mods and Controllers; copy the exact `<a>` markup pattern including the inline SVG)
- Create: `Tools/check-music-page.mjs`
- Modify: `Tools/check-shell-pages.mjs` (add `music.html` and `music_status` FULL/NULLS answers), `Tools/test-all.ps1` (run the new checker)

**Page structure (`music.js`, built from the store's `el(...)` helper and classes `store-grid`, `store-card`, `store-art`, `store-card-body`, `store-card-actions`, `section-tabs`, `mod-dialog`, `note`, `primary`):**
- Header block: switch "Custom soundtrack" (button pair On / Off), one status sentence from `note`, the base ("Original songs" / "NFL Street 1 Soundtrack mod").
- Apply bar (sticky top of main when `pending` is non-empty): "N changes staged" + `Apply changes` (primary) + `Discard` (unstages all). While `phase` is encoding/building, replace with a progress line and disable actions; poll `music_status` every 1000 ms until phase is idle/done/problem.
- Grid of 21 cards: art color block, `strong` title, `small` "artist · m:ss · source". Pending state shows a "Staged: …" line. Actions: `Replace` (opens Add dialog for that slot), `Reset` (stage stock; hidden when already original with no override), `Details` (opens the details dialog).
- Add dialog: file picker (`window.__TAURI__.dialog.open` if available; else a text input for a path, which is what the harness uses), on pick → `music_inspect` → fields Title / Artist / Album pre-filled, a line "Found online from the file name" or "From the file's tags" or "Type the song details", a "Not right? Search" input + list of candidates (click fills), length line, too-long refusal text inline, `Add to slot N` button → `music_stage_file`.
- Import dialog: file picker → `music_pack_preview` → table slot / now / becomes → `Stage this pack`.
- Export dialog: checkboxes for the 21 slots (pre-checked for non-original), name / author / description, `Save pack` → `music_export`.
- Every command call goes through the page's `act(cmd, args)` that catches and shows the error text inline in the status line. No alerts.

- [ ] **Step 1: Write `Tools/check-music-page.mjs`** modeled on `check-mods-page.mjs`: load music.js with FULL / NULLS / rejecting cores; assert: 21 cards drawn from FULL, the pending card shows "Staged", the switch reflects `enabled`, a rejecting core leaves a sentence in the status and throws nothing, NULLS draws without throwing.

- [ ] **Step 2: Write the page and JS**, run `node Tools/check-music-page.mjs`, `node Tools/check-shell-pages.mjs`.

- [ ] **Step 3: Real window**: `cargo build --workspace`, kill any running `reborn-launcher`, run the launcher from `target/debug` with `REBORN_DATA_ROOT`-style override if one exists (else the beta channel's data root), open Music, add a real MP3 to slot 21, watch encoding progress, Apply, watch building, confirm the status hash. Then Off, then On.

- [ ] **Step 4: Commit**

```bash
git add crates/reborn-ui/ui Tools/check-music-page.mjs Tools/check-shell-pages.mjs Tools/test-all.ps1
git commit -m "launcher: Music page, nav entry, page checker"
```

---

### Task 18: Soundtrack packages in the Mods page

**Files:**
- Modify: `crates/reborn-ui/src/main.rs` (`mods_toggle`), `crates/reborn-ui/ui/mods.js`

- [ ] **Step 1: In `mods_toggle`**, after `activation.save`, read `<dir>/package.json` as `serde_json::Value`; if `kind == "soundtrack"`: when `on`, set `Soundtrack.base` to `Base::Package { id, version, pack: "mods/packages/<id>/<version>/files/<file>" }` (or `Base::Stock` when the manifest's pack has no songs), when `off` set `Base::Stock`; save; then run `music_page::rebuild_now()` (a function in music_page.rs that acquires the guard and runs `apply_or_revert` synchronously, returning the note) and put its note into `ModSyncView.note` (add the field if absent). Skip `sync_active_now` for soundtrack packages (nothing to compose) but still return a `ModSyncView`.

- [ ] **Step 2: In `mods.js`**, when the toggled package's `kind` is soundtrack (expose `kind` in `mod_packages` view), show "Rebuilding your game…" on the card and disable the grid until the call returns; then re-read `mod_packages`. The Soundtrack tab's empty state text: "Soundtrack mods appear here. Your own songs live on the Music page."

- [ ] **Step 3: `node Tools/check-mods-page.mjs`**, install the two packages into the beta data root's `mods/packages`, toggle NFL Street 1 in the real window, confirm the disc rebuilds and the Music page shows 14 songs from the mod.

- [ ] **Step 4: Commit**

```bash
git add crates/reborn-ui/src/main.rs crates/reborn-ui/ui/mods.js
git commit -m "launcher: soundtrack packages rebuild the disc from the Mods page"
```

---

### Task 19: Retire the old model and docs

**Files:**
- Modify: `crates/reborn-core/src/music.rs` → keep only `music_root`, `sources_dir`, `packs_dir`, `soundtrack_path` (now pointing at `soundtrack.json` via `soundtrack::state`), delete `STREAM_SLOTS`, `TrackOrder`, `MusicPack`, `SoundtrackState`, `write_soundtrack`, `read_pack` and their tests; fix the CLI `music status` (now `MusicCommand::Status`) and `verify`'s state path.
- Modify: `docs/architecture/CONTENT_AND_UPDATE_MODEL.md` section 15: one paragraph noting the shipped design uses no work tree, and `music/soundtrack.json` is the record.
- Modify: `docs/RUNNING_REBORNCTL.md` with the music verbs.
- Modify: `C:\Users\Adam\nflstreet2_clean\tools\music_editor.py`: replace the body with a 10-line stub that prints "Retired 2026-09-12. Use the launcher's Music page, or rebornctl music write-tree --tree <root>." and exits 1. Commit in that repo separately if it is a git repo; otherwise just edit.

- [ ] **Step 1: `cargo test --workspace`, `cargo clippy --workspace --all-targets`**: green.

- [ ] **Step 2: Commit**

```bash
git add crates/reborn-core/src/music.rs crates/reborn-cli/src/main.rs docs
git commit -m "soundtrack: retire the 40-slot model; docs for the music verbs"
```

---

### Task 20: Full verification, boot test, ship

- [ ] **Step 1:** `Tools/test-all.ps1 -Browser` in the worktree. All green or each failure named.
- [ ] **Step 2:** Adam's boot test: from the real launcher, add one song to slot 21 with lookup-filled details, Apply, boot, jukebox slot 21: banner shows the text, song plays to the end, next song advances. Then enable NFL Street 1 from Mods, boot, jukebox slot 1: "Quick To Back Down" plays. Record both in the memory note.
- [ ] **Step 3:** Merge `music` into main (fast-forward or merge commit after `git status` shows the other agent's work committed), `cargo build --release --workspace`.
- [ ] **Step 4:** Publish: `rebornctl publish component mods --from target\soundtrack-packages ...` per `docs/RUNNING_UPDATES.md`, release sequence 4, upload with the rclone script (blobs first, release.json + sig last). Verify over HTTPS with `rebornctl update check`.
- [ ] **Step 5:** `installer\windows\build.ps1 -LauncherOnly -Version 3.1.0`, `Tools/verify-launcher-update.ps1`, upload zip + Update exe + latest.json (last). `rebornctl update check` reports 3.1.0.
- [ ] **Step 6:** Update the memory note `project_reborn_music_editor_design.md` with what shipped and what was cut.
