//! Settings > Files & storage > "Move to another drive". The work is
//! `reborn_core::relocate`; this is the window's half: the copy on a worker
//! thread the page polls, then the helper that finishes the move once the
//! launcher has closed, and reopens it.
//!
//! The helper is this same executable, copied to the temp folder and started
//! with `--finish-move`. A copy, because the launcher usually runs from
//! inside the folder being moved, and Windows renames no folder that holds a
//! running program.
//!
//! Linux (the Steam Deck) needs no helper: the AppImage lives outside the
//! data root, and Linux renames a folder whatever is open in it. The move
//! finishes in this process and the page reloads, so nothing restarts inside
//! Game Mode.

use reborn_core::relocate::{self, Plan};
use reborn_core::Platform;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub const FINISH_SWITCH: &str = "--finish-move";
const HELPER_PREFIX: &str = "reborn-move-";

/// Where moving the game works: Windows, and Linux for the Steam Deck's SD
/// card. Not macOS yet (untested there).
const SUPPORTED: bool = cfg!(any(windows, target_os = "linux"));
const UNSUPPORTED: &str = "Moving the game is not available on this system yet.";

#[derive(Debug, Clone, Default, Serialize)]
pub struct Status {
    /// idle | copying | restarting | moved | failed
    pub phase: &'static str,
    pub done: u64,
    pub total: u64,
    pub target: String,
    pub message: String,
}

static STATUS: Mutex<Option<Status>> = Mutex::new(None);
static CANCEL: AtomicBool = AtomicBool::new(false);

fn set(update: impl FnOnce(&mut Status)) {
    let mut held = STATUS.lock().unwrap_or_else(|p| p.into_inner());
    let status = held.get_or_insert_with(|| Status { phase: "idle", ..Status::default() });
    update(status);
}

fn data_root() -> Result<PathBuf, String> {
    Platform::detect().paths.data_root().map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct Info {
    pub supported: bool,
    pub data_root: String,
    /// Where the files really are; differs from `data_root` once moved.
    pub stored_at: String,
    pub moved: bool,
    /// The last move, when it failed: why, for the page to say.
    pub last_failure: Option<String>,
}

#[tauri::command]
pub fn move_drive_info() -> Result<Info, String> {
    let root = data_root()?;
    let (at, moved) = relocate::location(&root);
    let last_failure = relocate::last_outcome(&root).filter(|o| !o.moved).map(|o| o.message);
    Ok(Info {
        supported: SUPPORTED,
        data_root: root.display().to_string(),
        stored_at: at.display().to_string(),
        moved,
        last_failure,
    })
}

#[derive(Debug, Serialize)]
pub struct Check {
    pub target: String,
    pub bytes: u64,
    pub files: u64,
}

/// Where the game would go, and how big it is, or why it cannot go there.
/// Async: measuring walks every file, and that must not hold the window.
#[tauri::command]
pub async fn move_drive_check(folder: String) -> Result<Check, String> {
    if !SUPPORTED {
        return Err(UNSUPPORTED.into());
    }
    let root = data_root()?;
    let plan = tauri::async_runtime::spawn_blocking(move || relocate::plan(&root, Path::new(&folder)))
        .await
        .map_err(|e| e.to_string())??;
    Ok(Check { target: plan.target.display().to_string(), bytes: plan.bytes, files: plan.files })
}

#[tauri::command]
pub fn move_drive_status() -> Status {
    STATUS.lock().unwrap_or_else(|p| p.into_inner()).clone().unwrap_or(Status { phase: "idle", ..Status::default() })
}

#[tauri::command]
pub fn move_drive_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// Start the move. Returns at once; the page polls [`move_drive_status`].
#[tauri::command]
pub fn move_drive_start(app: tauri::AppHandle, folder: String, confirm: bool) -> Result<(), String> {
    if !confirm {
        return Err("The move was not confirmed.".into());
    }
    if !SUPPORTED {
        return Err(UNSUPPORTED.into());
    }
    if crate::session_active() {
        return Err("Close the game first.".into());
    }
    if crate::content_updates::close_state() != "idle" {
        return Err("An update is downloading or installing. Let it finish, then move the game.".into());
    }
    if matches!(move_drive_status().phase, "copying" | "restarting") {
        return Err("The game is already being moved.".into());
    }
    // Held for the whole copy: no game launch and no settings write while
    // the files are being copied.
    let guard = crate::LaunchGuard::acquire()?;
    CANCEL.store(false, Ordering::SeqCst);
    set(|s| *s = Status { phase: "copying", target: folder.clone(), ..Status::default() });
    std::thread::spawn(move || {
        let _guard = guard;
        match run(&folder) {
            Ok(Finish::InPlace(message)) => set(|s| {
                s.phase = "moved";
                s.message = message;
            }),
            Ok(Finish::Restart) => {
                set(|s| s.phase = "restarting");
                // A moment for the page to say so, then close for the helper.
                std::thread::sleep(std::time::Duration::from_millis(1500));
                app.exit(0);
            }
            Err(why) => set(|s| {
                s.phase = "failed";
                s.message = why;
            }),
        }
    });
    Ok(())
}

enum Finish {
    /// Done in this process (Linux); the sentence to show.
    InPlace(String),
    /// The helper finishes once this launcher has closed (Windows).
    Restart,
}

fn run(folder: &str) -> Result<Finish, String> {
    let root = data_root()?;
    let plan = relocate::plan(&root, Path::new(folder))?;
    set(|s| {
        s.target = plan.target.display().to_string();
        s.total = plan.bytes;
    });
    relocate::copy(&plan, &|done, total| set(|s| { s.done = done; s.total = total; }), &CANCEL)?;
    if cfg!(target_os = "linux") {
        // The launcher guard is held, so no game, update or settings write
        // is running. This session's log stays in the old folder until it is
        // deleted; the next start logs into the new one.
        let (outcome, retired) = relocate::finish(&plan, None);
        if let Some(old) = retired {
            if let Err(e) = std::fs::remove_dir_all(&old) {
                tracing::warn!(folder = %old.display(), %e, "The old game folder could not be removed yet");
            }
        }
        return if outcome.moved { Ok(Finish::InPlace(outcome.message)) } else { Err(outcome.message) };
    }
    if let Err(why) = start_helper(&plan) {
        let _ = std::fs::remove_dir_all(&plan.staged);
        return Err(why);
    }
    Ok(Finish::Restart)
}

fn start_helper(plan: &Plan) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{HELPER_PREFIX}{stamp}"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("The temp folder could not be used: {e}"))?;
    let helper = dir.join(exe.file_name().unwrap_or_else(|| "reborn-launcher.exe".as_ref()));
    std::fs::copy(&exe, &helper).map_err(|e| format!("The move helper could not be prepared: {e}"))?;
    let plan_file = dir.join("plan.json");
    std::fs::write(&plan_file, serde_json::to_vec_pretty(plan).map_err(|e| e.to_string())?)
        .map_err(|e| format!("The move helper could not be prepared: {e}"))?;
    std::process::Command::new(&helper)
        .arg(FINISH_SWITCH)
        .arg(&plan_file)
        .arg(std::process::id().to_string())
        .arg(&exe)
        .current_dir(&dir)
        .spawn()
        .map_err(|e| format!("The move helper could not be started: {e}"))?;
    Ok(())
}

/// The helper: `--finish-move <plan.json> <launcher pid> <launcher exe>`.
/// Runs before anything else in `main`, touching nothing in the data root
/// until the launcher has closed. Always reopens the launcher, moved or not.
pub fn finish_from_args(args: &[String]) {
    let (Some(plan_file), pid, relaunch) = (args.first(), args.get(1).and_then(|p| p.parse::<u32>().ok()), args.get(2)) else {
        return;
    };
    let plan: Option<Plan> = std::fs::read(plan_file).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let retired = plan.map(|plan| relocate::finish(&plan, pid).1).unwrap_or(None);
    if let Some(exe) = relaunch {
        let _ = std::process::Command::new(exe).spawn();
    }
    if let Some(old) = retired {
        let _ = std::fs::remove_dir_all(old);
    }
}

#[derive(Debug, Serialize)]
pub struct InstallPlace {
    /// internal | media
    pub kind: &'static str,
    pub label: String,
    /// The folder to hand [`move_drive_check`].
    pub path: String,
    pub free_bytes: u64,
    /// The game is here now.
    pub current: bool,
}

/// Where the game can go, for the "Where should the game go?" step before
/// setup's download: internal storage and every SD card or USB drive. Empty
/// where moving is not offered, and on Linux when no card is mounted, so the
/// step only shows when there is a choice to make.
#[tauri::command]
pub fn install_places() -> Vec<InstallPlace> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let Ok(root) = data_root() else { return Vec::new() };
    let Some(home) = root.parent().map(Path::to_path_buf) else { return Vec::new() };
    let (at, _) = relocate::location(&root);
    let media = crate::window_picker::media_mounts();
    let on = |mount: &Path| at.starts_with(mount);
    let mut out: Vec<InstallPlace> = media
        .iter()
        .filter_map(|m| {
            let facts = reborn_core::platform::dir_move::volume_facts(m)?;
            let name = m.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            Some(InstallPlace { kind: "media", label: format!("SD card: {name}"), path: m.display().to_string(), free_bytes: facts.free_bytes, current: on(m) })
        })
        .collect();
    if out.is_empty() {
        return out;
    }
    let free = reborn_core::platform::dir_move::volume_facts(&home).map(|f| f.free_bytes).unwrap_or(0);
    let current = !out.iter().any(|p| p.current);
    out.insert(0, InstallPlace { kind: "internal", label: "Internal storage".into(), path: home.display().to_string(), free_bytes: free, current });
    out
}

/// The folder the game was moved to, when that drive is not connected (a
/// Steam Deck's SD card taken out). `None` otherwise. Every page asks, and
/// says so instead of failing to read its own files.
#[tauri::command]
pub fn moved_drive_missing() -> Option<String> {
    let root = data_root().ok()?;
    let target = std::fs::read_link(&root).ok()?;
    (!root.exists()).then(|| target.display().to_string())
}

/// At start: a card is mounted a moment after a Deck wakes or boots, so wait
/// for a missing one a little before the first page asks.
pub fn wait_for_moved_drive() {
    let started = std::time::Instant::now();
    while moved_drive_missing().is_some() && started.elapsed() < std::time::Duration::from_secs(8) {
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

/// At start, in the background: the old folder of a move that could not be
/// deleted at the time, and the temp copies of the helper.
pub fn clean_leftovers() {
    std::thread::spawn(|| {
        if let Ok(root) = data_root() {
            for old in relocate::leftovers(&root) {
                if let Err(e) = std::fs::remove_dir_all(&old) {
                    tracing::warn!(folder = %old.display(), %e, "An old game folder from a move could not be removed yet");
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with(HELPER_PREFIX) {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
    });
}
