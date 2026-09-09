//! Staging, the apply pipeline, export.
//!
//! Apply reads the booted disc's FST, builds a new pathfind.dat (the
//! pristine ASF with the effective slots spliced in, its table rewritten)
//! and a new template.dat (the booted one, so the roster survives, with the
//! 21 banner records written), streams a new ISO with a regenerated FST,
//! records its hash, and renames it over the booted image. One rebuild per
//! apply, never one per song. Same order as `crate::roster::apply`: record
//! first, swap second, undo the record if the swap fails.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::content::pipeline::{ImageHook, RosterResult, Updater};
use crate::game_image::{self, EstablishOutcome, EstablishRefusal, IMAGE_FILE};
use crate::library::{hash_file, RebuildState};
use crate::manifest::Sha256Hex;
use crate::platform::{DataDir, Error, Platform, Result};
use crate::soundtrack::state::{self, BlobRef, Effective, Entry, Soundtrack, Source};
use crate::soundtrack::{asf, decode, eatx, gcm, lookup, pack, pathfind, segments, tags, Refusal, Slot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Reading,
    Splicing { slot: u8 },
    Building { done: u64, total: u64 },
    Hashing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub image: PathBuf,
    pub before: Option<Sha256Hex>,
    pub after: Sha256Hex,
    pub slots_changed: Vec<u8>,
    /// Slots left as the disc's own song because their audio was gone.
    pub skipped: Vec<(u8, String)>,
    /// What the roster re-apply said on a revert, when it did not simply
    /// succeed. `None` after an apply.
    #[allow(clippy::type_complexity)]
    pub roster_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Applied(Applied),
    Unchanged { image: PathBuf, sha256: Sha256Hex },
    Refused(Refusal),
}

impl Outcome {
    /// One sentence for a page or a console.
    pub fn note(&self) -> String {
        match self {
            Outcome::Applied(a) if a.slots_changed.is_empty() => match &a.roster_note {
                Some(n) => format!("Your game now plays the original soundtrack. {n}"),
                None => "Your game now plays the original soundtrack.".to_string(),
            },
            Outcome::Applied(a) => {
                let mut s = format!(
                    "Your soundtrack is on the disc: {} custom {}.",
                    a.slots_changed.len(),
                    if a.slots_changed.len() == 1 { "song" } else { "songs" }
                );
                if !a.skipped.is_empty() {
                    s.push_str(&format!(" {} slot(s) went back to the original song because their audio was missing.", a.skipped.len()));
                }
                s
            }
            Outcome::Unchanged { .. } => "The disc already had this soundtrack. Nothing changed.".to_string(),
            Outcome::Refused(r) => r.message(),
        }
    }
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.to_path_buf(), source }
}

/// The pristine image (never written) and the booted image (what Dolphin
/// opens). Both must exist for an apply.
pub fn images(data_root: &Path) -> std::result::Result<(PathBuf, PathBuf), Refusal> {
    let pristine = game_image::find_pristine(data_root).ok_or(Refusal::NoPristine)?;
    let booted = game_image::booted_path(data_root);
    if !booted.is_file() {
        return Err(Refusal::NoBooted);
    }
    Ok((pristine.path, booted))
}

/// Build the two files an apply changes. Pure apart from reading the blobs
/// named in `effective`. Stock slots take the pristine blob (already in the
/// pristine ASF) and the pristine banner record. A blob that cannot be read
/// is reported in the third return value and its slot stays stock.
pub fn build_files(
    pristine_pathfind: &[u8],
    pristine_template: &[u8],
    booted_template: &[u8],
    effective: &BTreeMap<Slot, Effective>,
    on: &mut dyn FnMut(Progress),
) -> Result<BuiltFiles> {
    on(Progress::Reading);
    let stock_records = eatx::read_records(pristine_template)?;
    let mut asf_bytes = pathfind::asf(pristine_pathfind).to_vec();
    if asf_bytes.is_empty() {
        pathfind::Pathfind::parse(pristine_pathfind)?;
    }
    let mut template = booted_template.to_vec();
    let mut changed = Vec::new();
    let mut skipped = Vec::new();
    for (slot, eff) in effective {
        match eff {
            Effective::Stock => {
                eatx::write_record(&mut template, *slot, &stock_records[slot.index()])?;
            }
            Effective::Blob { blob, title, artist, album, .. } => {
                on(Progress::Splicing { slot: slot.get() });
                let bytes = match read_blob(blob) {
                    Ok(b) => b,
                    Err(reason) => {
                        skipped.push((slot.get(), reason));
                        eatx::write_record(&mut template, *slot, &stock_records[slot.index()])?;
                        continue;
                    }
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
    Ok(BuiltFiles { pathfind: new_pf, template, changed, skipped })
}

#[derive(Debug, Clone)]
pub struct BuiltFiles {
    pub pathfind: Vec<u8>,
    pub template: Vec<u8>,
    pub changed: Vec<u8>,
    pub skipped: Vec<(u8, String)>,
}

fn read_blob(blob: &BlobRef) -> std::result::Result<Vec<u8>, String> {
    match blob {
        BlobRef::Encoded(p) => {
            let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
            asf::validate_blob(&bytes).map_err(|r| format!("{}: {r}", p.display()))?;
            Ok(bytes)
        }
        BlobRef::Pack { pack: path, slot } => {
            let pack = pack::read_pack(path).map_err(|e| e.to_string())?;
            pack::read_blob(&pack, *slot).map_err(|e| e.to_string())
        }
    }
}

/// Apply the staged soundtrack to the booted image.
pub fn apply(platform: &Platform, data_root: &Path, on: &mut dyn FnMut(Progress)) -> Result<Outcome> {
    apply_with_size(platform, data_root, None, on)
}

/// `disc_size` overrides the full-disc size for the toy discs the tests build.
pub fn apply_with_size(
    platform: &Platform,
    data_root: &Path,
    disc_size: Option<u64>,
    on: &mut dyn FnMut(Progress),
) -> Result<Outcome> {
    let mut st = Soundtrack::load(data_root)?;
    if !st.enabled {
        return Ok(Outcome::Refused(Refusal::Disabled));
    }
    let base_pack = st.base_pack(data_root)?;
    let effective = st.effective(data_root, base_pack.as_ref(), true);
    if Soundtrack::is_all_stock(&effective) {
        return Ok(Outcome::Refused(Refusal::NothingStaged));
    }
    let (pristine_path, booted) = match images(data_root) {
        Ok(p) => p,
        Err(r) => return Ok(Outcome::Refused(r)),
    };
    if platform.files.is_locked(&booted) {
        return Ok(Outcome::Refused(Refusal::ImageInUse { path: booted }));
    }

    let pristine = gcm::Disc::open(&pristine_path)?;
    let source = gcm::Disc::open(&booted)?;
    let pristine_pathfind = pristine.read_file("pathfind.dat")?;
    let pristine_template = pristine.read_file("template.dat")?;
    let booted_template = source.read_file("template.dat")?;
    let built = build_files(&pristine_pathfind, &pristine_template, &booted_template, &effective, on)?;

    let offline_dir = booted.parent().map(Path::to_path_buf).unwrap_or_else(|| data_root.join(DataDir::LibraryOffline.relative()));
    let staging = data_root.join(DataDir::Staging.relative()).join("music");
    std::fs::create_dir_all(&staging).map_err(|e| io(&staging, e))?;
    if !platform.files.same_volume(&staging, &offline_dir)? {
        return Err(Error::PlainIo(std::io::Error::other(format!(
            "{} and {} are on different volumes, so the rebuilt disc cannot be swapped in atomically",
            staging.display(),
            offline_dir.display()
        ))));
    }
    let part = staging.join(format!("{IMAGE_FILE}.part"));
    if part.exists() {
        std::fs::remove_file(&part).map_err(|e| io(&part, e))?;
    }
    let mut replacements = BTreeMap::new();
    replacements.insert("pathfind.dat".to_string(), built.pathfind.clone());
    replacements.insert("template.dat".to_string(), built.template.clone());
    let size = disc_size.unwrap_or(gcm::DISC_SIZE);
    let after = gcm::repack_with_size(&source, &replacements, &part, size, &mut |done, total| on(Progress::Building { done, total }))?;

    // Read-back gate: the new disc must open and carry the files just built.
    on(Progress::Hashing);
    let check = gcm::Disc::open(&part)?;
    let pf_len = check.file("pathfind.dat").map(|f| f.size).unwrap_or(0);
    if pf_len != built.pathfind.len() as u64 || std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0) != size {
        let _ = std::fs::remove_file(&part);
        return Err(Error::PlainIo(std::io::Error::other("the rebuilt disc did not read back with the files just written")));
    }
    let before = hash_file(&booted)?;
    if before == after {
        let _ = std::fs::remove_file(&part);
        st.commit_pending();
        st.save(data_root)?;
        return Ok(Outcome::Unchanged { image: booted, sha256: after });
    }

    // Record first, swap second; undo the record if the swap fails.
    let state_path = state::rebuild_state_path(data_root);
    let mut rebuild = RebuildState::load(&state_path)?;
    let previous = rebuild.rebuilt.get(IMAGE_FILE).cloned();
    rebuild.rebuilt.insert(IMAGE_FILE.to_string(), after.clone());
    rebuild.save(&state_path)?;
    if let Err(e) = platform.files.atomic_replace(&part, &booted) {
        match previous {
            Some(p) => {
                rebuild.rebuilt.insert(IMAGE_FILE.to_string(), p);
            }
            None => {
                rebuild.rebuilt.remove(IMAGE_FILE);
            }
        }
        if let Err(undo) = rebuild.save(&state_path) {
            tracing::warn!("could not undo the rebuild record after a failed swap: {undo}");
        }
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }

    st.commit_pending();
    st.base_sha256 = game_image::find_pristine(data_root).map(|p| p.folder_sha256);
    st.rebuilt_sha256 = Some(after.as_str().to_string());
    st.save(data_root)?;
    Ok(Outcome::Applied(Applied { image: booted, before: Some(before), after, slots_changed: built.changed, skipped: built.skipped, roster_note: None }))
}

// ---------------------------------------------------------------------------
// The disc's own songs
// ---------------------------------------------------------------------------

/// What a slot plays when nothing replaces it: the pristine image's banner
/// text and the length its position table records.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StockSong {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
}

/// The 21 stock songs of a pristine image, in slot order. Reads the image's
/// template.dat and pathfind.dat once; callers cache by the image.
pub fn stock_songs(pristine: &Path) -> Result<Vec<StockSong>> {
    let disc = gcm::Disc::open(pristine)?;
    let template = disc.read_file("template.dat")?;
    let pf = disc.read_file("pathfind.dat")?;
    let records = eatx::read_records(&template)?;
    let mut out = Vec::with_capacity(records.len());
    for (record, slot) in records.iter().zip(Slot::all()) {
        out.push(StockSong {
            title: record.title.clone(),
            artist: record.artist.clone(),
            album: record.album.clone(),
            duration_ms: pathfind::slot_duration_ms(&pf, slot).unwrap_or(0),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Staging
// ---------------------------------------------------------------------------

/// Copy a pack into `music/packs` (never overwriting a different file of the
/// same name) and stage every song it carries. Returns the slots staged.
pub fn stage_pack(data_root: &Path, path: &Path) -> Result<Vec<u8>> {
    let pack = pack::read_pack(path)?;
    let dir = state::packs_dir(data_root);
    std::fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "pack.nsrmusic".into());
    let src_hash = hash_file(path)?;
    let mut dest = dir.join(&file_name);
    let mut n = 2;
    while dest.exists() && hash_file(&dest)? != src_hash {
        let stem = Path::new(&file_name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = Path::new(&file_name).extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        dest = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    if !dest.exists() {
        std::fs::copy(path, &dest).map_err(|e| io(&dest, e))?;
    }
    let rel = format!("music/packs/{}", dest.file_name().unwrap().to_string_lossy());
    let mut st = Soundtrack::load(data_root)?;
    let mut staged = Vec::new();
    for (n, song) in &pack.manifest.songs {
        let slot = Slot::new(*n)?;
        st.stage(
            slot,
            Entry {
                source: Source::PackStream { pack: rel.clone(), slot: *n },
                title: song.title.clone(),
                artist: song.artist.clone(),
                album: song.album.clone(),
                duration_ms: song.duration_ms,
            },
        );
        staged.push(*n);
    }
    st.save(data_root)?;
    Ok(staged)
}

pub fn stage_stock(data_root: &Path, slot: Slot) -> Result<()> {
    let mut st = Soundtrack::load(data_root)?;
    st.stage(slot, Entry::stock());
    st.save(data_root)
}

/// Stage new banner text. A slot with nothing behind it stages the disc's
/// own audio with the new text.
pub fn stage_details(data_root: &Path, slot: Slot, title: Option<&str>, artist: Option<&str>, album: Option<&str>) -> Result<()> {
    let mut st = Soundtrack::load(data_root)?;
    let n = slot.get();
    let mut entry = st.pending.get(&n).or_else(|| st.slots.get(&n)).cloned().unwrap_or_else(Entry::stock);
    if let Some(t) = title {
        entry.title = t.to_string();
    }
    if let Some(a) = artist {
        entry.artist = a.to_string();
    }
    if let Some(l) = album {
        entry.album = l.to_string();
    }
    for (field, text) in [("title", &entry.title), ("artist", &entry.artist), ("album", &entry.album)] {
        if text.chars().count() > eatx::MAX_TEXT {
            return Err(Refusal::TooLongText { field, len: text.chars().count() }.into());
        }
    }
    st.stage(slot, entry);
    st.save(data_root)
}

pub fn unstage(data_root: &Path, slot: Slot) -> Result<bool> {
    let mut st = Soundtrack::load(data_root)?;
    let was = st.unstage(slot).is_some();
    st.save(data_root)?;
    Ok(was)
}

pub fn unstage_all(data_root: &Path) -> Result<usize> {
    let mut st = Soundtrack::load(data_root)?;
    let n = st.pending.len();
    st.pending.clear();
    st.save(data_root)?;
    Ok(n)
}

pub fn set_enabled(data_root: &Path, on: bool) -> Result<()> {
    let mut st = Soundtrack::load(data_root)?;
    st.enabled = on;
    st.save(data_root)
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Write a pack from the slots as the disc has them (applied, not pending).
/// Stock slots are exported from the pristine image with their own text.
#[derive(Debug, Clone, Copy)]
pub struct ExportRequest<'a> {
    pub data_root: &'a Path,
    pub pristine_pathfind: &'a [u8],
    pub pristine_template: &'a [u8],
    pub slots: &'a [Slot],
    pub out: &'a Path,
    pub name: &'a str,
    pub author: &'a str,
    pub description: &'a str,
}

pub fn export(req: &ExportRequest<'_>) -> Result<pack::Manifest> {
    let ExportRequest { data_root, pristine_pathfind, pristine_template, slots, out, name, author, description } = *req;
    let st = Soundtrack::load(data_root)?;
    let base_pack = st.base_pack(data_root)?;
    let effective = st.effective(data_root, base_pack.as_ref(), false);
    let stock_records = eatx::read_records(pristine_template)?;
    let stock_asf = pathfind::asf(pristine_pathfind);
    let mut songs = Vec::with_capacity(slots.len());
    for slot in slots {
        let (blob, title, artist, album) = match &effective[slot] {
            Effective::Stock => {
                let r = &stock_records[slot.index()];
                (asf::slot_blob(stock_asf, *slot)?, r.title.clone(), r.artist.clone(), r.album.clone())
            }
            Effective::Blob { blob, title, artist, album, .. } => {
                let bytes = read_blob(blob).map_err(|what| Refusal::SourceMissing { slot: slot.get(), what })?;
                (bytes, title.clone(), artist.clone(), album.clone())
            }
        };
        songs.push(pack::SongInput { slot: *slot, blob, title, artist, album });
    }
    pack::write_pack(out, name, author, description, &songs)
}



// ---------------------------------------------------------------------------
// A song from a file
// ---------------------------------------------------------------------------

/// What the launcher learned about a file before the user decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    pub tags: tags::Tags,
    /// The cleaned file name, which is what the lookup searched for when the
    /// tags were missing. Shown so "not right?" starts from something.
    pub query: String,
    pub duration_ms: u64,
    pub too_long: bool,
    /// Online candidates, best first. Empty when the tags were complete or
    /// nothing was found.
    pub candidates: Vec<lookup::Candidate>,
    /// Where the pre-filled details came from: "tags", "lookup", or "none".
    pub details_from: &'static str,
}

impl Inspection {
    /// Title, artist, and album to pre-fill: tags first, else the best
    /// candidate, else nothing.
    pub fn suggested(&self) -> (String, String, String) {
        let t = &self.tags;
        if t.title.is_some() && t.artist.is_some() {
            return (t.title.clone().unwrap_or_default(), t.artist.clone().unwrap_or_default(), t.album.clone().unwrap_or_default());
        }
        if let Some(c) = self.candidates.first() {
            return (
                t.title.clone().unwrap_or_else(|| c.title.clone()),
                t.artist.clone().unwrap_or_else(|| c.artist.clone()),
                t.album.clone().unwrap_or_else(|| c.album.clone()),
            );
        }
        (t.title.clone().unwrap_or_default(), t.artist.clone().unwrap_or_default(), t.album.clone().unwrap_or_default())
    }
}

/// Read a file's tags and length, and look it up online by its cleaned name
/// when the title or artist is missing. `http` is `None` for an offline
/// inspection (the CLI's --offline, the tests).
pub fn inspect(http: Option<&dyn crate::net::Http>, path: &Path) -> Result<Inspection> {
    let audio = decode::decode_file(path)?;
    let t = tags::read(path);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let query = tags::clean_query(&stem);
    let complete = t.title.is_some() && t.artist.is_some();
    let candidates = match (complete, http) {
        (false, Some(h)) if !query.is_empty() => lookup::lookup(h, &query),
        _ => Vec::new(),
    };
    let details_from = if complete {
        "tags"
    } else if !candidates.is_empty() {
        "lookup"
    } else {
        "none"
    };
    let too_long = audio.duration_ms > segments::slot_limit_hint() * 1000;
    Ok(Inspection { tags: t, query, duration_ms: audio.duration_ms, too_long, candidates, details_from })
}

/// Look a typed query up online. Never errors; empty when nothing was found.
pub fn lookup_query(http: &dyn crate::net::Http, query: &str) -> Vec<lookup::Candidate> {
    lookup::lookup(http, tags::clean_query(query).as_str())
}

/// Stage a song from a file: copy the file into `music/sources`, decode it,
/// encode it for the slot, cache the blob, and record the slot as pending.
/// `on(k, 40)` reports encoding progress.
#[derive(Debug, Clone, Copy)]
pub struct FileRequest<'a> {
    pub data_root: &'a Path,
    pub pristine_pathfind: &'a [u8],
    pub path: &'a Path,
    pub slot: Slot,
    pub title: &'a str,
    pub artist: &'a str,
    pub album: &'a str,
}

pub fn stage_file(req: &FileRequest<'_>, on: &mut dyn FnMut(usize, usize)) -> Result<Entry> {
    let FileRequest { data_root, pristine_pathfind, path, slot, title, artist, album } = *req;
    for (field, text) in [("title", title), ("artist", artist), ("album", album)] {
        if text.chars().count() > eatx::MAX_TEXT {
            return Err(Refusal::TooLongText { field, len: text.chars().count() }.into());
        }
    }
    let bytes = std::fs::read(path).map_err(|e| io(path, e))?;
    let track = state::track_id(&bytes);
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_else(|| "bin".into());
    let sources = state::sources_dir(data_root);
    std::fs::create_dir_all(&sources).map_err(|e| io(&sources, e))?;
    let copy = sources.join(format!("{track}.{ext}"));
    if !copy.is_file() {
        let tmp = sources.join(format!("{track}.{ext}.tmp"));
        std::fs::write(&tmp, &bytes).map_err(|e| io(&tmp, e))?;
        std::fs::rename(&tmp, &copy).map_err(|e| io(&copy, e))?;
    }
    drop(bytes);

    let audio = decode::decode_file(&copy)?;
    let pts = segments::stock_pt_headers(pathfind::asf(pristine_pathfind), slot)?;
    let blob = segments::encode_slot(&audio.left, &audio.right, &pts, on)?;
    let encoded = state::encoded_dir(data_root);
    std::fs::create_dir_all(&encoded).map_err(|e| io(&encoded, e))?;
    let blob_path = state::encoded_blob_path(data_root, &track, slot);
    let tmp = blob_path.with_extension("asf.tmp");
    std::fs::write(&tmp, &blob).map_err(|e| io(&tmp, e))?;
    std::fs::rename(&tmp, &blob_path).map_err(|e| io(&blob_path, e))?;

    let entry = Entry {
        source: Source::UserFile { track: track.clone(), file: format!("music/sources/{track}.{ext}") },
        title: title.to_string(),
        artist: artist.to_string(),
        album: album.to_string(),
        duration_ms: Some(audio.duration_ms),
    };
    let mut st = Soundtrack::load(data_root)?;
    st.stage(slot, entry.clone());
    st.save(data_root)?;
    Ok(entry)
}

// ---------------------------------------------------------------------------
// Revert, and the one entry point the page and the CLI call
// ---------------------------------------------------------------------------

/// Put the original soundtrack back: re-establish the pristine image as the
/// booted one and re-apply the installed roster through the content
/// pipeline. Custom slots are kept in the record so the switch can go back
/// on without re-encoding anything.
pub fn revert(platform: &Platform, data_root: &Path, updater: &Updater<'_>, hook: &ImageHook) -> Result<Outcome> {
    let Some(pristine) = game_image::find_pristine(data_root) else {
        return Ok(Outcome::Refused(Refusal::NoPristine));
    };
    let booted = game_image::booted_path(data_root);
    let before = if booted.is_file() { Some(hash_file(&booted)?) } else { None };
    match game_image::establish(platform, data_root, &pristine.path)? {
        EstablishOutcome::Done(_) => {}
        EstablishOutcome::Refused(EstablishRefusal::NoPristine { .. }) => return Ok(Outcome::Refused(Refusal::NoPristine)),
        EstablishOutcome::Refused(EstablishRefusal::BootedInUse { path }) => {
            return Ok(Outcome::Refused(Refusal::ImageInUse { path }))
        }
    }
    let roster_note = match updater.apply_current_roster(hook)? {
        RosterResult::Applied(_) | RosterResult::Unchanged { .. } | RosterResult::NothingToApply => None,
        RosterResult::NoBuilder { .. } => {
            Some("The roster could not be put back because no Reborn Builder was found; the next content check will do it.".to_string())
        }
        RosterResult::Refused(r) => Some(format!("The roster was not put back: {}", r.message())),
    };
    let after = hash_file(&booted)?;
    let mut st = Soundtrack::load(data_root)?;
    st.commit_pending();
    st.rebuilt_sha256 = None;
    st.base_sha256 = Some(pristine.folder_sha256);
    st.save(data_root)?;
    Ok(Outcome::Applied(Applied { image: booted, before, after, slots_changed: Vec::new(), skipped: Vec::new(), roster_note }))
}

/// Apply when the switch is on and any slot is custom; otherwise put the
/// original soundtrack back. A record that was never applied and has
/// nothing staged is refused rather than rebuilt for nothing.
pub fn apply_or_revert(
    platform: &Platform,
    data_root: &Path,
    updater: &Updater<'_>,
    hook: &ImageHook,
    disc_size: Option<u64>,
    on: &mut dyn FnMut(Progress),
) -> Result<Outcome> {
    let st = Soundtrack::load(data_root)?;
    let base_pack = match st.base_pack(data_root) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("the soundtrack base pack could not be read, treating the base as stock: {e}");
            None
        }
    };
    let effective = st.effective(data_root, base_pack.as_ref(), true);
    let custom = st.enabled && !Soundtrack::is_all_stock(&effective);
    if custom {
        return apply_with_size(platform, data_root, disc_size, on);
    }
    if st.rebuilt_sha256.is_none() && !st.has_pending() {
        return Ok(Outcome::Refused(Refusal::NothingStaged));
    }
    revert(platform, data_root, updater, hook)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::state::Origin;
    use crate::soundtrack::testing::{synthetic_disc, synthetic_pathfind, synthetic_stream, synthetic_template, SYN_PT};

    fn blob(samples: u32) -> Vec<u8> {
        (0..40).flat_map(|_| synthetic_stream(SYN_PT, samples, 512)).collect()
    }

    #[test]
    fn build_files_swaps_one_slot_and_leaves_the_rest_stock() {
        let pf = synthetic_pathfind(256);
        let tpl = synthetic_template();
        let dir = tempfile::tempdir().unwrap();
        let b = blob(4800);
        let blob_path = dir.path().join("x.asf");
        std::fs::write(&blob_path, &b).unwrap();
        let mut eff: BTreeMap<Slot, Effective> = Slot::all().map(|s| (s, Effective::Stock)).collect();
        eff.insert(
            Slot::new(9).unwrap(),
            Effective::Blob { blob: BlobRef::Encoded(blob_path), title: "New".into(), artist: "Me".into(), album: String::new(), origin: Origin::UserFile },
        );
        let built = build_files(&pf, &tpl, &tpl, &eff, &mut |_| {}).unwrap();
        assert_eq!(built.changed, vec![9]);
        assert!(built.skipped.is_empty());
        let a = pathfind::asf(&built.pathfind);
        assert_eq!(asf::slot_blob(a, Slot::new(9).unwrap()).unwrap(), b);
        assert_eq!(asf::slot_blob(a, Slot::new(10).unwrap()).unwrap(), asf::slot_blob(pathfind::asf(&pf), Slot::new(10).unwrap()).unwrap());
        let recs = eatx::read_records(&built.template).unwrap();
        assert_eq!(recs[8].title, "New");
        assert_eq!(recs[9].title, "Song 10");
        pathfind::Pathfind::parse(&built.pathfind).unwrap();
        // slot 9's w1 follows the new sample count: 4800 / 24 = 200 ms
        let k = 44 + 7 * 40;
        let w1 = u32::from_be_bytes(built.pathfind[0x1138C + 8 * k + 4..0x1138C + 8 * k + 8].try_into().unwrap());
        assert_eq!(w1, 200);
    }

    #[test]
    fn build_files_keeps_the_booted_template_and_restores_stock_text_on_reset() {
        let pf = synthetic_pathfind(256);
        let pristine_tpl = synthetic_template();
        let mut booted_tpl = pristine_tpl.clone();
        // the roster Builder changed unrelated bytes, and a previous apply
        // wrote custom text into slot 2
        booted_tpl[0x100] = 0x77;
        eatx::write_record(&mut booted_tpl, Slot::new(2).unwrap(), &eatx::Record { title: "Old custom".into(), album: String::new(), artist: String::new() }).unwrap();
        let eff: BTreeMap<Slot, Effective> = Slot::all().map(|s| (s, Effective::Stock)).collect();
        let built = build_files(&pf, &pristine_tpl, &booted_tpl, &eff, &mut |_| {}).unwrap();
        assert_eq!(built.template[0x100], 0x77, "roster bytes kept");
        assert_eq!(eatx::read_records(&built.template).unwrap()[1].title, "Song 2", "stock text restored");
        assert!(built.changed.is_empty());
    }

    #[test]
    fn a_missing_blob_is_skipped_not_fatal() {
        let pf = synthetic_pathfind(256);
        let tpl = synthetic_template();
        let mut eff: BTreeMap<Slot, Effective> = Slot::all().map(|s| (s, Effective::Stock)).collect();
        eff.insert(
            Slot::new(5).unwrap(),
            Effective::Blob { blob: BlobRef::Encoded(PathBuf::from("Z:/nowhere/gone.asf")), title: "X".into(), artist: String::new(), album: String::new(), origin: Origin::UserFile },
        );
        let built = build_files(&pf, &tpl, &tpl, &eff, &mut |_| {}).unwrap();
        assert!(built.changed.is_empty());
        assert_eq!(built.skipped.len(), 1);
        assert_eq!(built.skipped[0].0, 5);
        assert_eq!(eatx::read_records(&built.template).unwrap()[4].title, "Song 5");
    }

    /// A data root with a pristine synthetic disc and a booted copy of it.
    fn scratch_root() -> (tempfile::TempDir, PathBuf, PathBuf, u64) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let pf = synthetic_pathfind(256);
        let tpl = synthetic_template();
        let disc_dir = root.join("disc-src");
        std::fs::create_dir_all(&disc_dir).unwrap();
        let iso = synthetic_disc(&disc_dir, &[("a.dat", b"aaaa"), ("pathfind.dat", &pf), ("template.dat", &tpl), ("z.dat", b"zz")]);
        let sha = hash_file(&iso).unwrap();
        let pristine_dir = game_image::pristine_dir(&root, sha.as_str());
        std::fs::create_dir_all(&pristine_dir).unwrap();
        let pristine = pristine_dir.join(IMAGE_FILE);
        std::fs::rename(&iso, &pristine).unwrap();
        let booted = game_image::booted_path(&root);
        std::fs::create_dir_all(booted.parent().unwrap()).unwrap();
        std::fs::copy(&pristine, &booted).unwrap();
        let size = std::fs::metadata(&pristine).unwrap().len();
        (dir, pristine, booted, size)
    }

    #[test]
    fn apply_rebuilds_the_booted_image_records_the_hash_and_commits_pending() {
        let (dir, pristine, booted, size) = scratch_root();
        let root = dir.path();
        let platform = Platform::detect();
        // nothing staged: refused, nothing written
        assert!(matches!(apply_with_size(&platform, root, Some(size + 0x40000), &mut |_| {}).unwrap(), Outcome::Refused(Refusal::NothingStaged)));
        // stage a pack for slot 21
        let pack_path = root.join("friend.nsrmusic");
        pack::write_pack(&pack_path, "Friend", "f", "", &[pack::SongInput { slot: Slot::new(21).unwrap(), blob: blob(9600), title: "Pack song".into(), artist: "Them".into(), album: String::new() }]).unwrap();
        assert_eq!(stage_pack(root, &pack_path).unwrap(), vec![21]);
        assert!(state::packs_dir(root).join("friend.nsrmusic").is_file());
        let pristine_hash = hash_file(&pristine).unwrap();
        let mut seen = Vec::new();
        let outcome = apply_with_size(&platform, root, Some(size + 0x40000), &mut |p| seen.push(p)).unwrap();
        let applied = match outcome {
            Outcome::Applied(a) => a,
            other => panic!("{other:?}"),
        };
        assert_eq!(applied.slots_changed, vec![21]);
        assert_eq!(applied.before, Some(pristine_hash.clone()));
        assert_eq!(hash_file(&pristine).unwrap(), pristine_hash, "pristine untouched");
        assert_eq!(hash_file(&booted).unwrap(), applied.after);
        assert!(seen.iter().any(|p| matches!(p, Progress::Building { .. })));
        let rebuild = RebuildState::load(&state::rebuild_state_path(root)).unwrap();
        assert_eq!(rebuild.rebuilt.get(IMAGE_FILE), Some(&applied.after));
        let st = Soundtrack::load(root).unwrap();
        assert!(!st.has_pending());
        assert!(matches!(st.slots[&21].source, Source::PackStream { slot: 21, .. }));
        assert_eq!(st.rebuilt_sha256.as_deref(), Some(applied.after.as_str()));
        // the booted disc carries the song and its text
        let disc = gcm::Disc::open(&booted).unwrap();
        let new_pf = disc.read_file("pathfind.dat").unwrap();
        assert_eq!(asf::slot_blob(pathfind::asf(&new_pf), Slot::new(21).unwrap()).unwrap(), blob(9600));
        assert_eq!(eatx::read_records(&disc.read_file("template.dat").unwrap()).unwrap()[20].title, "Pack song");
        assert_eq!(disc.read_file("z.dat").unwrap(), b"zz");
        // applying again with nothing new is unchanged
        assert!(matches!(apply_with_size(&platform, root, Some(size + 0x40000), &mut |_| {}).unwrap(), Outcome::Unchanged { .. }));
        // the switch off refuses
        set_enabled(root, false).unwrap();
        assert!(matches!(apply_with_size(&platform, root, Some(size + 0x40000), &mut |_| {}).unwrap(), Outcome::Refused(Refusal::Disabled)));
    }

    #[test]
    fn stock_songs_come_from_the_pristine_image() {
        let (dir, pristine, _booted, _size) = scratch_root();
        let _ = dir.path();
        let songs = stock_songs(&pristine).unwrap();
        assert_eq!(songs.len(), 21);
        assert_eq!(songs[0].title, "Song 1");
        assert_eq!(songs[20].artist, "Artist 21");
        assert_eq!(songs[0].duration_ms, 4000);
    }

    #[test]
    fn details_and_reset_stage_entries_and_export_reads_them_back() {
        let (dir, pristine, _booted, _size) = scratch_root();
        let root = dir.path();
        stage_details(root, Slot::new(4).unwrap(), Some("Renamed"), None, Some("An Album")).unwrap();
        let st = Soundtrack::load(root).unwrap();
        assert_eq!(st.pending[&4].title, "Renamed");
        assert!(matches!(st.pending[&4].source, Source::Stock));
        assert!(stage_details(root, Slot::new(4).unwrap(), Some(&"x".repeat(60)), None, None).is_err());
        stage_stock(root, Slot::new(6).unwrap()).unwrap();
        assert!(unstage(root, Slot::new(6).unwrap()).unwrap());
        assert!(!unstage(root, Slot::new(6).unwrap()).unwrap());
        assert_eq!(unstage_all(root).unwrap(), 1);
        // export two stock slots from the pristine files
        let disc = gcm::Disc::open(&pristine).unwrap();
        let pf = disc.read_file("pathfind.dat").unwrap();
        let tpl = disc.read_file("template.dat").unwrap();
        let out = root.join("out.nsrmusic");
        let man = export(&ExportRequest {
            data_root: root,
            pristine_pathfind: &pf,
            pristine_template: &tpl,
            slots: &[Slot::new(1).unwrap(), Slot::new(2).unwrap()],
            out: &out,
            name: "Two",
            author: "me",
            description: "",
        })
        .unwrap();
        assert_eq!(man.songs.len(), 2);
        assert_eq!(man.songs[&2].title, "Song 2");
        let back = pack::read_pack(&out).unwrap();
        assert_eq!(pack::read_blob(&back, Slot::new(1).unwrap()).unwrap(), asf::slot_blob(pathfind::asf(&pf), Slot::new(1).unwrap()).unwrap());
    }

    #[test]
    fn revert_puts_the_pristine_image_back_and_clears_the_record() {
        let (dir, pristine, booted, size) = scratch_root();
        let root = dir.path();
        let platform = Platform::detect();
        let pack_path = root.join("p.nsrmusic");
        pack::write_pack(&pack_path, "P", "", "", &[pack::SongInput { slot: Slot::new(2).unwrap(), blob: blob(4800), title: "S".into(), artist: String::new(), album: String::new() }]).unwrap();
        stage_pack(root, &pack_path).unwrap();
        let origin = crate::origin::Origin::new("http://127.0.0.1:9/");
        let verifier = crate::signing::Ed25519Verifier::embedded();
        let updater = Updater::new(&platform, root, &origin, &verifier);
        let hook = ImageHook { builder: None, xdelta: None, extra_dirs: Vec::new(), textures_root: None };
        // custom slot staged: apply_or_revert applies
        match apply_or_revert(&platform, root, &updater, &hook, Some(size + 0x40000), &mut |_| {}).unwrap() {
            Outcome::Applied(a) => assert_eq!(a.slots_changed, vec![2]),
            other => panic!("{other:?}"),
        }
        assert_ne!(hash_file(&booted).unwrap(), hash_file(&pristine).unwrap());
        // switch off: apply_or_revert reverts, roster apply has nothing to do
        set_enabled(root, false).unwrap();
        match apply_or_revert(&platform, root, &updater, &hook, Some(size + 0x40000), &mut |_| {}).unwrap() {
            Outcome::Applied(a) => {
                assert!(a.slots_changed.is_empty());
                assert_eq!(a.roster_note, None);
                assert_eq!(a.after, hash_file(&pristine).unwrap());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(hash_file(&booted).unwrap(), hash_file(&pristine).unwrap());
        let st = Soundtrack::load(root).unwrap();
        assert_eq!(st.rebuilt_sha256, None);
        assert!(st.slots.contains_key(&2), "the custom slot is kept for the switch to go back on");
        assert!(!RebuildState::load(&state::rebuild_state_path(root)).unwrap().rebuilt.contains_key(IMAGE_FILE));
        // a fresh record with nothing staged and nothing applied is refused
        std::fs::remove_file(state::soundtrack_path(root)).unwrap();
        assert!(matches!(apply_or_revert(&platform, root, &updater, &hook, Some(size), &mut |_| {}).unwrap(), Outcome::Refused(Refusal::NothingStaged)));
    }

    /// Oracle 2: applying a pack through Rust yields the same pathfind.dat
    /// and template.dat as `music_pack.py import` on the same inputs.
    #[test]
    fn oracle_pack_import_matches_music_pack_py() {
        let Some(clean) = std::env::var_os("NSR_CLEAN") else {
            eprintln!("skipped: NSR_CLEAN unset");
            return;
        };
        let clean = PathBuf::from(clean);
        let root_dir = clean.join("GameCube Version").join("root");
        let stock_pf = root_dir.join("pathfind.dat.bak_good_2026-07-02");
        let pf = std::fs::read(&stock_pf).unwrap();
        let tpl = std::fs::read(root_dir.join("template.dat")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        // a real different song into slot 21: slot 3's stock blob
        let blob = asf::slot_blob(pathfind::asf(&pf), Slot::new(3).unwrap()).unwrap();
        let pack_path = dir.path().join("oracle.nsmp");
        crate::soundtrack::testing::write_v1_pack(&pack_path, 21, &blob, "Ground Zero", "Oracle Artist", "Oracle Album");
        // Python
        let out = dir.path().join("py");
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("Tools").join("oracle-pack-import.py");
        let status = std::process::Command::new("py")
            .args(["-3", &script.to_string_lossy(), &pack_path.to_string_lossy(), &stock_pf.to_string_lossy()])
            .arg(root_dir.join("template.dat"))
            .arg(&out)
            .env("NSR_CLEAN", &clean)
            .status()
            .unwrap();
        assert!(status.success(), "the Python oracle failed");
        let py_pf = std::fs::read(out.join("pathfind.dat")).unwrap();
        let py_tpl = std::fs::read(out.join("template.dat")).unwrap();
        // Rust
        let mut eff: BTreeMap<Slot, Effective> = Slot::all().map(|s| (s, Effective::Stock)).collect();
        eff.insert(
            Slot::new(21).unwrap(),
            Effective::Blob {
                blob: BlobRef::Pack { pack: pack_path.clone(), slot: Slot::new(21).unwrap() },
                title: "Ground Zero".into(),
                artist: "Oracle Artist".into(),
                album: "Oracle Album".into(),
                origin: Origin::Pack,
            },
        );
        let built = build_files(&pf, &tpl, &tpl, &eff, &mut |_| {}).unwrap();
        assert_eq!(built.changed, vec![21]);
        assert_eq!(built.pathfind.len(), py_pf.len(), "pathfind.dat length");
        let first_diff = built.pathfind.iter().zip(&py_pf).position(|(a, b)| a != b);
        assert_eq!(first_diff, None, "pathfind.dat differs at 0x{:X}", first_diff.unwrap_or(0));
        assert_eq!(built.template, py_tpl, "template.dat differs");
        eprintln!("oracle 2: pathfind.dat {} bytes and template.dat identical", py_pf.len());
    }

    #[test]
    fn stage_file_encodes_for_the_slot_caches_the_blob_and_copies_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let pf = synthetic_pathfind(256);
        // three seconds of two tones
        let n = 24_000 * 3;
        let left: Vec<i16> = (0..n).map(|i| ((i as f64 * 0.03).sin() * 9000.0) as i16).collect();
        let right: Vec<i16> = (0..n).map(|i| ((i as f64 * 0.05).sin() * 6000.0) as i16).collect();
        let wav = root.join("My Song.wav");
        crate::soundtrack::testing::write_wav_24k(&wav, &left, &right);
        let mut ticks = 0;
        let entry = stage_file(&FileRequest { data_root: root, pristine_pathfind: &pf, path: &wav, slot: Slot::new(7).unwrap(), title: "My Song", artist: "Me", album: "" }, &mut |_, _| ticks += 1).unwrap();
        assert_eq!(ticks, 40);
        assert_eq!(entry.duration_ms, Some(3000));
        let Source::UserFile { track, file } = &entry.source else { panic!("{entry:?}") };
        assert_eq!(track.len(), 16);
        assert!(root.join(file).is_file(), "source copied");
        let blob_path = state::encoded_blob_path(root, track, Slot::new(7).unwrap());
        let blob = std::fs::read(&blob_path).unwrap();
        let info = asf::validate_blob(&blob).unwrap();
        assert_eq!(info.streams, 40);
        // Segments are padded to a uniform size with silent blocks, and by
        // the EA-exact rule those count in PT 0x85, so a 3 s clip declares
        // more stream than audio. The entry keeps the real length.
        assert!(info.duration_ms >= 3000, "{}", info.duration_ms);
        let st = Soundtrack::load(root).unwrap();
        assert_eq!(st.pending[&7].title, "My Song");
        // the same file again reuses the copy and re-encodes without error
        let before = std::fs::metadata(root.join(file)).unwrap().modified().unwrap();
        stage_file(&FileRequest { data_root: root, pristine_pathfind: &pf, path: &wav, slot: Slot::new(8).unwrap(), title: "My Song", artist: "Me", album: "" }, &mut |_, _| {}).unwrap();
        assert_eq!(std::fs::metadata(root.join(file)).unwrap().modified().unwrap(), before);
        assert!(state::encoded_blob_path(root, track, Slot::new(8).unwrap()).is_file());
        // too much text is refused before any work
        assert!(stage_file(&FileRequest { data_root: root, pristine_pathfind: &pf, path: &wav, slot: Slot::new(9).unwrap(), title: &"x".repeat(60), artist: "", album: "" }, &mut |_, _| {}).is_err());
    }

    #[test]
    fn inspect_reads_tags_and_length_offline() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("03. Alicia Keys - Unthinkable (Official Video).wav");
        crate::soundtrack::testing::write_wav_24k(&wav, &[0; 24_000], &[0; 24_000]);
        let i = inspect(None, &wav).unwrap();
        assert_eq!(i.duration_ms, 1000);
        assert_eq!(i.query, "Alicia Keys Unthinkable");
        assert_eq!(i.details_from, "none");
        assert!(!i.too_long);
        assert_eq!(i.suggested(), (String::new(), String::new(), String::new()));
    }
}
