//! A file picker drawn inside the launcher window, for Steam Deck Game Mode.
//!
//! The native dialog (rfd: the xdg portal or GTK) opens no window under
//! gamescope, so in Game Mode every "choose your game" step sat there and the
//! step could not be finished (Deck tester, 2026-10). Desktop Mode and
//! Windows keep the native dialog; only Game Mode comes here.
//!
//! `pick_path` (main.rs) calls [`pick`]: it emits `pick-path-request` to the
//! page, whose shared script (app-shell.js) draws a browser over
//! [`browse_dir`] and answers through [`pick_path_answer`]. One request at a
//! time; a second one cancels the first.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::Emitter;

static PENDING: Mutex<Option<Sender<Option<String>>>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize)]
struct Request {
    kind: String,
    /// Lower-case extensions without the dot; empty = any file.
    extensions: Vec<String>,
    /// Choose a folder instead of a file.
    folder: bool,
    title: String,
}

/// Ask the page for a path and wait for the answer (None = cancelled).
pub(crate) async fn pick(app: &tauri::AppHandle, kind: &str, extensions: &[&str], folder: bool, title: &str) -> Result<Option<String>, String> {
    let (tx, rx) = channel();
    if let Some(old) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).replace(tx) {
        let _ = old.send(None);
    }
    let req = Request {
        kind: kind.to_string(),
        extensions: extensions.iter().map(|e| e.to_ascii_lowercase()).collect(),
        folder,
        title: title.to_string(),
    };
    app.emit("pick-path-request", req).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(60 * 60)).unwrap_or(None))
        .await
        .map_err(|e| e.to_string())
}

/// The page's answer to the open request.
#[tauri::command]
pub fn pick_path_answer(path: Option<String>) {
    if let Some(tx) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = tx.send(path.filter(|p| !p.is_empty()));
    }
}

#[derive(Debug, Serialize)]
pub struct Entry {
    name: String,
    path: String,
    dir: bool,
    bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct Place {
    label: String,
    path: String,
}

#[derive(Debug, Serialize)]
pub struct Listing {
    path: String,
    parent: Option<String>,
    places: Vec<Place>,
    entries: Vec<Entry>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// The places a Deck player keeps files: home, Downloads, Desktop, and every
/// mounted SD card or USB drive (/run/media/<user>/<label>).
fn places() -> Vec<Place> {
    let h = home();
    let mut out = vec![Place { label: "Home".into(), path: h.display().to_string() }];
    for (label, sub) in [("Downloads", "Downloads"), ("Desktop", "Desktop")] {
        let p = h.join(sub);
        if p.is_dir() {
            out.push(Place { label: label.into(), path: p.display().to_string() });
        }
    }
    let mut media = Vec::new();
    for root in [PathBuf::from("/run/media"), PathBuf::from("/media")] {
        let Ok(level1) = std::fs::read_dir(&root) else { continue };
        for a in level1.flatten() {
            let Ok(level2) = std::fs::read_dir(a.path()) else { continue };
            for b in level2.flatten() {
                if b.path().is_dir() {
                    media.push(b.path());
                }
            }
            // /run/media/<label> on systems that mount without the user level.
            if a.path().is_dir() && std::fs::read_dir(a.path()).map(|mut d| d.any(|e| e.is_ok_and(|e| e.path().is_file()))).unwrap_or(false) {
                media.push(a.path());
            }
        }
    }
    media.sort();
    media.dedup();
    for m in media {
        let name = m.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        out.push(Place { label: format!("SD card / drive: {name}"), path: m.display().to_string() });
    }
    out
}

/// One folder's contents: folders first, then files, hidden ones left out.
#[tauri::command]
pub fn browse_dir(path: Option<String>) -> Result<Listing, String> {
    let dir = path.filter(|p| !p.is_empty()).map(PathBuf::from).unwrap_or_else(home);
    let read = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut entries: Vec<Entry> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let meta = std::fs::metadata(e.path()).ok()?;
            Some(Entry { name, path: e.path().display().to_string(), dir: meta.is_dir(), bytes: meta.len() })
        })
        .collect();
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Listing {
        parent: dir.parent().map(Path::to_path_buf).map(|p| p.display().to_string()),
        path: dir.display().to_string(),
        places: places(),
        entries,
    })
}
