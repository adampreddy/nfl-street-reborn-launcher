//! The Music page as the window drives it: status, staging, one rebuild.
//!
//! `reborn_core::soundtrack` does the work. What lives here is the state
//! between calls (an encode or a rebuild running on a thread) held in the
//! process rather than in a page, so navigating away and back does not lose
//! it, and every page asks `music_status` and gets the same answer. Same
//! shape as `content_updates`.
//!
//! A rebuild takes the launch guard for its duration, so the game cannot be
//! started while the disc is being swapped, and it refuses to start while a
//! game is running.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

use reborn_core::content::pipeline::Updater;
use reborn_core::origin::Origin;
use reborn_core::soundtrack::apply::{self, Outcome, Progress};
use reborn_core::soundtrack::state::{Base, Effective, Origin as SlotOrigin, Soundtrack, Source};
use reborn_core::soundtrack::{gcm, pack, segments, Slot};
use reborn_core::Platform;

use crate::content_updates::{image_hook, load_settings, verifier};
use crate::LaunchGuard;

#[derive(Debug)]
enum Phase {
    Idle,
    /// One song being encoded; `file` of `files` when several were added
    /// at once, and the file's name for the progress line.
    Encoding { slot: u8, done: usize, total: usize, file: usize, files: usize, name: String },
    Building { done: u64, total: u64 },
    Done,
    Problem,
}

impl Phase {
    fn name(&self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Encoding { .. } => "encoding",
            Phase::Building { .. } => "building",
            Phase::Done => "done",
            Phase::Problem => "problem",
        }
    }
}

#[derive(Debug)]
struct Held {
    phase: Phase,
    /// The last sentence worth showing: what a rebuild did, or why not.
    note: String,
}

static HELD: Mutex<Held> = Mutex::new(Held { phase: Phase::Idle, note: String::new() });

fn held() -> std::sync::MutexGuard<'static, Held> {
    HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn busy() -> bool {
    matches!(held().phase, Phase::Encoding { .. } | Phase::Building { .. })
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressView {
    pub label: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingView {
    /// "original", "your_file", "pack"
    pub kind: &'static str,
    pub title: String,
    pub artist: String,
    pub album: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OriginalView {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotView {
    pub slot: u8,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: Option<u64>,
    /// "original", "your_file", "pack", "soundtrack_mod"
    pub source: &'static str,
    pub source_name: Option<String>,
    pub pending: Option<PendingView>,
    /// The song the disc shipped with in this slot, whatever plays now.
    /// Absent only when no pristine image can be read.
    pub original: Option<OriginalView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaseView {
    /// "original" or "package"
    pub kind: &'static str,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MusicStatusView {
    /// "idle", "encoding", "building", "done", "problem"
    pub phase: &'static str,
    pub progress: Option<ProgressView>,
    pub enabled: bool,
    pub base: BaseView,
    pub slots: Vec<SlotView>,
    pub pending_count: usize,
    pub note: String,
    /// Both game images exist, so an apply can run.
    pub image_ok: bool,
    pub image_note: String,
    pub limit_seconds: u64,
    pub formats: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidateView {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: Option<u64>,
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectView {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// "tags", "lookup", "none"
    pub details_from: &'static str,
    pub duration_ms: u64,
    pub too_long: bool,
    pub query: String,
    pub candidates: Vec<CandidateView>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackSongView {
    pub slot: u8,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: Option<u64>,
    /// What the slot shows now, in a phrase.
    pub replaces: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackPreviewView {
    pub path: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub format: u32,
    pub songs: Vec<PackSongView>,
}

/// The disc's own 21 songs, read once per pristine image and kept for the
/// life of the process (music_status is polled every second during a build).
static STOCK: Mutex<Option<(String, Vec<apply::StockSong>)>> = Mutex::new(None);

fn stock_songs(root: &Path) -> Option<Vec<apply::StockSong>> {
    let pristine = reborn_core::game_image::find_pristine(root)?;
    let mut cache = STOCK.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((sha, songs)) = cache.as_ref() {
        if *sha == pristine.folder_sha256 {
            return Some(songs.clone());
        }
    }
    let songs = apply::stock_songs(&pristine.path).ok()?;
    *cache = Some((pristine.folder_sha256.clone(), songs.clone()));
    Some(songs)
}

fn data_root() -> Result<PathBuf, String> {
    Platform::detect().paths.data_root().map_err(|e| e.to_string())
}

fn http() -> reborn_core::net::UreqHttp {
    reborn_core::net::UreqHttp::new(&format!("NFL Street Reborn Launcher/{}", crate::launcher_version()))
}

fn candidate(c: &reborn_core::soundtrack::lookup::Candidate) -> CandidateView {
    CandidateView { title: c.title.clone(), artist: c.artist.clone(), album: c.album.clone(), duration_ms: c.duration_ms, source: c.source }
}

fn slot_phrase(eff: &Effective) -> String {
    match eff {
        Effective::Stock { .. } => "the original song".to_string(),
        Effective::Blob { title, artist, .. } if artist.is_empty() => title.clone(),
        Effective::Blob { title, artist, .. } => format!("{title} by {artist}"),
    }
}

fn view(root: &Path) -> Result<MusicStatusView, String> {
    let st = Soundtrack::load(root).map_err(|e| e.to_string())?;
    let base_pack = st.base_pack(root).ok().flatten();
    let applied = st.effective(root, base_pack.as_ref(), false);
    let base = match &st.base {
        Base::Stock => BaseView { kind: "original", name: None },
        Base::Package { id, .. } => BaseView { kind: "package", name: Some(base_pack.as_ref().map(|p| p.manifest.name.clone()).unwrap_or_else(|| id.clone())) },
    };
    let stock = stock_songs(root);
    let mut slots = Vec::with_capacity(21);
    for slot in Slot::all() {
        let n = slot.get();
        let original = stock.as_ref().and_then(|s| s.get(slot.index())).map(|o| OriginalView { title: o.title.clone(), artist: o.artist.clone(), album: o.album.clone(), duration_ms: o.duration_ms });
        let (title, artist, album, duration_ms, source, source_name) = match &applied[&slot] {
            // The disc's own song, shown under the player's name when they
            // renamed it: the audio is the disc's, the banner is theirs.
            Effective::Stock { text } => match (&original, text) {
                (Some(o), Some(t)) => (t.title.clone(), t.artist.clone(), t.album.clone(), Some(o.duration_ms), "original", None),
                (Some(o), None) => (o.title.clone(), o.artist.clone(), o.album.clone(), Some(o.duration_ms), "original", None),
                (None, Some(t)) => (t.title.clone(), t.artist.clone(), t.album.clone(), None, "original", None),
                (None, None) => (String::new(), String::new(), String::new(), None, "original", None),
            },
            Effective::Blob { title, artist, album, origin, .. } => {
                let entry = st.slots.get(&n);
                let duration = entry.and_then(|e| e.duration_ms).or_else(|| base_pack.as_ref().and_then(|p| p.manifest.songs.get(&n)).and_then(|s| s.duration_ms));
                let (source, name) = match origin {
                    SlotOrigin::UserFile => ("your_file", entry.and_then(|e| match &e.source { Source::UserFile { file, .. } => Path::new(file).file_name().map(|f| f.to_string_lossy().into_owned()), _ => None })),
                    SlotOrigin::Pack => ("pack", entry.and_then(|e| match &e.source { Source::PackStream { pack, .. } => Path::new(pack).file_name().map(|f| f.to_string_lossy().into_owned()), _ => None })),
                    SlotOrigin::BasePackage => ("soundtrack_mod", base.name.clone()),
                    // The disc's own song, moved here from another slot.
                    SlotOrigin::Original => ("original_moved", None),
                };
                (title.clone(), artist.clone(), album.clone(), duration, source, name)
            }
        };
        let pending = st.pending.get(&n).map(|e| PendingView {
            kind: match &e.source {
                Source::Stock => "original",
                Source::UserFile { .. } => "your_file",
                Source::PackStream { .. } => "pack",
                Source::StockFrom { .. } => "original_moved",
            },
            title: e.title.clone(),
            artist: e.artist.clone(),
            album: e.album.clone(),
        });
        slots.push(SlotView { slot: n, title, artist, album, duration_ms, source, source_name, pending, original });
    }
    let (image_ok, image_note) = match apply::images(root) {
        Ok(_) => (true, String::new()),
        Err(r) => (false, r.message()),
    };
    let h = held();
    let progress = match &h.phase {
        Phase::Encoding { slot, done, total, file, files, name } => Some(ProgressView {
            label: if *files > 1 { format!("Adding song {file} of {files}: {name}") } else { format!("Encoding slot {slot}") },
            done: *done as u64,
            total: *total as u64,
        }),
        Phase::Building { done, total } => Some(ProgressView { label: "Building your game disc".to_string(), done: *done, total: *total }),
        _ => None,
    };
    Ok(MusicStatusView {
        phase: h.phase.name(),
        progress,
        enabled: st.enabled,
        base,
        slots,
        pending_count: st.pending.len(),
        note: h.note.clone(),
        image_ok,
        image_note,
        limit_seconds: segments::slot_limit_hint(),
        formats: reborn_core::soundtrack::decode::EXTENSIONS.to_vec(),
    })
}

fn status_now() -> Result<MusicStatusView, String> {
    view(&data_root()?)
}

fn pristine_files(root: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (pristine, _) = apply::images(root).map_err(|r| r.message())?;
    let disc = gcm::Disc::open(&pristine).map_err(|e| e.to_string())?;
    Ok((disc.read_file("pathfind.dat").map_err(|e| e.to_string())?, disc.read_file("template.dat").map_err(|e| e.to_string())?))
}

fn slot_of(n: u8) -> Result<Slot, String> {
    Slot::new(n).map_err(|r| r.message())
}

fn refuse_if_busy() -> Result<(), String> {
    if busy() {
        return Err("The launcher is still working on your soundtrack. Wait for it to finish.".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn music_status() -> Result<MusicStatusView, String> {
    tauri::async_runtime::spawn_blocking(status_now).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn music_inspect(path: String) -> Result<InspectView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let h = http();
        let i = apply::inspect(Some(&h), &p).map_err(|e| e.to_string())?;
        let (title, artist, album) = i.suggested();
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let title = if title.is_empty() { stem } else { title };
        let note = match i.details_from {
            "tags" => "Details read from the file's tags.".to_string(),
            "lookup" => "Details found online from the file name. Check them.".to_string(),
            _ => "Type the song's details.".to_string(),
        };
        Ok(InspectView {
            path,
            title,
            artist,
            album,
            details_from: i.details_from,
            duration_ms: i.duration_ms,
            too_long: i.too_long,
            query: i.query,
            candidates: i.candidates.iter().map(candidate).collect(),
            note,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn music_lookup(query: String, duration_ms: Option<u64>) -> Result<Vec<CandidateView>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let h = http();
        Ok(apply::lookup_query(&h, &query, duration_ms).iter().map(candidate).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stage a song from a file. The encode runs on its own thread; the page
/// polls `music_status` for the encoding progress and the note.
#[tauri::command]
pub async fn music_stage_file(path: String, slot: u8, title: String, artist: String, album: String) -> Result<MusicStatusView, String> {
    refuse_if_busy()?;
    let slot = slot_of(slot)?;
    let root = data_root()?;
    let name = file_name(&path);
    {
        let mut h = held();
        h.phase = Phase::Encoding { slot: slot.get(), done: 0, total: 40, file: 1, files: 1, name: name.clone() };
        h.note = String::new();
    }
    let root2 = root.clone();
    std::thread::spawn(move || {
        let outcome = (|| -> Result<String, String> {
            let (pf, _) = pristine_files(&root2)?;
            let req = apply::FileRequest { data_root: &root2, pristine_pathfind: &pf, path: Path::new(&path), slot, title: &title, artist: &artist, album: &album };
            let entry = apply::stage_file(&req, &mut |k, n| {
                let mut h = held();
                h.phase = Phase::Encoding { slot: slot.get(), done: k, total: n, file: 1, files: 1, name: name.clone() };
            })
            .map_err(|e| e.to_string())?;
            Ok(format!("{} is staged for slot {slot}. Apply changes to put it on the disc.", if entry.title.is_empty() { "The song".to_string() } else { entry.title.clone() }))
        })();
        let mut h = held();
        match outcome {
            Ok(note) => {
                h.phase = Phase::Done;
                h.note = note;
            }
            Err(e) => {
                h.phase = Phase::Problem;
                h.note = e;
            }
        }
    });
    view(&root)
}

/// The last path component, for progress lines and notes.
fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

/// Several songs at once. Each is read (its tags, then the online lookup
/// for whatever the tags lack), then encoded into the next slot from
/// `start_slot` on, in the order given. A song that cannot be added is
/// skipped and named in the note; the rest still go in. Songs past slot 21
/// are counted and left out. The page polls `music_status` for the
/// per-song progress ("Adding song 2 of 5: name").
#[tauri::command]
pub async fn music_stage_files(paths: Vec<String>, start_slot: u8) -> Result<MusicStatusView, String> {
    refuse_if_busy()?;
    let first = slot_of(start_slot)?;
    if paths.is_empty() {
        return Err("Choose at least one song.".to_string());
    }
    let root = data_root()?;
    let files = paths.len();
    {
        let mut h = held();
        h.phase = Phase::Encoding { slot: first.get(), done: 0, total: 40, file: 1, files, name: file_name(&paths[0]) };
        h.note = String::new();
    }
    let root2 = root.clone();
    std::thread::spawn(move || {
        let pf = match pristine_files(&root2) {
            Ok((pf, _)) => pf,
            Err(e) => {
                let mut h = held();
                h.phase = Phase::Problem;
                h.note = e;
                return;
            }
        };
        let http = http();
        let mut added: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        let mut left_out = 0usize;
        let mut last_slot = first.get();
        for (k, path) in paths.iter().enumerate() {
            let name = file_name(path);
            let n = usize::from(first.get()) + k;
            let Some(slot) = u8::try_from(n).ok().and_then(|n| Slot::new(n).ok()) else {
                left_out += 1;
                continue;
            };
            {
                let mut h = held();
                h.phase = Phase::Encoding { slot: slot.get(), done: 0, total: 40, file: k + 1, files, name: name.clone() };
            }
            let progress_name = name.clone();
            let outcome = (|| -> Result<String, String> {
                let p = Path::new(path);
                let looked = apply::inspect(Some(&http), p).map_err(|e| e.to_string())?;
                if looked.too_long {
                    return Err(format!("longer than {} seconds", segments::slot_limit_hint()));
                }
                let (title, artist, album) = looked.suggested();
                let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let title = if title.is_empty() { stem } else { title };
                let req = apply::FileRequest { data_root: &root2, pristine_pathfind: &pf, path: p, slot, title: &title, artist: &artist, album: &album };
                let entry = apply::stage_file(&req, &mut |done, total| {
                    let mut h = held();
                    h.phase = Phase::Encoding { slot: slot.get(), done, total, file: k + 1, files, name: progress_name.clone() };
                })
                .map_err(|e| e.to_string())?;
                Ok(if entry.title.is_empty() { name.clone() } else { entry.title })
            })();
            match outcome {
                Ok(title) => {
                    added.push(title);
                    last_slot = slot.get();
                }
                Err(e) => skipped.push(format!("{name} ({e})")),
            }
        }
        let mut parts = Vec::new();
        match added.len() {
            0 => {}
            1 => parts.push(format!("Added {} to slot {}.", added[0], first.get())),
            n => parts.push(format!("Added {n} songs to slots {} to {last_slot}, in the order you chose them.", first.get())),
        }
        if !skipped.is_empty() {
            parts.push(format!("Not added: {}.", skipped.join(", ")));
        }
        if left_out > 0 {
            parts.push(format!("{left_out} did not fit: the soundtrack has 21 slots."));
        }
        if !added.is_empty() {
            parts.push("Check the names, then apply your changes to put them on the disc.".to_string());
        }
        let mut h = held();
        h.phase = if added.is_empty() { Phase::Problem } else { Phase::Done };
        h.note = parts.join(" ");
    });
    view(&root)
}

/// Move the song in `from` to `to` the way a list reorders; the songs
/// between shift by one. Every slot whose song changes is staged.
#[tauri::command]
pub async fn music_move(from: u8, to: u8) -> Result<MusicStatusView, String> {
    refuse_if_busy()?;
    let from = slot_of(from)?;
    let to = slot_of(to)?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = data_root()?;
        let stock = stock_songs(&root).unwrap_or_default();
        let changed = apply::move_slot(&root, from, to, &stock).map_err(|e| e.to_string())?;
        held().note = if changed.is_empty() { String::new() } else { format!("Song {from} is now song {to}. Apply changes to put the new order on the disc.") };
        view(&root)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn music_stage_stock(slot: u8) -> Result<MusicStatusView, String> {
    refuse_if_busy()?;
    let slot = slot_of(slot)?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = data_root()?;
        apply::stage_stock(&root, slot).map_err(|e| e.to_string())?;
        held().note = format!("Slot {slot} will go back to the original song when you apply.");
        view(&root)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn music_stage_details(slot: u8, title: String, artist: String, album: String) -> Result<MusicStatusView, String> {
    refuse_if_busy()?;
    let slot = slot_of(slot)?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = data_root()?;
        apply::stage_details(&root, slot, Some(&title), Some(&artist), Some(&album)).map_err(|e| e.to_string())?;
        held().note = format!("Slot {slot} details are staged. Apply changes to put them on the disc.");
        view(&root)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn music_unstage(slot: Option<u8>) -> Result<MusicStatusView, String> {
    refuse_if_busy()?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = data_root()?;
        match slot {
            Some(n) => {
                apply::unstage(&root, slot_of(n)?).map_err(|e| e.to_string())?;
            }
            None => {
                apply::unstage_all(&root).map_err(|e| e.to_string())?;
            }
        }
        held().note = String::new();
        view(&root)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn music_pack_preview(path: String) -> Result<PackPreviewView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = data_root()?;
        let p = pack::read_pack(Path::new(&path)).map_err(|e| e.to_string())?;
        let st = Soundtrack::load(&root).map_err(|e| e.to_string())?;
        let base_pack = st.base_pack(&root).ok().flatten();
        let applied = st.effective(&root, base_pack.as_ref(), false);
        let stock = stock_songs(&root);
        let songs = p
            .manifest
            .songs
            .iter()
            .filter_map(|(n, s)| {
                let slot = Slot::new(*n).ok()?;
                let replaces = match (&applied[&slot], stock.as_ref().and_then(|v| v.get(slot.index()))) {
                    (Effective::Stock { .. }, Some(o)) if !o.title.is_empty() => format!("the original song ({} by {})", o.title, o.artist),
                    (eff, _) => slot_phrase(eff),
                };
                Some(PackSongView {
                    slot: *n,
                    title: s.title.clone(),
                    artist: s.artist.clone(),
                    album: s.album.clone(),
                    duration_ms: s.duration_ms,
                    replaces,
                })
            })
            .collect();
        Ok(PackPreviewView {
            path,
            name: p.manifest.name.clone(),
            author: p.manifest.author.clone(),
            description: p.manifest.description.clone(),
            format: p.manifest.format,
            songs,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn music_stage_pack(path: String) -> Result<MusicStatusView, String> {
    refuse_if_busy()?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = data_root()?;
        let slots = apply::stage_pack(&root, Path::new(&path)).map_err(|e| e.to_string())?;
        held().note = format!("{} song(s) from the pack are staged. Apply changes to put them on the disc.", slots.len());
        view(&root)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn music_export(path: String, slots: Vec<u8>, name: String, author: String, description: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = data_root()?;
        let slots: Vec<Slot> = slots.into_iter().map(slot_of).collect::<Result<_, _>>()?;
        if slots.is_empty() {
            return Err("Pick at least one slot to share.".to_string());
        }
        let (pf, tpl) = pristine_files(&root)?;
        let mut out = PathBuf::from(&path);
        if out.extension().map(|e| !e.eq_ignore_ascii_case(pack::EXT_V2)).unwrap_or(true) {
            out.set_extension(pack::EXT_V2);
        }
        let man = apply::export(&apply::ExportRequest {
            data_root: &root,
            pristine_pathfind: &pf,
            pristine_template: &tpl,
            slots: &slots,
            out: &out,
            name: &name,
            author: &author,
            description: &description,
        })
        .map_err(|e| e.to_string())?;
        held().note = format!("Saved {} song(s) to {}.", man.songs.len(), out.display());
        Ok(out.display().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Run apply_or_revert now, under the launch guard, on the calling thread.
/// Used by the Mods page when a soundtrack package is switched, and by the
/// background thread `music_apply` starts.
pub(crate) fn rebuild_now(root: &Path, progress: &mut dyn FnMut(Progress)) -> Result<Outcome, String> {
    let guard = LaunchGuard::acquire().map_err(|_| "The game is running. Close it, then apply your soundtrack.".to_string())?;
    let p = Platform::detect();
    let settings = load_settings(&p)?;
    let verifier = verifier()?;
    let origin = Origin::new(&crate::content_updates::origin_url(&settings));
    let updater = Updater::new(&p, root, &origin, &verifier);
    let hook = image_hook(&p, &settings);
    let outcome = apply::apply_or_revert(&p, root, &updater, &hook, None, progress).map_err(|e| e.to_string());
    drop(guard);
    outcome
}

fn start_rebuild(root: PathBuf) -> Result<MusicStatusView, String> {
    if crate::session_active() {
        return Err("The game is running. Close it, then apply your soundtrack.".to_string());
    }
    {
        let mut h = held();
        h.phase = Phase::Building { done: 0, total: gcm::DISC_SIZE };
        h.note = String::new();
    }
    let root2 = root.clone();
    std::thread::spawn(move || {
        let outcome = rebuild_now(&root2, &mut |p| {
            if let Progress::Building { done, total } = p {
                held().phase = Phase::Building { done, total };
            }
        });
        let mut h = held();
        match outcome {
            Ok(o) => {
                h.phase = match o {
                    Outcome::Refused(_) => Phase::Problem,
                    _ => Phase::Done,
                };
                h.note = o.note();
            }
            Err(e) => {
                h.phase = Phase::Problem;
                h.note = e;
            }
        }
    });
    view(&root)
}

#[tauri::command]
pub async fn music_set_enabled(on: bool) -> Result<MusicStatusView, String> {
    refuse_if_busy()?;
    let root = data_root()?;
    apply::set_enabled(&root, on).map_err(|e| e.to_string())?;
    start_rebuild(root)
}

/// Forget every custom song and rebuild the disc with its own soundtrack.
#[tauri::command]
pub async fn music_reset_all(confirm: bool) -> Result<MusicStatusView, String> {
    crate::confirmed(confirm, "music_reset_all", "remove every custom song and rebuild the game with its own soundtrack")?;
    refuse_if_busy()?;
    let root = data_root()?;
    let had = apply::reset_all(&root).map_err(|e| e.to_string())?;
    {
        let mut h = held();
        h.note = if had == 0 {
            "There was nothing to remove.".to_string()
        } else {
            format!("Removed {had} custom song{}. Your files are kept.", if had == 1 { "" } else { "s" })
        };
    }
    start_rebuild(root)
}

#[tauri::command]
pub async fn music_apply(confirm: bool) -> Result<MusicStatusView, String> {
    crate::confirmed(confirm, "music_apply", "rebuild the offline game image with your soundtrack")?;
    refuse_if_busy()?;
    let root = data_root()?;
    start_rebuild(root)
}

/// Clear a finished or failed phase back to idle, once the page showed it.
#[tauri::command]
pub async fn music_dismiss() -> Result<MusicStatusView, String> {
    {
        let mut h = held();
        if matches!(h.phase, Phase::Done | Phase::Problem) {
            h.phase = Phase::Idle;
            h.note = String::new();
        }
    }
    tauri::async_runtime::spawn_blocking(status_now).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_view_serializes_with_every_field_named() {
        let v = MusicStatusView {
            phase: "idle",
            progress: None,
            enabled: true,
            base: BaseView { kind: "original", name: None },
            slots: vec![SlotView { slot: 1, title: String::new(), artist: String::new(), album: String::new(), duration_ms: None, source: "original", source_name: None, pending: None, original: None }],
            pending_count: 0,
            note: String::new(),
            image_ok: false,
            image_note: "x".into(),
            limit_seconds: 380,
            formats: vec!["mp3"],
        };
        let json = serde_json::to_value(&v).unwrap();
        for key in ["phase", "progress", "enabled", "base", "slots", "pending_count", "note", "image_ok", "image_note", "limit_seconds", "formats"] {
            assert!(json.get(key).is_some(), "{key}");
        }
        assert_eq!(json["slots"][0]["source"], "original");
    }

    #[test]
    fn a_fresh_root_reads_as_all_original_with_no_image() {
        let dir = tempfile::tempdir().unwrap();
        let v = view(dir.path()).unwrap();
        assert_eq!(v.slots.len(), 21);
        assert!(v.slots.iter().all(|s| s.source == "original" && s.pending.is_none()));
        assert!(!v.image_ok);
        assert!(v.image_note.contains("Import the game"));
        assert!(v.enabled);
    }
}
