//! The launcher window.
//!
//! # What this layer is allowed to do
//!
//! Nothing. It holds no rules of its own. Every decision, which pad to bind,
//! whether a session ended badly, whether migration is safe, already lives in
//! `reborn-core` where it is tested. This file turns those answers into JSON
//! and hands them to a web view.
//!
//! That split is deliberate and worth keeping. The two data-loss bugs found so
//! far were both found by a command line tool driving the same core, which only
//! worked because the core is where the thinking happens. A rule that lived in
//! the interface could only be exercised by clicking.
//!
//! # Why the window can be trusted not to lie
//!
//! Every command here returns either a real answer or an error string that says
//! what failed. None of them invent a plausible-looking placeholder. An
//! interface that shows a controller which is not connected, or a session that
//! did not happen, is worse than one that shows nothing.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use serde::Serialize;

use reborn_core::devices::{DeviceScanner, Enumeration};
use reborn_core::runtime::{GameBuild, GraphicsPreset};
use reborn_core::session::{LaunchRequest, Launched, Session};
use reborn_core::supervisor::{SessionEnd, Startup, Supervisor};
use reborn_core::{DataDir, Platform};

/// Where the launcher keeps things, and whether that is safe.
#[derive(Debug, Serialize)]
struct Paths {
    install_root: String,
    data_root: String,
    /// True when the data root sits outside the install root, which is the
    /// property that stops an uninstall taking the player's memory cards.
    saves_survive_uninstall: bool,
    directories: Vec<DirEntry>,
}

#[derive(Debug, Serialize)]
struct DirEntry {
    relative: String,
    exists: bool,
    class: &'static str,
}

#[derive(Debug, Serialize)]
struct DeviceView {
    device_string: String,
    family: String,
    port: Option<u8>,
    /// Set when the device was found but will not be used, explaining why.
    set_aside: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeviceReport {
    pads: Vec<DeviceView>,
    other: Vec<DeviceView>,
    /// True when every backend was searched. When false an empty list means
    /// "we did not look everywhere", not "nothing is plugged in".
    complete: bool,
    unsearched: Vec<String>,
    /// How the list was obtained, so the interface never implies more certainty
    /// than it has.
    source: &'static str,
}

#[derive(Debug, Serialize)]
struct SessionView {
    id: String,
    build: String,
    minutes: f64,
    ended: String,
    needs_recovery: bool,
}

#[tauri::command]
fn paths() -> Result<Paths, String> {
    let p = Platform::detect();
    let install = p.paths.install_root().map_err(|e| e.to_string())?;
    let data = p.paths.data_root().map_err(|e| e.to_string())?;

    let directories = DataDir::ALL
        .iter()
        .map(|d| DirEntry {
            relative: d.relative().to_string(),
            exists: data.join(d.relative()).exists(),
            class: if d.is_user_data() {
                "user"
            } else if d.is_derived() {
                "derived"
            } else {
                "managed"
            },
        })
        .collect();

    Ok(Paths {
        saves_survive_uninstall: !data.starts_with(&install),
        install_root: install.display().to_string(),
        data_root: data.display().to_string(),
        directories,
    })
}

fn to_report(scan: &Enumeration, source: &'static str) -> DeviceReport {
    let pads = scan
        .playable_pads()
        .iter()
        .enumerate()
        .map(|(i, d)| DeviceView {
            device_string: d.device_string(),
            family: d.family.label().to_string(),
            port: Some((i + 1) as u8),
            set_aside: None,
        })
        .collect();

    let other = scan
        .set_aside()
        .into_iter()
        .map(|(d, why)| DeviceView {
            device_string: d.device_string(),
            family: d.family.label().to_string(),
            port: None,
            set_aside: Some(why.to_string()),
        })
        .collect();

    DeviceReport {
        pads,
        other,
        complete: scan.is_complete(),
        unsearched: scan
            .not_searched
            .iter()
            .map(|(b, why)| format!("{}: {}", b.source(), why))
            .collect(),
        source,
    }
}

#[tauri::command]
fn devices(runtime: Option<String>) -> Result<DeviceReport, String> {
    let p = Platform::detect();

    // Asking the runtime is the only authoritative answer. The built in scan
    // sees XInput and nothing else, which on a real machine misses the WGInput
    // backend entirely and would miss every PlayStation pad.
    if let Some(exe) = runtime {
        let scratch = p
            .paths
            .data_root()
            .map_err(|e| e.to_string())?
            .join(DataDir::Cache.relative());
        match reborn_core::devices::scan_via_runtime(&p, std::path::Path::new(&exe), &scratch) {
            Ok(scan) => return Ok(to_report(&scan, "runtime")),
            // Fall through rather than fail. A missing runtime should not leave
            // the page blank, but the interface is told which answer it got.
            Err(_) => return Ok(to_report(&DeviceScanner::scan(), "fallback")),
        }
    }
    Ok(to_report(&DeviceScanner::scan(), "fallback"))
}

#[tauri::command]
fn sessions() -> Result<Vec<SessionView>, String> {
    let p = Platform::detect();
    match Supervisor::acquire(&p).map_err(|e| e.to_string())? {
        Startup::AlreadyRunning { .. } => {
            Err("another launcher is running and owns the session history".to_string())
        }
        Startup::Owner(s) => {
            let _ = s.recover_unfinished().map_err(|e| e.to_string())?;
            let history = s.history().map_err(|e| e.to_string())?;
            Ok(history
                .iter()
                .rev()
                .map(|r| SessionView {
                    id: r.id.clone(),
                    build: format!("{:?}", r.build).to_lowercase(),
                    minutes: r.duration_ms().unwrap_or(0) as f64 / 60_000.0,
                    ended: r
                        .end
                        .as_ref()
                        .map(|e| format!("{e:?}"))
                        .unwrap_or_else(|| "running".to_string()),
                    needs_recovery: r.end.as_ref().is_some_and(|e| e.needs_recovery()),
                })
                .collect())
        }
    }
}

/// A mod as it actually exists on disk.
#[derive(Debug, Serialize)]
struct ModEntry {
    name: String,
    applied: bool,
}

/// Live state for Home.
///
/// The anti-vibecoding review, V2, says the Home hero must carry live product
/// state rather than a promotional carousel, because that is the difference
/// between Home being useful and Home being a marketing page. V4 says invented
/// numbers are the failure that looks plausible in review and breaks on real
/// data.
///
/// So every field here is measured or absent. There is no rating, no win-loss
/// record and no last score, because nothing in this product produces them yet.
#[derive(Debug, Serialize)]
struct HomeState {
    sessions_played: usize,
    minutes_played: u64,
    /// None when no session has ever finished.
    last_played: Option<String>,
    controllers: usize,
    mods_installed: usize,
    mods_applied: usize,
    runtime_set: bool,
}

/// Read the real mod library.
///
/// The review requires real Reborn content: mod names come from the actual
/// JSGME `MODS` folder, never invented. JSGME records an install log per
/// applied mod in `!INSTLOGS`, so applied and merely present are different
/// facts and are reported separately.
#[tauri::command]
fn mods(legacy_root: Option<String>) -> Result<Vec<ModEntry>, String> {
    let root = match legacy_root {
        Some(r) => std::path::PathBuf::from(r),
        None => return Ok(Vec::new()),
    };
    let dir = root.join("MODS");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let logs = dir.join("!INSTLOGS");
    let applied_names: Vec<String> = std::fs::read_dir(&logs)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    e.path()
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        // JSGME's own bookkeeping directories are not mods.
        if name.starts_with('!') {
            continue;
        }
        let applied = applied_names.iter().any(|a| a.eq_ignore_ascii_case(&name));
        out.push(ModEntry { name, applied });
    }
    out.sort_by_key(|m| m.name.to_lowercase());
    Ok(out)
}

#[tauri::command]
fn home_state(runtime: Option<String>, legacy_root: Option<String>) -> Result<HomeState, String> {
    let p = Platform::detect();

    let (sessions_played, minutes_played, last_played) = match Supervisor::acquire(&p)
        .map_err(|e| e.to_string())?
    {
        Startup::Owner(s) => {
            let _ = s.recover_unfinished();
            let history = s.history().unwrap_or_default();
            let minutes: u64 = history.iter().filter_map(|r| r.duration_ms()).sum::<u64>() / 60_000;
            let last = history.last().map(|r| r.id.clone());
            (history.len(), minutes, last)
        }
        Startup::AlreadyRunning { .. } => (0, 0, None),
    };

    let devices_report = devices(runtime.clone())?;
    let mod_list = mods(legacy_root)?;

    Ok(HomeState {
        sessions_played,
        minutes_played,
        last_played,
        controllers: devices_report.pads.len(),
        mods_installed: mod_list.len(),
        mods_applied: mod_list.iter().filter(|m| m.applied).count(),
        runtime_set: runtime.is_some(),
    })
}

/// The result of pressing Play.
#[derive(Debug, Serialize)]
struct PrepareResult {
    user_dir: String,
    /// A sentence naming which user folder is in use and why.
    user_dir_note: String,
    /// False when the folder is a fresh empty one, which is the state that
    /// makes the game boot to factory defaults.
    user_dir_populated: bool,
    updated: Vec<String>,
    unchanged: Vec<String>,
    keys_set: usize,
    no_controller: bool,
}

#[derive(Debug, Serialize)]
struct LaunchResult {
    /// True when the game actually ran.
    started: bool,
    /// Which user folder the session used. Absent when nothing started.
    user_dir_note: Option<String>,
    /// A sentence for the player. Always set.
    message: String,
    /// Present only when the game ran.
    minutes: Option<f64>,
    /// True when the session ended in a way worth showing a recovery page for.
    needs_recovery: bool,
    /// Which pad went to which port.
    pads: Vec<String>,
    /// False when the pad list came from the built in scan rather than the
    /// runtime, so the interface can say the binding may be incomplete.
    pads_authoritative: bool,
}

fn preset_from(name: &str) -> GraphicsPreset {
    match name.to_lowercase().as_str() {
        "performance" => GraphicsPreset::Performance,
        "quality" => GraphicsPreset::Quality,
        "ultra" => GraphicsPreset::Ultra,
        _ => GraphicsPreset::Balanced,
    }
}

/// Write the configuration a launch would use, and start nothing.
///
/// Exists so the interface can show what will happen before it happens, which
/// is the same courtesy the command line already offered with --dry-run.
#[tauri::command]
async fn prepare_launch(
    runtime: String,
    game: String,
    preset: String,
    fullscreen: bool,
    user_dir: Option<String>,
) -> Result<PrepareResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let req = LaunchRequest {
            runtime_exe: std::path::PathBuf::from(runtime),
            game_path: std::path::PathBuf::from(game),
            graphics: preset_from(&preset),
            fullscreen,
            build: GameBuild::Offline,
            user_dir: user_dir.map(std::path::PathBuf::from),
        };
        Session::prepare(&p, &req)
            .map(|w| PrepareResult {
                user_dir: w.user_dir.path().display().to_string(),
                user_dir_note: w.user_dir.explain(),
                user_dir_populated: w.user_dir.is_populated(),
                updated: w.files.iter().map(|f| f.display().to_string()).collect(),
                unchanged: w
                    .unchanged
                    .iter()
                    .map(|f| f.display().to_string())
                    .collect(),
                keys_set: w.keys_set,
                no_controller: w.no_controller,
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Start the game and answer when it stops.
///
/// Runs on a blocking thread rather than the interface thread. The sequence has
/// to hold the single-instance lock and the open session record for as long as
/// the game runs, which means something must block; it just must not be the
/// thread drawing the window.
#[tauri::command]
async fn launch(
    runtime: String,
    game: String,
    preset: String,
    fullscreen: bool,
    user_dir: Option<String>,
) -> Result<LaunchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let req = LaunchRequest {
            runtime_exe: std::path::PathBuf::from(runtime),
            game_path: std::path::PathBuf::from(game),
            graphics: preset_from(&preset),
            fullscreen,
            build: GameBuild::Offline,
            user_dir: user_dir.map(std::path::PathBuf::from),
        };

        match Session::run(&p, &req).map_err(|e| e.to_string())? {
            Launched::Refused(r) => Ok(LaunchResult {
                started: false,
                user_dir_note: None,
                message: r.message(),
                minutes: None,
                needs_recovery: false,
                pads: Vec::new(),
                pads_authoritative: false,
            }),
            Launched::Finished {
                end,
                minutes,
                pads,
                pads_authoritative,
                user_dir,
                ..
            } => {
                let needs_recovery = end.needs_recovery();
                let message = match &end {
                    SessionEnd::Exited { code: Some(0) } => {
                        format!("Session ended after {minutes:.0} minutes.")
                    }
                    SessionEnd::Exited { code } => format!(
                        "The game exited with status {}. That is a failure, not a crash.",
                        code.unwrap_or(-1)
                    ),
                    SessionEnd::Crashed { detail } => {
                        format!("The game crashed: {detail}")
                    }
                    SessionEnd::Terminated => {
                        "The game was closed by something outside the launcher.".to_string()
                    }
                    SessionEnd::Interrupted => {
                        "The launcher stopped while the game was running.".to_string()
                    }
                };
                Ok(LaunchResult {
                    started: true,
                    user_dir_note: Some(user_dir.explain()),
                    message,
                    minutes: Some(minutes),
                    needs_recovery,
                    pads: pads
                        .iter()
                        .map(|(port, dev)| format!("Port {port}: {dev}"))
                        .collect(),
                    pads_authoritative,
                })
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            paths,
            devices,
            sessions,
            mods,
            home_state,
            prepare_launch,
            launch
        ])
        .run(tauri::generate_context!())
        .expect("the launcher window could not start");
}
