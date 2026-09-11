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

mod content_updates;
mod music_page;
mod setup_tools;
mod shell_tools;
mod stats_view;
mod dolphin_settings;

use std::path::{Path, PathBuf};

/// The version this launcher believes it is: the one `build.ps1 -Version`
/// stamped on the build, falling back to the crate version for a plain
/// `cargo build`. Every version comparison and every user agent reads this,
/// never `CARGO_PKG_VERSION` directly. The crate said 3.1.0 while every
/// beta was stamped 3.0.1-beta.N, so `update_check` called each newer beta
/// "up to date" and no launcher built after 2026-09-09 ever updated itself.
pub fn launcher_version() -> &'static str {
    match option_env!("REBORN_BUILD_VERSION") {
        Some(v) if !v.trim().is_empty() => v,
        _ => env!("CARGO_PKG_VERSION"),
    }
}

use serde::{Deserialize, Serialize};

use reborn_core::content::ContentService;
use reborn_core::controller::{ControlsProfile, DeviceFamily};
use reborn_core::devices::{DeviceScanner, Enumeration};
use reborn_core::gamepad::{
    action_for, ContextualSlot, Destructive, Direction, InputContext, NavAction, PadButton,
    RepeatTiming, Step,
};
use reborn_core::ini::IniDocument;
use reborn_core::inventory::ComponentId;
use reborn_core::library::{FileVerdict, RebuildState, Verifier, VerifyReport};
use reborn_core::manifest::Component;
use reborn_core::migration::{LegacyInstall, MigrationPlan};
use reborn_core::mods::{
    Activation, ComposeRequest, Composer, Composition, GameId, LinkMethod,
    ModId, ModPackage, PackageVersion,
};
use reborn_core::onboarding::{
    Advance, Back, Environment, Next, Onboarding, PrepTask, RecordedPreset, Satisfaction,
    Stage,
};
use reborn_core::runtime::{GameBuild, GraphicsPreset};
use reborn_core::session::{LaunchRequest, Launched, Session, UserDir};
use reborn_core::stats::{Enqueued, EntryState, QueueEntry, StatsQueue};
use reborn_core::supervisor::{SessionEnd, SessionIntent, Startup, Supervisor};
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
    /// "stable" or "beta". A beta has its own roots, so a beta under test
    /// never writes where the real launcher reads.
    channel: &'static str,
    channel_note: String,
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
    let channel = reborn_core::Channel::detect();
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
        channel: channel.channel.name(),
        channel_note: channel.explain(),
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
async fn devices(runtime: Option<String>) -> Result<DeviceReport, String> {
    tauri::async_runtime::spawn_blocking(move || read_devices(runtime))
        .await.map_err(|e| e.to_string())?
}

fn read_devices(runtime: Option<String>) -> Result<DeviceReport, String> {
    let p = Platform::detect();

    // Asking the runtime is the only authoritative answer. The built in scan
    // sees XInput and nothing else, which on a real machine misses the WGInput
    // backend entirely and would miss every PlayStation pad.
    if let Some(exe) = runtime {
        let scratch = p
            .paths
            .data_root()
            .map_err(|e| e.player_message().to_string())?
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
    /// When the last session ended, milliseconds since the epoch.
    last_played_ms: Option<u64>,
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
        // JSGME names its logs "<Mod> install.log", so the stem is
        // "<Mod> install" and comparing it to the folder name never matched.
        // This reported 0 of 11 applied on a machine with 4 applied, which is
        // worse than no answer: it made a dangerous state look clean.
        let applied = applied_names.iter().any(|a| {
            let trimmed = a
                .strip_suffix(" install")
                .or_else(|| a.strip_suffix(" Install"))
                .unwrap_or(a);
            trimmed.eq_ignore_ascii_case(&name) || a.eq_ignore_ascii_case(&name)
        });
        out.push(ModEntry { name, applied });
    }
    out.sort_by_key(|m| m.name.to_lowercase());
    Ok(out)
}

#[tauri::command]
async fn home_state(runtime: Option<String>, legacy_root: Option<String>) -> Result<HomeState, String> {
    tauri::async_runtime::spawn_blocking(move || read_home_state(runtime, legacy_root))
        .await.map_err(|e| e.to_string())?
}

fn read_home_state(runtime: Option<String>, legacy_root: Option<String>) -> Result<HomeState, String> {
    let p = Platform::detect();

    let (sessions_played, minutes_played, last_played, last_played_ms) = match Supervisor::acquire(&p)
        .map_err(|e| e.to_string())?
    {
        Startup::Owner(s) => {
            let _ = s.recover_unfinished();
            let history = s.history().unwrap_or_default();
            let minutes: u64 = history.iter().filter_map(|r| r.duration_ms()).sum::<u64>() / 60_000;
            let last = history.last().map(|r| r.id.clone());
            let last_ms = history.last().and_then(|r| r.ended_ms);
            (history.len(), minutes, last, last_ms)
        }
        Startup::AlreadyRunning { .. } => (0, 0, None, None),
    };

    let devices_report = to_report(&DeviceScanner::scan(), "fallback");
    let mod_list = mods(legacy_root)?;

    Ok(HomeState {
        sessions_played,
        minutes_played,
        last_played,
        last_played_ms,
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
    /// What the game was pointed at, mods-wise. Absent when nothing started.
    /// Present and naming the problem when the mods could not be applied,
    /// because that never stops a launch and must not be silent either.
    mods_note: Option<String>,
    /// The session the record was filed under. Absent when nothing started.
    session_id: Option<String>,
    /// What happened to the match results Dolphin wrote. Absent when nothing
    /// started or when there were none, which is most sessions.
    stats_note: Option<String>,
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
    runtime: Option<String>,
    game: Option<String>,
    preset: String,
    fullscreen: bool,
    user_dir: Option<String>,
) -> Result<PrepareResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let legacy = default_legacy_root(&p);
        let (found, game_path) = find_runtime_and_game(
            &p,
            runtime.as_deref(),
            game.as_deref(),
            legacy.as_deref(),
        )?;
        let req = LaunchRequest {
            runtime_exe: found.path().to_path_buf(),
            game_path,
            graphics: preset_from(&preset),
            fullscreen,
            build: GameBuild::Offline,
            mods_disabled: false,
            user_dir: selected_user_dir(&p, user_dir.as_deref())?,
            league_match: false,
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

/// The runtime and game a launch should use, found rather than trusted.
///
/// Both arguments are HINTS. A path that does not exist is not an error and not
/// obeyed: the launcher searches for the real thing and says where it came
/// from. That matters because the interface remembers what it was last told,
/// and a remembered path outlives the folder it named. A stale box refused
/// every launch with "Set the runtime in Settings" while the emulator sat
/// beside the launcher and inside the player's own install.
///
/// On Windows there is one right answer anyway, so being asked at all is the
/// oddity; see reborn_core::session::resolve_runtime.
fn find_runtime_and_game(
    platform: &Platform,
    runtime_hint: Option<&str>,
    game_hint: Option<&str>,
    legacy_root: Option<&Path>,
) -> Result<(reborn_core::session::Runtime, PathBuf), String> {
    let saved = saved_paths(platform)?;
    find_runtime_and_game_for(platform, runtime_hint, game_hint, legacy_root, saved.user_dir.as_deref())
}

fn find_runtime_and_game_for(
    platform: &Platform,
    runtime_hint: Option<&str>,
    game_hint: Option<&str>,
    legacy_root: Option<&Path>,
    user_dir: Option<&Path>,
) -> Result<(reborn_core::session::Runtime, PathBuf), String> {
    let saved = saved_paths(platform)?;
    let runtime_hint = runtime_hint.or_else(|| saved.runtime_exe.as_deref().and_then(Path::to_str));
    let game_hint = game_hint.or_else(|| saved.game_path.as_deref().and_then(Path::to_str));
    // Cleaned first. Windows Explorer's Copy as path wraps the path in quotes
    // and the quotes are pasted with it, which produces a path that looks
    // perfect in every message and matches nothing. See session::user_path.
    let cleaned_runtime = runtime_hint.map(reborn_core::session::user_path);

    // An existing hint is honoured as a deliberate choice. A hint that names
    // nothing is dropped, so the search is not poisoned by a dead path.
    let live_hint = cleaned_runtime.as_deref().filter(|path| path.is_file());

    let found = reborn_core::session::resolve_runtime(platform, live_hint, legacy_root)
        .map_err(|looked| reborn_core::session::no_runtime_message(&looked))?;

    let game = match game_hint
        .map(reborn_core::session::user_path)
        .filter(|path| path.is_file())
    {
        Some(path) => path,
        None => {
            // The folder this emulator will boot against decides which
            // Dolphin.ini's recorded default counts, so it is resolved here
            // the way a launch resolves it.
            let user = reborn_core::session::resolve_user_dir_for(platform, user_dir, Some(found.path()))
                .map_err(|e| e.player_message().to_string())?;
            let launcher_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
            let search = reborn_core::session::find_game_image(
                found.path(),
                user.path(),
                legacy_root,
                launcher_dir.as_deref(),
            );
            match search.path() {
                Some(path) => path.to_path_buf(),
                None => return Err(search.explain()),
            }
        }
    };

    Ok((found, game))
}

/// The emulator a hint resolves to, or none. Tolerant: a page that only
/// wants to know which user folder the mods should link must not fail
/// because no emulator was found; it falls back to the global search.
fn runtime_for(platform: &Platform, hint: Option<&str>) -> Option<PathBuf> {
    let cleaned = hint.map(reborn_core::session::user_path);
    let live = cleaned.as_deref().filter(|p| p.is_file());
    let legacy = default_legacy_root(platform);
    reborn_core::session::resolve_runtime(platform, live, legacy.as_deref())
        .ok()
        .map(|r| r.path().to_path_buf())
}

#[derive(Debug, Serialize)]
struct LaunchCheck {
    /// True when both the emulator and the image were found.
    ready: bool,
    /// The emulator a launch would start, and where it came from.
    runtime: Option<String>,
    runtime_note: Option<String>,
    /// "bundled", "legacy_install", or "chosen". Absent when none was found.
    runtime_kind: Option<&'static str>,
    game: Option<String>,
    /// Why THIS image, in a sentence, so a wrong one is caught before the
    /// menu comes up. Absent when the image was given rather than found.
    game_note: Option<String>,
    /// Which user folder the launch would boot against, and why.
    user_dir_note: Option<String>,
    /// Why a launch would refuse, in a sentence. Absent when ready.
    problem: Option<String>,
}

/// What a launch would use, without starting anything.
///
/// The same search a launch makes, so what this page says will be used is
/// what the Play button uses. Read only: nothing is written and nothing is
/// started. Exists because the only other way to learn which emulator the
/// launcher found was to launch.
#[tauri::command]
async fn launch_check(runtime: Option<String>, game: Option<String>, user_dir: Option<String>) -> Result<LaunchCheck, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let legacy = default_legacy_root(&p);
        let chosen_user = selected_user_dir(&p, user_dir.as_deref())?;
        match find_runtime_and_game_for(&p, runtime.as_deref(), game.as_deref(), legacy.as_deref(), chosen_user.as_deref()) {
            Ok((found, image)) => Ok(LaunchCheck {
                user_dir_note: reborn_core::session::resolve_user_dir_for(&p, chosen_user.as_deref(), Some(found.path()))
                    .ok()
                    .map(|d| d.explain()),
                game_note: {
                    let saved_game = reborn_core::settings::Settings::load(&reborn_core::settings::settings_path(&p).map_err(|e| e.player_message().to_string())?).map_err(|e| e.player_message().to_string())?.game_path;
                    let given = game.as_deref().map(reborn_core::session::user_path).or(saved_game).filter(|g| g.is_file());
                    if given.is_some() {
                        None
                    } else {
                        let user = reborn_core::session::resolve_user_dir_for(&p, chosen_user.as_deref(), Some(found.path())).ok();
                        let launcher_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
                        user.map(|u| {
                            reborn_core::session::find_game_image(found.path(), u.path(), legacy.as_deref(), launcher_dir.as_deref())
                                .explain()
                        })
                    }
                },
                ready: true,
                runtime: Some(found.path().display().to_string()),
                runtime_note: Some(found.explain()),
                runtime_kind: Some(match found {
                    reborn_core::session::Runtime::Bundled(_) => "bundled",
                    reborn_core::session::Runtime::FromLegacyInstall(_) => "legacy_install",
                    reborn_core::session::Runtime::Chosen(_) => "chosen",
                }),
                game: Some(image.display().to_string()),
                problem: None,
            }),
            Err(problem) => Ok(LaunchCheck {
                game_note: None,
                user_dir_note: None,
                ready: false,
                runtime: None,
                runtime_note: None,
                runtime_kind: None,
                game: None,
                problem: Some(problem),
            }),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The default 2.1 install to search, when the caller named none.
///
/// The launcher's own data root sits beside the 2.1 one, so its parent is where
/// an existing install would be. Guessed rather than required, because a launch
/// should not need a player to tell the launcher where their own game is.
fn default_legacy_root(platform: &Platform) -> Option<PathBuf> {
    if let Ok(file) = reborn_core::settings::settings_path(platform) {
        if let Ok(settings) = reborn_core::settings::Settings::load(&file) {
            if let Some(root) = settings.legacy_root { return Some(root); }
        }
    }
    let guess = platform
        .paths
        .data_root()
        .ok()?
        .parent()?
        .join("NFL Street Reborn");
    guess.is_dir().then_some(guess)
}

static LAUNCH_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct LaunchGuard;
impl LaunchGuard {
    fn acquire() -> Result<Self, String> {
        LAUNCH_ACTIVE.compare_exchange(false, true, std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst).map_err(|_| "A game session is already active.".to_string())?;
        Ok(Self)
    }
}
impl Drop for LaunchGuard {
    fn drop(&mut self) { LAUNCH_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst); }
}
#[tauri::command]
fn session_active() -> bool { LAUNCH_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) }

/// Start the game and answer when it stops.
///
/// Runs on a blocking thread rather than the interface thread. The sequence has
/// to hold the single-instance lock and the open session record for as long as
/// the game runs, which means something must block; it just must not be the
/// thread drawing the window.
#[tauri::command]
async fn launch(
    runtime: Option<String>,
    game: Option<String>,
    preset: String,
    fullscreen: bool,
    user_dir: Option<String>,
    mods_disabled: bool,
    league_match: Option<bool>,
) -> Result<LaunchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = LaunchGuard::acquire()?;
        let p = Platform::detect();
        let legacy = default_legacy_root(&p);
        let chosen_user = selected_user_dir(&p, user_dir.as_deref())?;
        let (found, game_path) = find_runtime_and_game_for(
            &p,
            runtime.as_deref(),
            game.as_deref(),
            legacy.as_deref(),
            chosen_user.as_deref(),
        )?;
        let req = LaunchRequest {
            runtime_exe: found.path().to_path_buf(),
            game_path,
            graphics: preset_from(&preset),
            fullscreen,
            build: GameBuild::Offline,
            mods_disabled,
            user_dir: chosen_user,
            league_match: league_match.unwrap_or(false),
        };

        let launched = Session::run(&p, &req).map_err(|e| e.player_message().to_string())?;

        // The guard, back, before anything reaches the network. `Session::run`
        // has already given the single-instance lock back; this is the other
        // half. A send is one request per queued game with the client's full
        // timeout each, so a backlog against a slow or unreachable backend is
        // minutes, and for all of those minutes the emulator has already
        // exited while Play answers "A game session is already active", the
        // Online page says to close the emulator, opening the emulator from
        // Settings is refused and the command line retry is refused.
        //
        // Nothing is at risk in the gap: the captures are already in the queue
        // on disk, and a second launch starting here would collect an empty
        // folder and send the same queue, which is the send this one was going
        // to do.
        drop(_guard);

        match launched {
            Launched::Refused(r) => Ok(LaunchResult {
                started: false,
                user_dir_note: None,
                message: r.message(),
                minutes: None,
                needs_recovery: false,
                pads: Vec::new(),
                pads_authoritative: false,
                mods_note: None,
                session_id: None,
                stats_note: None,
            }),
            Launched::Finished {
                end,
                minutes,
                pads,
                pads_authoritative,
                user_dir,
                mods,
                session_id,
                results,
                ..
            } => {
                // With the lock and the guard both back. Never surfaced as a
                // failure: the game was played whatever the backend answered.
                let stats = results.send(&p);
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
                    mods_note: Some(mods.explain()),
                    session_id: Some(session_id),
                    stats_note: stats.explain(),
                })
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Confirmation
// ---------------------------------------------------------------------------

/// Refuse a command that changes something when the caller has not said yes.
///
/// Every command below that deletes, copies, moves or uploads takes a `confirm`
/// flag and calls this first, before it acquires a lock, builds a plan or opens
/// a directory. Checking late is the failure this shape prevents: a rebuild that
/// plans, deletes the composed tree and then notices it was never confirmed has
/// already destroyed the thing it was asking about.
///
/// The message names the flag, because a caller that is told "not confirmed" and
/// cannot tell what to send next will send something else.
fn confirmed(confirm: bool, command: &str, what: &str) -> Result<(), String> {
    if confirm {
        return Ok(());
    }
    Err(format!(
        "{command} would {what}. Nothing was changed. Show the plan, get the \
         player's answer, then call {command} again with confirm set to true."
    ))
}

/// The one sentence for "a game is running, so this cannot happen now".
///
/// Shared, so the several commands that need the single-instance lock cannot
/// drift into describing the same state three different ways.
fn already_running() -> String {
    "Another launcher is running and owns this install. Close it, or wait for \
     the session to end, then try again."
        .to_string()
}

// ---------------------------------------------------------------------------
// The resolved user directory
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct UserDirView {
    path: String,
    /// `chosen`, `existing` or `fresh`. The interface needs the difference:
    /// `fresh` is the state that boots the game to factory defaults with no
    /// memory cards visible.
    kind: &'static str,
    populated: bool,
    /// The core's own sentence, not a re-worded one, so the window and the
    /// command line say the same thing about the same folder.
    note: String,
}

fn user_dir_view(dir: &UserDir) -> UserDirView {
    UserDirView {
        path: dir.path().display().to_string(),
        kind: match dir {
            UserDir::Chosen(_) => "chosen",
            UserDir::Portable(_) => "portable",
            UserDir::Existing(_) => "existing",
            UserDir::Fresh(_) => "fresh",
        },
        populated: dir.is_populated(),
        note: dir.explain(),
    }
}

/// Which Dolphin user directory a launch would use, without launching.
///
/// Separate from `prepare_launch`, which writes files. A Settings page that
/// wants to show the player which folder is in use must not have to write six
/// INIs to find out.
///
/// Blocking because resolution reads several directories off disk, and on this
/// machine the real one holds 35,015 files.
#[tauri::command]
async fn user_dir(chosen: Option<String>, runtime: Option<String>) -> Result<UserDirView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let picked = chosen.as_deref().map(reborn_core::session::user_path);
        let exe = runtime_for(&p, runtime.as_deref());
        reborn_core::session::resolve_user_dir_for(&p, picked.as_deref(), exe.as_deref())
            .map(|d| user_dir_view(&d))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Mod packages and composition
// ---------------------------------------------------------------------------

/// A package the caller wants in a composition, named the way it is stored.
///
/// Two fields rather than one `id@version` string, because a split that guesses
/// the separator wrong resolves to a directory that does not exist, and the
/// resulting "package not installed" would be this layer's fault rather than
/// the library's state.
#[derive(Debug, Clone, Deserialize)]
struct PackageRef {
    id: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct ActivePackageView {
    id: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct PackageEntryView {
    id: String,
    version: String,
    name: String,
    targets: Vec<String>,
    /// Absent means the package declares no conceptual exclusivity. It is not a
    /// group called "none": this field drives a pick-one interface only, and an
    /// invented group would make two unrelated mods refuse each other.
    conflict_group: Option<String>,
    files: usize,
    bytes: u64,
    /// True when `mods/activation.json` lists this exact id and version.
    active: bool,
    /// The manifest's `kind`, when it declares one. "soundtrack" packages
    /// carry a music pack instead of textures: switching one rebuilds the
    /// disc rather than composing a tree.
    kind: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExclusiveGroupView {
    members: Vec<String>,
    /// A path more than one of them writes, so the interface can say WHY these
    /// are exclusive rather than just asserting it.
    example_path: Option<String>,
    /// True when they overlap on disk, which the composer refuses. False when
    /// they only declare the same group, which is advice and composes fine.
    computed: bool,
}

#[derive(Debug, Serialize)]
struct ModLibraryView {
    packages_root: String,
    packages: Vec<PackageEntryView>,
    /// Directories that look like packages and could not be read, with the
    /// reason. Reported rather than skipped: a mod that silently vanishes from
    /// the list is indistinguishable from a mod that was never installed.
    unreadable: Vec<String>,
    /// What `activation.json` says is on, whether or not the payload is still
    /// installed. Kept beside `packages` so a stale activation entry is visible
    /// instead of being quietly dropped.
    activation: Vec<ActivePackageView>,
    /// Sets of packages that cannot be on together, derived from the file
    /// lists rather than a curated table.
    ///
    /// This is what a pick-one control needs. The seven recoloured footballs all
    /// write one texture path, so showing them as independently switchable
    /// would be untrue: turning on two is refused, and before it was refused
    /// JSGME kept whichever landed last without saying so.
    exclusive: Vec<ExclusiveGroupView>,
    /// Packages the controls profile owns, so a mods list does not offer them
    /// twice. Turning one of these on by hand would fight the controls setting.
    managed_by_controls: Vec<String>,
}

/// The manifest's `kind` field, read raw so composition's typed view need
/// not know about it. Absent for every texture package.
fn package_kind(dir: &Path) -> Option<String> {
    let raw = std::fs::read(dir.join(reborn_core::mods::PACKAGE_MANIFEST)).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    v.get("kind").and_then(|k| k.as_str()).map(str::to_string)
}

/// Soundtrack packages under `mods/packages`: schema `nsr.soundtrack-mod.v1`,
/// one `.nsrmusic` (or an empty pack.json for the original soundtrack) instead
/// of textures. The composition loader refuses them by design, so they are
/// read raw here, listed beside the texture packages, and switched through
/// the soundtrack record rather than activation.json. Returns the entries and
/// the `unreadable` list with their loader complaints removed.
fn read_soundtrack_packages(root: &Path, unreadable: Vec<String>, data_root: &Path) -> (Vec<PackageEntryView>, Vec<String>) {
    use reborn_core::soundtrack::state::{Base, Soundtrack};
    let record = Soundtrack::load(data_root).ok();
    let mut found = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    let Ok(ids) = std::fs::read_dir(root) else { return (found, unreadable) };
    for id_entry in ids.flatten() {
        let Ok(versions) = std::fs::read_dir(id_entry.path()) else { continue };
        for version_entry in versions.flatten() {
            let dir = version_entry.path();
            let Ok(raw) = std::fs::read(dir.join(reborn_core::mods::PACKAGE_MANIFEST)) else { continue };
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&raw) else { continue };
            if v.get("schema").and_then(|x| x.as_str()) != Some("nsr.soundtrack-mod.v1") {
                continue;
            }
            let get = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (id, version) = (get("id"), get("version"));
            if id.is_empty() || version.is_empty() {
                continue;
            }
            let files = v.get("files").and_then(|f| f.as_array()).cloned().unwrap_or_default();
            let bytes: u64 = files.iter().filter_map(|f| f.get("bytes").and_then(|b| b.as_u64())).sum();
            let has_songs = files.iter().any(|f| f.get("path").and_then(|p| p.as_str()).map(|p| p.ends_with(".nsrmusic")).unwrap_or(false));
            // Active means "this is the soundtrack base": the package with a
            // pack when the record names it, the original when the base is stock.
            let active = match record.as_ref().map(|r| &r.base) {
                Some(Base::Package { id: bid, version: bver, .. }) => *bid == id && *bver == version,
                Some(Base::Stock) => !has_songs,
                None => !has_songs,
            };
            found.push(PackageEntryView {
                id,
                version,
                name: get("name"),
                targets: Vec::new(),
                conflict_group: Some("soundtrack".to_string()),
                files: files.len(),
                bytes,
                active,
                kind: Some("soundtrack".to_string()),
            });
            dirs.push(dir);
        }
    }
    found.sort_by(|a, b| (a.id.as_str(), a.version.as_str()).cmp(&(b.id.as_str(), b.version.as_str())));
    let unreadable = unreadable
        .into_iter()
        .filter(|line| !dirs.iter().any(|d| line.starts_with(&d.display().to_string())))
        .collect();
    (found, unreadable)
}

/// A soundtrack package switched on or off: set the soundtrack base and
/// rebuild the disc now, under the launch guard. Returns the sentence to show.
///
/// A package whose pack carries no songs (the "original" package) sets the
/// base to stock, and so does switching any soundtrack package off.
fn soundtrack_switch(data_root: &Path, dir: &Path, id: &str, version: &str, on: bool) -> Result<String, String> {
    use reborn_core::soundtrack::state::{Base, Soundtrack};
    let mut st = Soundtrack::load(data_root).map_err(|e| e.to_string())?;
    let mut base = Base::Stock;
    if on {
        let raw = std::fs::read(dir.join(reborn_core::mods::PACKAGE_MANIFEST)).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        let file = v.get("files").and_then(|f| f.as_array()).and_then(|a| a.first()).and_then(|f| f.get("path")).and_then(|p| p.as_str()).unwrap_or("");
        // Only a .nsrmusic payload is a pack; the original package carries a
        // bare pack.json with no songs, which means the disc's own soundtrack.
        if file.ends_with(".nsrmusic") {
            let pack_path = dir.join("files").join(file);
            match reborn_core::soundtrack::pack::read_pack(&pack_path) {
                Ok(p) if !p.manifest.songs.is_empty() => {
                    let rel = pack_path.strip_prefix(data_root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| pack_path.to_string_lossy().into_owned());
                    base = Base::Package { id: id.to_string(), version: version.to_string(), pack: rel };
                }
                Ok(_) => {}
                Err(e) => return Err(format!("{id} could not be read as a soundtrack pack: {e}")),
            }
        }
    }
    st.base = base;
    if on {
        st.enabled = true;
    }
    st.save(data_root).map_err(|e| e.to_string())?;
    let outcome = music_page::rebuild_now(data_root, &mut |_| {})?;
    Ok(match outcome {
        reborn_core::soundtrack::apply::Outcome::Refused(reborn_core::soundtrack::Refusal::NothingStaged) => "Your game plays the original soundtrack.".to_string(),
        other => other.note(),
    })
}

/// Enumerate `mods/packages/<id>/<version>/package.json`.
///
/// Enumeration only. This makes no choice about which version of a package
/// wins, because no rule for that exists anywhere in the core yet, and
/// inventing one here would put it in the layer with no tests. Every version
/// found is reported and the caller names the one it wants.
fn read_package_library(root: &Path) -> (Vec<ModPackage>, Vec<String>) {
    let mut found = Vec::new();
    let mut unreadable = Vec::new();

    let ids = match std::fs::read_dir(root) {
        Ok(rd) => rd,
        // A launcher that has never installed a package has no directory here,
        // and that is the normal state of a fresh install rather than an error.
        Err(_) => return (found, unreadable),
    };
    for id_entry in ids.flatten() {
        if !id_entry.path().is_dir() {
            continue;
        }
        let versions = match std::fs::read_dir(id_entry.path()) {
            Ok(rd) => rd,
            Err(e) => {
                unreadable.push(format!("{}: {e}", id_entry.path().display()));
                continue;
            }
        };
        for version_entry in versions.flatten() {
            let dir = version_entry.path();
            if !dir.is_dir() {
                continue;
            }
            if !dir.join(reborn_core::mods::PACKAGE_MANIFEST).exists() {
                continue;
            }
            match ModPackage::load(&dir) {
                Ok(package) => found.push(package),
                Err(e) => unreadable.push(format!("{}: {e}", dir.display())),
            }
        }
    }
    found.sort_by(|a, b| {
        (a.id.as_str(), a.version.as_str()).cmp(&(b.id.as_str(), b.version.as_str()))
    });
    (found, unreadable)
}

/// The launcher's own mod library, and which packages are switched on.
///
/// This is not the same list as the `mods` command above. That one reads the
/// player's JSGME `MODS` folder, which is 11 directories of loose files with no
/// manifest, no id and no version. This one reads validated packages under the
/// launcher's data root, which is the only thing composition can act on. Both
/// are real and they are different sets, so they are two commands rather than
/// one that merges them and loses which is which.
#[tauri::command]
async fn mod_packages() -> Result<ModLibraryView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let root = reborn_core::mods::packages_root(&p).map_err(|e| e.player_message().to_string())?;
        let activation_path =
            reborn_core::mods::activation_path(&p).map_err(|e| e.player_message().to_string())?;
        let activation =
            Activation::load(&activation_path).map_err(|e| e.player_message().to_string())?;

        let (packages, unreadable) = read_package_library(&root);
        let data_root = p.paths.data_root().map_err(|e| e.player_message().to_string())?;
        let (soundtracks, unreadable) = read_soundtrack_packages(&root, unreadable, &data_root);

        // Per game, because exclusivity is a fact about a composed tree and two
        // packages that collide on one build may not both target another. Every
        // build any installed package targets, so nothing is missed.
        let mut builds: Vec<GameId> = Vec::new();
        for pkg in &packages {
            for game in &pkg.targets {
                if !builds.contains(game) {
                    builds.push(game.clone());
                }
            }
        }
        let mut exclusive: Vec<ExclusiveGroupView> = Vec::new();
        if soundtracks.len() > 1 {
            exclusive.push(ExclusiveGroupView {
                members: soundtracks.iter().map(|s| s.id.clone()).collect(),
                example_path: None,
                computed: false,
            });
        }
        for game in &builds {
            for group in reborn_core::mods::exclusive_groups(&packages, game) {
                let members: Vec<String> = group.members.iter().map(|m| m.to_string()).collect();
                // One build's group can repeat another's. Reported once.
                if exclusive.iter().any(|g| g.members == members) {
                    continue;
                }
                exclusive.push(ExclusiveGroupView {
                    members,
                    example_path: group.example_path,
                    computed: group.computed,
                });
            }
        }

        // The prompt packs belong to the controls profile now, so a mods list
        // must not offer them as ordinary switches. Only the ones actually
        // installed are named, so this is a fact about the library rather than
        // a list of ids that may not exist.
        let managed_by_controls: Vec<String> = ControlsProfile::ALL
            .iter()
            .filter_map(|p| p.package_id())
            .filter(|id| packages.iter().any(|pkg| pkg.id.as_str() == *id))
            .map(|id| id.to_string())
            .collect();

        Ok(ModLibraryView {
            packages_root: root.display().to_string(),
            packages: packages
                .iter()
                .map(|pkg| PackageEntryView {
                    id: pkg.id.to_string(),
                    version: pkg.version.to_string(),
                    name: pkg.name.clone(),
                    targets: pkg.targets.iter().map(|g| g.to_string()).collect(),
                    conflict_group: pkg.conflict_group.clone(),
                    files: pkg.files.len(),
                    bytes: pkg.files.iter().map(|f| f.bytes).sum(),
                    // Both halves, because an id that is active at a different
                    // version is a different thing on disk from this one.
                    active: activation
                        .active
                        .iter()
                        .any(|a| a.id == pkg.id && a.version == pkg.version),
                    kind: package_kind(&root.join(pkg.id.as_str()).join(pkg.version.as_str())),
                })
                .chain(soundtracks)
                .collect(),
            unreadable,
            activation: activation
                .active
                .iter()
                .map(|a| ActivePackageView {
                    id: a.id.to_string(),
                    version: a.version.to_string(),
                })
                .collect(),
            exclusive,
            managed_by_controls,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct ModPlanView {
    game: String,
    /// False when the core refused. `refusal` then carries the sentence.
    planned: bool,
    refusal: Option<String>,
    /// Absent when there is no plan, rather than an empty string: a blank path
    /// renders as a real directory that happens to have no name.
    pristine_root: Option<String>,
    composed_root: Option<String>,
    /// `hardlink` or `copy`, absent with no plan. The difference is whether the
    /// composed tree costs its own bytes.
    method: Option<&'static str>,
    total_entries: usize,
    overlay_entries: usize,
    pristine_entries: usize,
    total_bytes: u64,
    activation: Vec<ActivePackageView>,
    /// Packages asked for that have no payload for this build. A mod switched
    /// on that quietly does nothing looks exactly like a broken mod.
    not_targeted: Vec<String>,
    /// Declared-group conflicts, which do not stop composition. Carried so they
    /// cannot be silently dropped.
    soft_conflicts: Vec<String>,
}

fn link_method_label(method: LinkMethod) -> &'static str {
    match method {
        LinkMethod::Hardlink => "hardlink",
        LinkMethod::Copy => "copy",
    }
}

fn plan_view(game: &GameId, composition: &Composition) -> ModPlanView {
    let mut view = ModPlanView {
        game: game.to_string(),
        planned: false,
        refusal: composition.refusal().map(|r| r.message()),
        pristine_root: None,
        composed_root: None,
        method: None,
        total_entries: 0,
        overlay_entries: 0,
        pristine_entries: 0,
        total_bytes: 0,
        activation: Vec::new(),
        not_targeted: Vec::new(),
        soft_conflicts: Vec::new(),
    };
    if let Some(plan) = composition.plan() {
        view.planned = true;
        view.pristine_root = Some(plan.pristine_root.display().to_string());
        view.composed_root = Some(plan.composed_root.display().to_string());
        view.method = Some(link_method_label(plan.method));
        view.total_entries = plan.entries.len();
        view.overlay_entries = plan.overlay_count();
        view.pristine_entries = plan.pristine_count();
        view.total_bytes = plan.total_bytes();
        view.activation = plan
            .activation
            .iter()
            .map(|a| ActivePackageView {
                id: a.id.to_string(),
                version: a.version.to_string(),
            })
            .collect();
        view.not_targeted = plan.not_targeted.iter().map(|i| i.to_string()).collect();
        view.soft_conflicts = plan.soft_conflicts.iter().map(|c| c.message()).collect();
    }
    view
}

/// Load the exact packages the caller named.
///
/// A missing one is an error naming the directory, not a shorter list. Silently
/// composing three of four requested mods produces a tree the player did not
/// ask for and cannot tell apart from one that worked.
fn load_packages(root: &Path, wanted: &[PackageRef]) -> Result<Vec<ModPackage>, String> {
    let mut out = Vec::new();
    for want in wanted {
        let id = ModId::parse(&want.id).map_err(|e| e.to_string())?;
        let version = PackageVersion::parse(&want.version).map_err(|e| e.to_string())?;
        let dir = root.join(id.as_str()).join(version.as_str());
        let package = ModPackage::load(&dir).map_err(|e| {
            format!(
                "{} could not be read as an installed package: {e} Re-import it, \
                 or leave it out of the list.",
                dir.display()
            )
        })?;
        out.push(package);
    }
    Ok(out)
}

/// Build a composition request out of what the caller named.
fn compose_request(
    platform: &Platform,
    game: &str,
    packages: &[PackageRef],
) -> Result<(GameId, ComposeRequest), String> {
    let game = GameId::parse(game).map_err(|e| e.to_string())?;
    let root = reborn_core::mods::packages_root(platform).map_err(|e| e.to_string())?;
    let loaded = load_packages(&root, packages)?;
    let req =
        ComposeRequest::in_data_root(platform, game.clone(), loaded).map_err(|e| e.to_string())?;
    Ok((game, req))
}

#[derive(Debug, Serialize)]
struct ModSyncView {
    game: String,
    /// False when the plan refused, in which case nothing on disk changed and
    /// the game keeps loading what it loaded before.
    synced: bool,
    refusal: Option<String>,
    /// One line for a player.
    note: String,
    /// What activation.json listed, in order.
    active: Vec<ActivePackageView>,
    /// Listed but not installed, so left out of the tree and left listed.
    missing: Vec<ActivePackageView>,
    /// "already_current" or "rebuilt". Absent when refused.
    tree: Option<&'static str>,
    entries: u64,
    overlaid: u64,
    /// "linked" or "already_linked". Absent when refused.
    link: Option<&'static str>,
    link_at: Option<String>,
    /// Where the folder that was at link_at went, when one was there. Nothing
    /// is deleted; a player renames it back to undo the link.
    moved_aside: Option<String>,
    composed_root: Option<String>,
    /// Present when the package switched was a soundtrack: what the disc
    /// rebuild did, in a sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    soundtrack_note: Option<String>,
}

fn active_view(entries: &[reborn_core::mods::ActivePackage]) -> Vec<ActivePackageView> {
    entries
        .iter()
        .map(|a| ActivePackageView {
            id: a.id.to_string(),
            version: a.version.to_string(),
        })
        .collect()
}

fn sync_view(game: &GameId, outcome: reborn_core::mods::SyncOutcome) -> ModSyncView {
    use reborn_core::mods::{SyncOutcome, TextureLink, TreeStep};
    let note = outcome.explain();
    match outcome {
        SyncOutcome::Refused(why) => ModSyncView {
            game: game.to_string(),
            synced: false,
            refusal: Some(why),
            note,
            active: Vec::new(),
            missing: Vec::new(),
            tree: None,
            entries: 0,
            overlaid: 0,
            link: None,
            link_at: None,
            moved_aside: None,
            composed_root: None,
            soundtrack_note: None,
        },
        SyncOutcome::Done(r) => {
            let (tree, entries, overlaid) = match &r.tree {
                TreeStep::AlreadyCurrent => ("already_current", 0, 0),
                TreeStep::Rebuilt {
                    entries, overlaid, ..
                } => ("rebuilt", *entries, *overlaid),
            };
            let (link, link_at, moved_aside) = match &r.link {
                TextureLink::LeftAlone { at } => ("left_alone", at.clone(), None),
                TextureLink::AlreadyLinked { at, .. } => ("already_linked", at.clone(), None),
                TextureLink::Linked {
                    at, moved_aside, ..
                } => ("linked", at.clone(), moved_aside.clone()),
            };
            ModSyncView {
                game: r.game.to_string(),
                synced: true,
                refusal: None,
                note,
                active: active_view(&r.active),
                missing: active_view(&r.missing),
                tree: Some(tree),
                entries,
                overlaid,
                link: Some(link),
                link_at: Some(link_at.display().to_string()),
                moved_aside: moved_aside.map(|p| p.display().to_string()),
                composed_root: Some(r.composed_root.display().to_string()),
            soundtrack_note: None,
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Make the game load what activation.json says, right now.
///
/// The same call a launch makes, against the same user directory a launch
/// would resolve, so what the Mods page shows as applied is what the next
/// launch loads and not a parallel notion of it.
pub(crate) fn sync_active_now(p: &Platform, runtime_hint: Option<&str>) -> Result<ModSyncView, String> {
    let Startup::Owner(lock) = Supervisor::acquire(p).map_err(|e| e.to_string())? else {
        return Err(already_running());
    };
    let exe = runtime_for(p, runtime_hint);
    let preferences = reborn_core::settings::Settings::load(&reborn_core::settings::settings_path(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let user = reborn_core::session::resolve_user_dir_for(p, preferences.user_dir.as_deref(), exe.as_deref()).map_err(|e| e.to_string())?;
    let game = GameId::parse(GameBuild::Offline.game_id()).map_err(|e| e.to_string())?;
    let mut req = reborn_core::mods::SyncRequest::in_data_root(p, game.clone(), user.path())
        .map_err(|e| e.to_string())?;
    req.link_into_user_dir = preferences.manage_texture_folder.unwrap_or(true);
    let outcome =
        reborn_core::mods::sync_active(p, &lock, &req, now_ms()).map_err(|e| e.to_string())?;
    Ok(sync_view(&game, outcome))
}

/// Make the game load what activation.json says.
///
/// Read this as "apply": the activation list is the selection and this is
/// what makes it real. Idle when the tree already matches, which is what lets
/// a launch call it every time.
#[tauri::command]
async fn mods_sync(runtime: Option<String>) -> Result<ModSyncView, String> {
    tauri::async_runtime::spawn_blocking(move || sync_active_now(&Platform::detect(), runtime.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

/// Turn one package on or off, and make the game load the result.
///
/// Turning one on turns off every other member of its exclusive group first.
/// That is the pick-one rule: seven of the eleven JSGME mods write the same
/// football, and a list that let two of them be on at once would refuse to
/// compose and blame the player for a state the interface let them reach.
/// The group is computed from the file lists, never from a curated table, so
/// a new football that writes the same path joins it by itself.
#[tauri::command]
async fn mods_toggle(
    id: String,
    version: String,
    on: bool,
    runtime: Option<String>,
) -> Result<ModSyncView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let id = ModId::parse(&id).map_err(|e| e.to_string())?;
        let version = PackageVersion::parse(&version).map_err(|e| e.to_string())?;
        let root = reborn_core::mods::packages_root(&p).map_err(|e| e.to_string())?;

        // A soundtrack package changes the disc, not the texture tree, and is
        // never listed in activation.json: the soundtrack record is its state.
        let dir = root.join(id.as_str()).join(version.as_str());
        if package_kind(&dir).as_deref() == Some("soundtrack") {
            if !dir.join(reborn_core::mods::PACKAGE_MANIFEST).is_file() {
                return Err(format!("{} is not an installed package, so it cannot be switched.", dir.display()));
            }
            let data_root = p.paths.data_root().map_err(|e| e.to_string())?;
            let note = soundtrack_switch(&data_root, &dir, id.as_str(), version.as_str(), on)?;
            return Ok(ModSyncView {
                game: GameBuild::Offline.game_id().to_string(),
                synced: true,
                refusal: None,
                note: note.clone(),
                active: Vec::new(),
                missing: Vec::new(),
                tree: None,
                entries: 0,
                overlaid: 0,
                link: None,
                link_at: None,
                moved_aside: None,
                composed_root: None,
                soundtrack_note: Some(note),
            });
        }

        let activation_path =
            reborn_core::mods::activation_path(&p).map_err(|e| e.to_string())?;
        let mut activation = Activation::load(&activation_path).map_err(|e| e.to_string())?;

        if on {
            // It has to be installed to be turned on. A list entry with no
            // package behind it is a stale entry, and the interface has a way
            // to show those without making new ones.
            let dir = root.join(id.as_str()).join(version.as_str());
            ModPackage::load(&dir).map_err(|e| {
                format!(
                    "{} is not an installed package, so it cannot be turned on: {e}",
                    dir.display()
                )
            })?;

            let game = GameId::parse(GameBuild::Offline.game_id()).map_err(|e| e.to_string())?;
            let (library, _) = read_package_library(&root);
            for group in reborn_core::mods::exclusive_groups(&library, &game) {
                if !group.members.contains(&id) {
                    continue;
                }
                for other in &group.members {
                    if other != &id {
                        activation.deactivate(other);
                    }
                }
            }
            activation.activate(id, version);
        } else {
            activation.deactivate(&id);
        }
        activation.save(&activation_path).map_err(|e| e.to_string())?;

        sync_active_now(&p, runtime.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What composing these packages would do, changing nothing.
///
/// Read only, so the interface can show the file count, the disk cost and every
/// conflict before the player commits. A hard conflict comes back as a refusal
/// with a sentence rather than as an error, because two mods writing one texture
/// is a state a real library is in, not a fault. On this machine seven of the
/// eleven JSGME mods write the same football texture.
#[tauri::command]
async fn mods_plan(game: String, packages: Vec<PackageRef>) -> Result<ModPlanView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let (game, req) = compose_request(&p, &game, &packages)?;
        let composition = Composer::plan(&p, &req).map_err(|e| e.to_string())?;
        Ok(plan_view(&game, &composition))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct ModApplyView {
    game: String,
    /// False when the plan was refused, in which case nothing was written.
    applied: bool,
    refusal: Option<String>,
    composed_root: Option<String>,
    /// Directory entries made, which cost no data bytes.
    linked: u64,
    /// Full copies, which do.
    copied: u64,
    bytes_copied: u64,
    overlaid: u64,
    entries: u64,
    /// Why hardlinking was abandoned part way, when it was. Present on exFAT
    /// and FAT32 and across volumes, which on a handheld is the common case
    /// rather than an edge one.
    link_failure: Option<String>,
}

/// Rebuild the composed mod tree from these packages.
///
/// Destructive, so it takes an explicit confirm: the composed tree is deleted
/// and rebuilt. It is not destructive to anything irreplaceable, because the
/// core refuses outright when the composed root and the pristine texture tree
/// overlap, and refuses to delete a directory it did not create.
///
/// Un-applying one mod is this command with a shorter list. There is
/// deliberately no per-mod remove: an operation that restores files can fail
/// half way and leave a tree that is neither state, while a rebuild either
/// finishes or is discarded and run again.
///
/// Blocking, and it holds the single-instance lock for the whole rebuild. Two
/// launchers composing the directory Dolphin is about to load would interleave
/// writes, and the loser's composition would be silently half present.
#[tauri::command]
async fn mods_apply(
    game: String,
    packages: Vec<PackageRef>,
    confirm: bool,
) -> Result<ModApplyView, String> {
    confirmed(
        confirm,
        "mods_apply",
        "delete and rebuild the composed mod tree",
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let (game, req) = compose_request(&p, &game, &packages)?;

        let Startup::Owner(lock) = Supervisor::acquire(&p).map_err(|e| e.to_string())? else {
            return Err(already_running());
        };

        let composition = Composer::plan(&p, &req).map_err(|e| e.to_string())?;
        let Some(plan) = composition.plan() else {
            return Ok(ModApplyView {
                game: game.to_string(),
                applied: false,
                refusal: composition.refusal().map(|r| r.message()),
                composed_root: None,
                linked: 0,
                copied: 0,
                bytes_copied: 0,
                overlaid: 0,
                entries: 0,
                link_failure: None,
            });
        };

        let report = Composer::apply(&p, &lock, plan).map_err(|e| e.to_string())?;
        Ok(ModApplyView {
            game: report.game.to_string(),
            applied: true,
            refusal: None,
            composed_root: Some(report.composed_root.display().to_string()),
            linked: report.linked,
            copied: report.copied,
            bytes_copied: report.bytes_copied,
            overlaid: report.overlaid,
            entries: report.entries(),
            link_failure: report.link_failure.clone(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct ModUnapplyView {
    game: String,
    composed_root: String,
    /// False when there was nothing there, which is not a failure.
    removed: bool,
    /// Packages `activation.json` still lists after the tree is gone.
    ///
    /// This command does not edit that file. Deciding what the activation record
    /// should say after a discard is a rule, and rules belong in
    /// `reborn_core::mods` where they are tested. Reporting the mismatch lets
    /// the interface show it instead of the two quietly disagreeing.
    activation_still_lists: Vec<ActivePackageView>,
}

/// Remove the composed mod tree entirely, so the game loads the pristine one.
///
/// Destructive, so it takes a confirm. The core refuses to delete a directory
/// that holds files but no marker of its own, which is what stops this becoming
/// a recursive delete of whatever path it was handed. The shipped 2.1
/// uninstaller is that bug.
#[tauri::command]
async fn mods_unapply(game: String, confirm: bool) -> Result<ModUnapplyView, String> {
    confirmed(
        confirm,
        "mods_unapply",
        "delete the composed mod tree and go back to the pristine textures",
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let game = GameId::parse(&game).map_err(|e| e.to_string())?;
        let composed = reborn_core::mods::composed_root(&p, &game).map_err(|e| e.to_string())?;
        let activation_path = reborn_core::mods::activation_path(&p).map_err(|e| e.to_string())?;
        let activation = Activation::load(&activation_path).map_err(|e| e.to_string())?;

        let Startup::Owner(lock) = Supervisor::acquire(&p).map_err(|e| e.to_string())? else {
            return Err(already_running());
        };
        let removed = Composer::discard(&lock, &composed).map_err(|e| e.to_string())?;

        Ok(ModUnapplyView {
            game: game.to_string(),
            composed_root: composed.display().to_string(),
            removed,
            activation_still_lists: activation
                .active
                .iter()
                .map(|a| ActivePackageView {
                    id: a.id.to_string(),
                    version: a.version.to_string(),
                })
                .collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct ModVerifyView {
    game: String,
    /// False when the plan was refused, so nothing could be compared. The
    /// interface must not read that as a pass.
    checked: bool,
    refusal: Option<String>,
    composed_root: Option<String>,
    /// Absent when nothing was checked. A `false` here would claim the tree was
    /// examined and found wrong.
    matches_the_plan: Option<bool>,
    discrepancies: Vec<String>,
}

/// Check the composed tree against the plan that should have built it.
///
/// Read only and safe before a launch. Every mod file is hashed, because a
/// package manifest gives a trustworthy expected value for those; the pristine
/// side is checked by count, which is the trade the core documents.
#[tauri::command]
async fn mods_verify(game: String, packages: Vec<PackageRef>) -> Result<ModVerifyView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let (game, req) = compose_request(&p, &game, &packages)?;
        let composition = Composer::plan(&p, &req).map_err(|e| e.to_string())?;
        let Some(plan) = composition.plan() else {
            return Ok(ModVerifyView {
                game: game.to_string(),
                checked: false,
                refusal: composition.refusal().map(|r| r.message()),
                composed_root: None,
                matches_the_plan: None,
                discrepancies: Vec::new(),
            });
        };
        let verification = Composer::verify(plan).map_err(|e| e.to_string())?;
        Ok(ModVerifyView {
            game: game.to_string(),
            checked: true,
            refusal: None,
            composed_root: Some(verification.composed_root.display().to_string()),
            matches_the_plan: Some(verification.matches_the_plan()),
            discrepancies: verification
                .discrepancies
                .iter()
                .map(|d| d.message())
                .collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Migration from a 2.1 install
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct MigrationStepView {
    order: u8,
    what: String,
    from: String,
    /// The data directory it lands in, relative to the data root.
    to: String,
    /// Whether losing this would cost the player something unrecoverable.
    irreplaceable: bool,
    why: String,
}

#[derive(Debug, Serialize)]
struct MigrationSkipView {
    what: String,
    why: String,
}

#[derive(Debug, Serialize)]
struct MigrationPlanView {
    /// The folder that was examined. Absent when the caller named none, in
    /// which case nothing was looked for and `found` being false says nothing
    /// about the machine.
    legacy_root_checked: Option<String>,
    found: bool,
    has_memory_cards: bool,
    has_screenshots: bool,
    has_textures: bool,
    has_applied_mods: bool,
    has_jsgme_backup: bool,
    steps: Vec<MigrationStepView>,
    skipped: Vec<MigrationSkipView>,
    needs_mod_unapply: bool,
    /// The core's own blocking reason, when there is one.
    blocked: Option<String>,
    /// After this step number, an interruption costs nothing permanent. Worth
    /// showing during a copy that is about 2 GB.
    safe_after_step: Option<u8>,
    /// Whether `migration_apply` would do anything if it were confirmed.
    can_apply: bool,
    /// Why not, when it would not.
    cannot_apply_because: Option<String>,
}

/// The single answer to "may this migration run".
///
/// One function, called by both `migration_plan` and `migration_apply`, so the
/// button state and the command cannot disagree about whether copying is safe.
///
/// Two states stop it, and the second one is the subtle one:
///
/// - `blocked`: mods are applied and JSGME's `!BACKUP` is empty or missing, so
///   the original textures cannot be restored and the tree on disk is modded.
/// - `needs_mod_unapply`: mods are applied and a backup does exist. The core's
///   plan says textures are copied "after mods are un-applied", and **nothing
///   in this build can perform that un-apply**: `reborn_core::jsgme` reads and
///   never writes, and the migration module has no un-apply step. Copying
///   anyway would capture the overlaid state and the pristine tree would be
///   polluted from birth, which then reads as pristine forever. So it refuses
///   and says to un-apply in JSGME first.
fn migration_refusal(plan: &MigrationPlan) -> Option<String> {
    if let Some(reason) = &plan.blocked {
        return Some(reason.to_string());
    }
    if plan.needs_mod_unapply {
        return Some(
            "JSGME mods are applied in the 2.1 install. The textures there are \
             the modded files, not the originals, and this launcher cannot \
             un-apply them for you. Open JSGME, un-apply every mod, then run \
             the migration again."
                .to_string(),
        );
    }
    if plan.steps.is_empty() {
        return Some(
            "There is nothing to migrate from that folder: no memory cards, no \
             screenshots and no textures were found in it."
                .to_string(),
        );
    }
    None
}

fn plan_to_view(root: &Path, plan: Option<&MigrationPlan>) -> MigrationPlanView {
    let mut view = MigrationPlanView {
        legacy_root_checked: Some(root.display().to_string()),
        found: false,
        has_memory_cards: false,
        has_screenshots: false,
        has_textures: false,
        has_applied_mods: false,
        has_jsgme_backup: false,
        steps: Vec::new(),
        skipped: Vec::new(),
        needs_mod_unapply: false,
        blocked: None,
        safe_after_step: None,
        can_apply: false,
        cannot_apply_because: Some(format!(
            "No NFL Street Reborn 2.1 install was found at {}.",
            root.display()
        )),
    };
    let Some(plan) = plan else { return view };

    view.found = true;
    view.has_memory_cards = plan.source.has_memory_cards;
    view.has_screenshots = plan.source.has_screenshots;
    view.has_textures = plan.source.has_textures;
    view.has_applied_mods = plan.source.has_applied_mods;
    view.has_jsgme_backup = plan.source.has_jsgme_backup;
    view.needs_mod_unapply = plan.needs_mod_unapply;
    view.blocked = plan.blocked.clone();
    view.safe_after_step = plan.safe_after_step();
    view.steps = plan
        .steps
        .iter()
        .map(|s| MigrationStepView {
            order: s.order,
            what: s.what.to_string(),
            from: s.from.display().to_string(),
            to: s.to.relative().to_string(),
            irreplaceable: s.irreplaceable,
            why: s.why.to_string(),
        })
        .collect();
    view.skipped = plan
        .skipped
        .iter()
        .map(|s| MigrationSkipView {
            what: s.what.to_string(),
            why: s.why.to_string(),
        })
        .collect();
    let refusal = migration_refusal(plan);
    view.can_apply = refusal.is_none();
    view.cannot_apply_because = refusal;
    view
}

/// What migrating a 2.1 install would copy, in order, copying nothing.
///
/// `legacy_root` is required rather than guessed. The core knows the standard
/// location but does not expose it, and spelling `%APPDATA%\NFL Street Reborn`
/// here would put a second copy of that path in a second layer. When it is
/// absent nothing is examined, and the view says so instead of reporting "no
/// install found", which would be a claim about the machine.
#[tauri::command]
async fn migration_plan(legacy_root: Option<String>) -> Result<MigrationPlanView, String> {
    let Some(root) = legacy_root else {
        return Err(
            "migration_plan needs the 2.1 install folder. Ask the player to \
             pick it, or pass the standard location. Reporting \"nothing \
             found\" without looking would be a claim about their machine."
                .to_string(),
        );
    };
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(root);
        let install = LegacyInstall::detect(&root);
        let plan = install.as_ref().map(MigrationPlan::build);
        Ok(plan_to_view(&root, plan.as_ref()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct MigrationStepResultView {
    order: u8,
    what: String,
    to: String,
    /// BYTES, not files. copy_tree_no_clobber returns what std::fs::copy
    /// returns, summed, and the core's tests pin it as bytes. This field was
    /// called files_copied and was reporting 22 million files.
    bytes_copied: u64,
}

#[derive(Debug, Serialize)]
struct MigrationApplyView {
    steps: Vec<MigrationStepResultView>,
    total_bytes: u64,
    /// Present when the copy finished but first run's journal would not take
    /// it, which happens when onboarding is already latched complete. The files
    /// moved either way, and saying nothing would leave the flow looking stuck
    /// for a reason nobody could see.
    journal_note: Option<String>,
    /// Stated every time, because a player watching a 2 GB copy needs to know
    /// the source is still intact.
    note: &'static str,
}

/// Copy a 2.1 install's irreplaceable data into the launcher's data root.
///
/// Destructive in the sense that matters here: it writes, and it writes the
/// player's memory cards. It never deletes or changes anything in the 2.1
/// install, and it refuses to overwrite anything already at the destination,
/// which is what stops a second attempt replacing a newer save with an older
/// one.
///
/// Order is the core's, saves first, so an interruption after step 1 costs
/// nothing permanent. Blocking, and it holds the single-instance lock: copying
/// memory cards while a game is running would race Dolphin writing them.
#[tauri::command]
async fn migration_apply(legacy_root: Option<String>, confirm: bool, saves_only: Option<bool>) -> Result<MigrationApplyView, String> {
    confirmed(
        confirm,
        "migration_apply",
        "copy memory cards, screenshots and textures out of the 2.1 install",
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let root = legacy_root.map(PathBuf::from).or_else(|| default_legacy_root(&p))
            .ok_or("Choose your previous installation folder before copying saves.")?;
        let Some(install) = LegacyInstall::detect(&root) else {
            return Err(format!(
                "No NFL Street Reborn 2.1 install at {}. Nothing was copied.",
                root.display()
            ));
        };
        let plan = if saves_only.unwrap_or(false) { MigrationPlan::saves_only(&install) } else { MigrationPlan::build(&install) };
        if let Some(reason) = migration_refusal(&plan) {
            return Err(format!("{reason} Nothing was copied."));
        }

        let Startup::Owner(_lock) = Supervisor::acquire(&p).map_err(|e| e.to_string())? else {
            return Err(already_running());
        };

        // Before the first byte moves. If the machine dies mid-copy, this
        // marker is the only evidence the flow was inside a stage that writes,
        // and without it the next run would offer the copy again from scratch.
        let mut flow = open_flow(Some(root.display().to_string()))?;
        let reached = flow.reach(Stage::MigrationCopy).map_err(|e| e.to_string())?;
        if let Advance::Refused(why) = &reached {
            return Err(format!(
                "{} Nothing was copied.",
                why.message()
            ));
        }

        let data = p.paths.data_root().map_err(|e| e.to_string())?;
        let mut steps = Vec::new();
        let mut total = 0u64;
        for step in &plan.steps {
            let dest = data.join(step.to.relative());
            let copied = reborn_core::migration::copy_tree_no_clobber(&step.from, &dest)
                .map_err(|e| e.to_string())?;
            total += copied;
            steps.push(MigrationStepResultView {
                order: step.order,
                what: step.what.to_string(),
                to: dest.display().to_string(),
                bytes_copied: copied,
            });
        }
        if install.has_memory_cards {
            let saved = saved_paths(&p)?;
            let user = reborn_core::session::resolve_user_dir_for(&p, saved.user_dir.as_deref(), saved.runtime_exe.as_deref())
                .map_err(|e| e.to_string())?;
            let target = user.path().join("GC");
            let source = install.memory_card_dir();
            if std::fs::canonicalize(&source).ok() != std::fs::canonicalize(&target).ok() {
                total += reborn_core::migration::copy_tree_no_clobber(&source, &target).map_err(|e| e.to_string())?;
            }
        }
        // After, with the measured count. Recorded here rather than taken
        // from the interface for the usual reason: this is the only code that
        // knows what actually moved.
        //
        // A refused journal write is reported, not raised. The copy already
        // happened, and failing the whole command would say it had not.
        let recorded = flow
            .satisfy(Stage::MigrationCopy, Satisfaction::Migrated { bytes: total })
            .map_err(|e| e.to_string())?;
        let journal_note = match &recorded {
            Advance::Recorded | Advance::AlreadyRecorded => None,
            Advance::Refused(why) => Some(why.message()),
        };

        Ok(MigrationApplyView {
            steps,
            total_bytes: total,
            journal_note,
            note: "Nothing in the 2.1 install was changed or removed, and \
                   nothing already present at the destination was overwritten.",
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Library verify and repair
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct LibraryVerifyView {
    component: String,
    /// Where the files were looked for, resolved through the core's path guard
    /// rather than taken from the manifest, so a manifest naming `../../saves`
    /// cannot point a verify at somebody's memory cards.
    root: String,
    healthy: bool,
    intact: usize,
    missing: usize,
    damaged: usize,
    /// Files that do not match the manifest because the player customized them
    /// and the launcher recorded the result. These are never repaired: treating
    /// a rebuilt soundtrack as corruption is what re-downloads 1.5 GB over
    /// somebody's music.
    user_modified: usize,
    missing_paths: Vec<String>,
    damaged_paths: Vec<String>,
    user_modified_paths: Vec<String>,
    /// Bytes a repair would actually re-fetch.
    repair_bytes: u64,
    /// Whether a repair is possible at all today. See `library_repair`.
    repair_available: bool,
}

/// Count the verdicts, keeping user-modified files out of the repair set.
///
/// The per-file verdict list is deliberately not returned. The textures
/// component on this machine is 27,025 files, and a full verdict row per file
/// would push tens of megabytes of JSON through the bridge on every verify. The
/// counts are exact and the three path lists carry every file a screen has
/// anything to say about.
fn verify_view(report: &VerifyReport, component: &Component, root: &Path) -> LibraryVerifyView {
    let mut intact = 0usize;
    let mut missing = Vec::new();
    let mut damaged = Vec::new();
    let mut user_modified = Vec::new();

    for (path, verdict) in &report.verdicts {
        match verdict {
            FileVerdict::Intact => intact += 1,
            FileVerdict::Missing => missing.push(path.clone()),
            FileVerdict::Damaged { .. } => damaged.push(path.clone()),
            FileVerdict::UserModified => user_modified.push(path.clone()),
        }
    }

    LibraryVerifyView {
        component: report.component.clone(),
        root: root.display().to_string(),
        healthy: report.is_healthy(),
        intact,
        missing: missing.len(),
        damaged: damaged.len(),
        user_modified: user_modified.len(),
        missing_paths: missing,
        damaged_paths: damaged,
        user_modified_paths: user_modified,
        repair_bytes: report.repair_bytes(component),
        repair_available: false,
    }
}

/// Verify an installed component against a component manifest.
///
/// `manifest_path` is required and there is no default, because no component
/// manifest is stored anywhere on this machine. `content.rs` takes manifest
/// bytes from a signed release at update time and keeps only a version pointer
/// afterwards, so there is nothing on disk to verify against until a release
/// has been opened. Inventing a file list would report a healthy component that
/// was never checked.
///
/// Blocking: every file whose length already matches gets hashed, and for a
/// 1.5 GB image that is seconds rather than milliseconds.
#[tauri::command]
async fn library_verify(
    manifest_path: String,
    root: Option<String>,
) -> Result<LibraryVerifyView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let manifest_path = PathBuf::from(manifest_path);
        let bytes = std::fs::read(&manifest_path)
            .map_err(|e| format!("{} could not be read: {e}", manifest_path.display()))?;
        let component = Component::parse(&bytes).map_err(|e| {
            format!(
                "{} is not a readable component manifest: {e}",
                manifest_path.display()
            )
        })?;

        let layout =
            reborn_core::content::ContentLayout::from_platform(&p).map_err(|e| e.to_string())?;
        let resolved = match root {
            Some(r) => PathBuf::from(r),
            None => layout.live_root(&component).map_err(|e| e.to_string())?,
        };

        // Without this record there is no way to tell a rebuilt soundtrack from
        // a corrupted download, because both are simply "not the shipped hash".
        // An absent record is an empty one, which is correct: nothing has been
        // rebuilt yet.
        let state_path = reborn_core::music::music_root(layout.data_root()).join("state.json");
        let state = RebuildState::load(&state_path).map_err(|e| e.to_string())?;

        let report = Verifier::new(&resolved, &state)
            .verify(&component)
            .map_err(|e| e.to_string())?;
        Ok(verify_view(&report, &component, &resolved))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Repair a component by re-fetching the files that are missing or damaged.
///
/// Refuses, and says exactly what is missing. Repair is a content update with
/// `is_repair` set, and the path into `content.rs` needs two things this
/// workspace does not have: a `SignatureVerifier`, because a release cannot be
/// opened without one and no verifier is implemented on purpose while key
/// custody is unresolved, and a `BlobSource`, because the crate contains no
/// network code and no URL.
///
/// Returning "0 files repaired" instead would be the worst possible answer: the
/// screen would show a successful repair of a component nothing touched.
#[tauri::command]
async fn library_repair(component: String, confirm: bool) -> Result<LibraryVerifyView, String> {
    confirmed(
        confirm,
        "library_repair",
        "overwrite the installed files of that component",
    )?;
    Err(format!(
        "Cannot repair {component}: no signed release can be opened on this \
         machine yet. Two pieces are missing, both deliberately: a signature \
         verifier for nsr.release.v1, which is unimplemented while key custody \
         is unresolved, and a blob source, because the core holds no network \
         code. Until both exist, use library_verify to show what is damaged and \
         reinstall the component by hand."
    ))
}

// ---------------------------------------------------------------------------
// First run
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct PlannedStageView {
    stage: &'static str,
    /// One based, over the stages that apply to this machine. Deliberately not
    /// the wireframes' "of 12": seven of those twelve are specified nowhere, and
    /// a hardcoded total would be an invented number on screen.
    position: usize,
    total: usize,
    /// Which `WIREFRAMES.md` section drew this stage. Absent means the screen is
    /// unspecified and whoever builds it is designing rather than implementing.
    wireframe_section: Option<u8>,
    irreversible: bool,
    allows_back: bool,
    satisfied: bool,
}

#[derive(Debug, Serialize)]
struct NextStageView {
    /// `show`, `reconcile` or `already_complete`.
    kind: &'static str,
    stage: Option<&'static str>,
    position: Option<usize>,
    total: Option<usize>,
    /// True when the player has been here before, which is the difference
    /// between "Continue" and "Get started".
    resumed: Option<bool>,
    /// Present only for `reconcile`: this stage writes files and the last run
    /// stopped inside it, so its work may have partly happened.
    what_to_do: Option<&'static str>,
    flow_version: Option<u32>,
}

#[derive(Debug, Serialize)]
struct PrepTaskView {
    slug: &'static str,
    label: &'static str,
}

#[derive(Debug, Serialize)]
struct OnboardingSummaryView {
    controller: Option<String>,
    controller_port: Option<u8>,
    no_controller_because: Option<String>,
    graphics: Option<String>,
    /// Absent means the machine was never measured, in which case the
    /// wireframe's "RECOMMENDED FOR YOUR PC" badge has nothing behind it and
    /// must not be drawn.
    graphics_was_recommended: Option<bool>,
    detected_hardware: Option<String>,
    /// The button art chosen, by slug. Absent means never asked, and there is
    /// no declined value because the game always draws something.
    controls_profile: Option<&'static str>,
    /// Whether ordinary updates are applied without asking. Absent until
    /// first run has asked.
    automatic_updates: Option<bool>,
    migrated_from: Option<String>,
    migrated_bytes: Option<u64>,
    game_bytes: Option<u64>,
    /// Named fields nothing measured. The Ready screen prints absence rather
    /// than a plausible default.
    absent: Vec<String>,
}

#[derive(Debug, Serialize)]
struct OnboardingView {
    complete: bool,
    completed_flow_version: Option<u32>,
    /// True when the journal stopped at a line that would not parse. Every
    /// irreversible stage then routes to `reconcile`, because the journal's
    /// silence about a stage no longer proves the work never ran.
    truncated: bool,
    /// Stages answered under an older version of first run, named.
    ///
    /// NOT a problem and not the same as `truncated`. It means the flow changed
    /// since this machine last ran it, so those answers no longer apply. Shown
    /// rather than dropped silently, because a step that quietly reappears
    /// looks like a step the player never did.
    superseded: Vec<String>,
    next: NextStageView,
    plan: Vec<PlannedStageView>,
    /// Stages this machine does not have, and why. Reported rather than
    /// silently dropped: a flow that quietly omits a step is one nobody can
    /// check.
    set_aside: Vec<MigrationSkipView>,
    preparation_remaining: Vec<PrepTaskView>,
    tasks_done: Vec<PrepTaskView>,
    /// The folder that was examined for a 2.1 install. Absent when the caller
    /// named none, in which case first run will not offer migration at all.
    legacy_root_checked: Option<String>,
    migration_blocked: Option<String>,
    summary: OnboardingSummaryView,
}

fn next_view(next: &Next) -> NextStageView {
    match next {
        Next::Show {
            stage,
            position,
            total,
            resumed,
        } => NextStageView {
            kind: "show",
            stage: Some(stage.slug()),
            position: Some(*position),
            total: Some(*total),
            resumed: Some(*resumed),
            what_to_do: None,
            flow_version: None,
        },
        Next::Reconcile {
            stage,
            position,
            total,
            what_to_do,
        } => NextStageView {
            kind: "reconcile",
            stage: Some(stage.slug()),
            position: Some(*position),
            total: Some(*total),
            resumed: None,
            what_to_do: Some(what_to_do),
            flow_version: None,
        },
        Next::AlreadyComplete { flow_version } => NextStageView {
            kind: "already_complete",
            stage: None,
            position: None,
            total: None,
            resumed: None,
            what_to_do: None,
            flow_version: Some(*flow_version),
        },
    }
}

/// Where first run has got to, read from its journal on disk.
///
/// The write side is below. Each stage gets its own command rather than one
/// generic answer(stage, evidence), for the reason this comment used to give for
/// there being no write side at all: the stages are satisfied by different
/// evidence, and one shape wide enough for all of them would be wide enough to
/// accept invented values.
///
/// So no command below takes a measurement from the interface. The controller
/// command is given a port and reads the device string from the emulator's own
/// enumeration. The game command is given a path and weighs the file itself.
/// The mods command is given package references and resolves them against the
/// library on disk. What the interface may state is what IT did, which is which
/// mods the player picked and whether the overlap between them was shown.
#[tauri::command]
async fn onboarding_progress(legacy_root: Option<String>) -> Result<OnboardingView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let checked = if setup_tools::fresh(&p) { None } else { legacy_root.map(PathBuf::from).or_else(|| default_legacy_root(&p)) };
        let env = match checked.as_deref().and_then(LegacyInstall::detect) {
            Some(install) => Environment::with_legacy_install(install),
            None => Environment::nothing_found(),
        };

        let flow = Onboarding::open(&p, env).map_err(|e| e.to_string())?;
        let progress = flow.progress();
        let summary = flow.summary();

        Ok(OnboardingView {
            complete: flow.is_complete(),
            completed_flow_version: progress.completed_flow_version(),
            truncated: progress.is_truncated(),
            superseded: progress.superseded().to_vec(),
            next: next_view(&flow.next()),
            plan: flow
                .plan()
                .iter()
                .map(|p| PlannedStageView {
                    stage: p.stage.slug(),
                    position: p.position,
                    total: p.total,
                    wireframe_section: p.stage.wireframe_section(),
                    irreversible: p.stage.is_irreversible(),
                    allows_back: p.stage.allows_back(),
                    satisfied: progress.satisfaction(p.stage).is_some(),
                })
                .collect(),
            set_aside: flow
                .set_aside()
                .iter()
                .map(|(stage, why)| MigrationSkipView {
                    what: stage.slug().to_string(),
                    why: why.to_string(),
                })
                .collect(),
            preparation_remaining: flow
                .preparation_remaining()
                .iter()
                .map(|t| PrepTaskView {
                    slug: t.slug(),
                    label: t.label(),
                })
                .collect(),
            tasks_done: progress
                .tasks_done()
                .iter()
                .map(|t| PrepTaskView {
                    slug: t.slug(),
                    label: t.label(),
                })
                .collect(),
            legacy_root_checked: checked.map(|c| c.display().to_string()),
            migration_blocked: flow.migration_plan().and_then(|plan| plan.blocked.clone()),
            summary: OnboardingSummaryView {
                controller: summary.controller.clone(),
                controller_port: summary.controller_port,
                no_controller_because: summary.no_controller_because.clone(),
                graphics: summary
                    .graphics
                    .map(|g| format!("{:?}", g.preset()).to_lowercase()),
                graphics_was_recommended: summary.graphics_was_recommended,
                detected_hardware: summary.detected_hardware.clone(),
                controls_profile: summary.controls_profile.map(|p| p.slug()),
                automatic_updates: summary.automatic_updates,
                migrated_from: summary
                    .migrated_from
                    .as_ref()
                    .map(|p| p.display().to_string()),
                migrated_bytes: summary.migrated_bytes,
                game_bytes: summary.game_bytes,
                absent: summary.absent().iter().map(|a| a.to_string()).collect(),
            },
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Removable drives
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct DriveView {
    /// What the player sees, for example `E:` or `/Volumes/XBOX-USB`.
    mount_point: String,
    label: Option<String>,
    total_bytes: u64,
    free_bytes: u64,
    filesystem: Option<String>,
    /// A value stable across remounts. Absent on every platform today, and that
    /// is not a placeholder: without it an interrupted Xbox copy starts over
    /// instead of resuming, which is the intended degradation. A label is not a
    /// substitute, because two sticks can both be called `XBOX-USB`.
    volume_identity: Option<String>,
    selectable: bool,
    /// Why a drive that was found will not be written to. A refused drive stays
    /// in the list, or the player replugs it forever.
    why_not: Option<String>,
}

#[derive(Debug, Serialize)]
struct DrivesView {
    /// Every volume the operating system would admit to.
    volumes_seen: usize,
    /// How many of those it called removable media.
    removable_seen: usize,
    /// The rest. "No drives found" and "one drive found and it is your system
    /// disk" are different facts, and a picker that shows neither leaves the
    /// player believing their stick is unplugged.
    fixed_seen: usize,
    offers: Vec<DriveView>,
}

// ---------------------------------------------------------------------------
// Onboarding, the write side
// ---------------------------------------------------------------------------

/// What one answer did, and where the flow now stands.
///
/// Every answering command returns this, so a caller never keeps its own copy
/// of the stage order and never has to guess what to draw next.
#[derive(Debug, Serialize)]
struct OnboardingStepView {
    /// `recorded`, `already_recorded`, or `refused`. Kept distinct rather than
    /// collapsed into a boolean: "this was already answered" and "this was
    /// rejected" want different screens.
    outcome: &'static str,
    /// Present only when refused, in the core's own words. The core writes
    /// these as sentences that say what to do, so they are passed through
    /// rather than rewritten here.
    refused_because: Option<String>,
    next: NextStageView,
    complete: bool,
}

/// Open the journal the way the read side does, so both see one flow.
fn open_flow(legacy_root: Option<String>) -> Result<Onboarding, String> {
    let p = Platform::detect();
    let legacy = if setup_tools::fresh(&p) { None } else { legacy_root.map(PathBuf::from).or_else(|| default_legacy_root(&p)) };
    let env = match legacy.as_deref() {
        Some(root) => match LegacyInstall::detect(root) {
            Some(install) => Environment::with_legacy_install(install),
            None => Environment::nothing_found(),
        },
        None => Environment::nothing_found(),
    };
    Onboarding::open(&p, env).map_err(|e| e.to_string())
}

fn step_view(flow: &Onboarding, advance: Advance) -> OnboardingStepView {
    let (outcome, refused_because) = match &advance {
        Advance::Recorded => ("recorded", None),
        Advance::AlreadyRecorded => ("already_recorded", None),
        Advance::Refused(r) => ("refused", Some(r.message())),
    };
    OnboardingStepView {
        outcome,
        refused_because,
        next: next_view(&flow.next()),
        complete: flow.is_complete(),
    }
}

/// Open a stage's screen.
///
/// Permissive on purpose, matching the core: re-opening a screen is how a
/// resumed run gets back to where it was. What it records is that the screen
/// was seen, which is what stops an irreversible stage being entered twice.
#[tauri::command]
async fn onboarding_reach(
    stage: String,
    legacy_root: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = stage_from_slug(&stage)?;
        let mut flow = open_flow(legacy_root)?;
        let advance = flow.reach(target).map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Acknowledge the welcome screen. Nothing is decided, hence no arguments.
#[tauri::command]
async fn onboarding_acknowledge(legacy_root: Option<String>) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut flow = open_flow(legacy_root)?;
        let advance = flow
            .satisfy(Stage::Welcome, Satisfaction::Acknowledged)
            .map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn checked_setup_image(path: &Path) -> Result<u64, String> {
    setup_tools::check_reborn_id(path)?;
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot read the game image: {e}"))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("The selected game image is empty or is not a file.".into());
    }
    Ok(metadata.len())
}

/// Confirm the shipped game image, weighing it here.
///
/// The byte count is read from the file rather than accepted as an argument. An
/// interface that passed one would be reporting a size nothing measured, and
/// this number is printed on the Ready screen.
#[tauri::command]
async fn onboarding_confirm_game(
    image: Option<String>,
    legacy_root: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let legacy = legacy_root.as_deref().map(reborn_core::session::user_path)
            .or_else(|| default_legacy_root(&p));
        // A typo in an explicit selection must not silently confirm a different detected image.
        if let Some(chosen) = image.as_deref() {
            checked_setup_image(&reborn_core::session::user_path(chosen))?;
        }
        let user = selected_user_dir(&p, None)?;
        let (_, game) = find_runtime_and_game_for(&p, None, image.as_deref(), legacy.as_deref(), user.as_deref())?;
        let total_bytes = Some(checked_setup_image(&game)?);
        let file = reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
        // Persist before advancing so a failed save leaves this step available to retry.
        reborn_core::settings::set_path(&file, reborn_core::settings::PathSetting::Game, &game.to_string_lossy())
            .map_err(|e| e.to_string())?;
        let mut flow = open_flow(legacy_root)?;
        let advance = flow
            .satisfy(
                Stage::GameVerification,
                Satisfaction::GameConfirmed { total_bytes },
            )
            .map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Record the choice alongside the local hardware recommendation, if available.
#[tauri::command]
async fn onboarding_choose_graphics(
    preset: String,
    legacy_root: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let hardware=setup_tools::hardware_info();
        let chosen = RecordedPreset::from(preset_from(&preset));
        let mut flow = open_flow(legacy_root)?;
        let advance = flow
            .satisfy(
                Stage::SystemCheck,
                Satisfaction::Graphics {
                    chosen,
                    recommended: hardware["recommended"].as_str().map(|p|RecordedPreset::from(preset_from(p))),
                    detected: hardware["description"].as_str().map(str::to_owned),
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Accept the pad on a port, taking its identity from the emulator.
///
/// The interface picks a PORT. It does not pass a device name. The string that
/// goes into the journal ends up in `GCPadNew.ini`, and it has to be the
/// emulator's own `source/id/name`, because a friendly name parses fine there
/// and binds nothing. So the pad is looked up and its own string is used.
///
/// `runtime` is the emulator path. With it, every backend the emulator knows is
/// searched; without it only XInput is, which on a real machine misses WGInput
/// entirely and would miss every PlayStation pad. The refusal says which case
/// it was, because "no pad" and "no pad, but we only looked in one place" send
/// the player to do different things.
#[tauri::command]
async fn onboarding_choose_controller(
    port: u8,
    runtime: Option<String>,
    legacy_root: Option<String>,
    expected_device: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let scan = match runtime.as_deref() {
            Some(exe) => {
                let scratch = p
                    .paths
                    .data_root()
                    .map_err(|e| e.to_string())?
                    .join(DataDir::Cache.relative());
                reborn_core::devices::scan_via_runtime(&p, Path::new(exe), &scratch)
                    .unwrap_or_else(|_| DeviceScanner::scan())
            }
            None => DeviceScanner::scan(),
        };

        // The lookup, and the wording of every refusal, live in the core. A
        // window that built this itself could build the wrong evidence, and
        // nothing could test it there.
        if let Some(expected) = expected_device.as_deref() {
            let pads = scan.playable_pads();
            let actual = pads.get(usize::from(port)).map(|pad| pad.device_string());
            if actual.as_deref() != Some(expected) {
                return Err("The controller list changed. Scan again and select your controller.".to_string());
            }
        }
        let answer = Satisfaction::controller_on_port(&scan, port)?;
        let settings_file = reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;
        let mut preferences = reborn_core::settings::Settings::load(&settings_file).map_err(|e|e.to_string())?;
        preferences.primary_controller = scan.playable_pads().get(usize::from(port)).map(|pad|pad.device_string());
        if let Some(slots)=preferences.controller_slots.as_mut(){let selected=preferences.primary_controller.clone();for slot in slots.iter_mut(){if *slot==selected{*slot=None;}}slots[0]=selected;}
        preferences.save(&settings_file).map_err(|e|e.to_string())?;
        let mut flow = open_flow(legacy_root)?;
        let advance = flow
            .satisfy(Stage::ControllerSetup, answer)
            .map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Continue with no pad, recording why.
///
/// A reason is required. The launcher shows it later instead of leaving the
/// controller line blank, and a blank line is indistinguishable from a stage
/// that was never answered.
#[tauri::command]
async fn onboarding_decline_controller(
    because: String,
    legacy_root: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let because = because.trim().to_string();
        if because.is_empty() {
            return Err("a reason is needed, because the launcher shows it later rather \
                        than leaving the controller line blank"
                .to_string());
        }
        let mut flow = open_flow(legacy_root)?;
        let advance = flow
            .satisfy(
                Stage::ControllerSetup,
                Satisfaction::NoController { because },
            )
            .map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct ControlsSetView {
    profile: &'static str,
    label: &'static str,
    /// What it was, or absent when nothing was set before.
    was: Option<&'static str>,
    /// True when it was already this, so nothing was written.
    already_set: bool,
    /// The package now on, absent for GameCube which applies none.
    package: Option<&'static str>,
    /// Packages turned off, which is how the profiles stay exclusive.
    turned_off: Vec<String>,
    /// Present when the profile needs a package that is not installed. The
    /// choice still stands and the game draws its own art until it arrives.
    missing_package: Option<String>,
    /// The tree and link, made to match. Absent when the game is running,
    /// in which case the next launch does it; sync_note says so.
    synced: Option<ModSyncView>,
    sync_note: String,
}

/// Change the button art, at any time.
///
/// A SETTING, not a first run answer. The onboarding journal latches and stays
/// closed, so a preference kept only there could never be changed without
/// re-walking the wizard; see reborn_core::settings for why they are separate.
///
/// Applies immediately as well as recording, so the next launch has nothing
/// left to do and a player who changes this does not have to guess whether it
/// took effect.
#[tauri::command]
async fn controls_set(profile: String, runtime: Option<String>) -> Result<ControlsSetView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let chosen = ControlsProfile::from_slug(&profile).ok_or_else(|| {
            format!(
                "no controls profile called {profile}. Known: {}",
                ControlsProfile::ALL
                    .iter()
                    .map(|p| p.slug())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

        let p = Platform::detect();
        let settings_file =
            reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
        let outcome = reborn_core::settings::set_controls_profile(&settings_file, chosen)
            .map_err(|e| e.to_string())?;

        // Only the packages this profile could want, so changing an icon set
        // does not wait on a full library scan.
        let root = reborn_core::mods::packages_root(&p).map_err(|e| e.to_string())?;
        let mut installed = Vec::new();
        if let Some(id) = chosen.package_id() {
            if let Ok(versions) = std::fs::read_dir(root.join(id)) {
                for version in versions.flatten() {
                    if let Ok(package) = ModPackage::load(&version.path()) {
                        installed.push(package);
                    }
                }
            }
        }

        let activation_file =
            reborn_core::mods::activation_path(&p).map_err(|e| e.to_string())?;
        let applied =
            reborn_core::mods::apply_controls_profile(&activation_file, chosen, &installed)
                .map_err(|e| e.to_string())?;

        let (synced, sync_note) = match sync_active_now(&p, runtime.as_deref()) {
            Ok(view) => {
                let note = view.note.clone();
                (Some(view), note)
            }
            Err(why) => (None, format!("Recorded. Applied at the next launch: {why}")),
        };

        Ok(ControlsSetView {
            profile: chosen.slug(),
            label: chosen.label(),
            was: match outcome {
                reborn_core::settings::Set::Changed { from } => from.map(|p| p.slug()),
                reborn_core::settings::Set::AlreadySet => Some(chosen.slug()),
            },
            already_set: matches!(outcome, reborn_core::settings::Set::AlreadySet),
            package: applied.turned_on.as_ref().map(|_| {
                chosen.package_id().unwrap_or("")
            }),
            turned_off: applied.turned_off.iter().map(|i| i.to_string()).collect(),
            missing_package: applied.missing_package,
            // Made real now, not at the next launch, so a player who changes
            // this does not have to guess whether it took. While the game is
            // running the lock is held and the launch that follows does it.
            synced,
            sync_note,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Record which button art to show.
///
/// Three profiles and no declining, because the game always draws something.
/// GameCube is the profile that applies NO pack: the prompt textures are
/// add-only, so applying none does not restore anything, it uncovers what the
/// disc itself always drew.
///
/// The profile is stored, never a pack name. The mapping lives in
/// ControlsProfile::pack, and a pack name in the journal would go stale the
/// moment a pack was renamed.
#[tauri::command]
async fn onboarding_choose_controls(
    profile: String,
    legacy_root: Option<String>,
) -> Result<OnboardingControlsView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let chosen = ControlsProfile::from_slug(&profile).ok_or_else(|| {
            format!(
                "no controls profile called {profile}. Known: {}",
                ControlsProfile::ALL
                    .iter()
                    .map(|p| p.slug())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

        let mut flow = open_flow(legacy_root)?;
        let advance = flow
            .satisfy(
                Stage::ControlsProfile,
                Satisfaction::Controls { profile: chosen },
            )
            .map_err(|e| e.to_string())?;

        Ok(OnboardingControlsView {
            step: step_view(&flow, advance),
            profile: chosen.slug(),
            label: chosen.label(),
            // Absent for GameCube, which is the answer and not a gap. A caller
            // that read this as "unknown" and applied a pack anyway would
            // override the one thing the player chose.
            pack: chosen.package_id(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct OnboardingControlsView {
    step: OnboardingStepView,
    profile: &'static str,
    label: &'static str,
    /// The prompt pack preparation will apply, or absent for GameCube.
    pack: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct ControlsCurrentView {
    /// The profile in force, or absent when neither settings nor first run
    /// has one. Absent is a real state: nothing has been chosen, and the
    /// interface must be able to tell that from a player choosing Xbox.
    profile: Option<&'static str>,
    label: Option<&'static str>,
    /// "settings" when changed after first run, "first_run" when only the
    /// journal has it, "none" otherwise.
    source: &'static str,
}

/// The controls profile in force, and where it came from.
///
/// Read only. Settings first, first run as the seed, the same order a
/// launch resolves it in, so what the interface shows as current is what
/// the next launch draws.
#[tauri::command]
async fn controls_current() -> Result<ControlsCurrentView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let flow = Onboarding::open(&p, Environment::nothing_found()).map_err(|e| e.to_string())?;
        let from_first_run = flow.summary().controls_profile;
        let settings_file =
            reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
        let from_settings = reborn_core::settings::Settings::load(&settings_file)
            .map_err(|e| e.to_string())?
            .controls_profile;
        let (profile, source) = match (from_settings, from_first_run) {
            (Some(p), _) => (Some(p), "settings"),
            (None, Some(p)) => (Some(p), "first_run"),
            (None, None) => (None, "none"),
        };
        Ok(ControlsCurrentView {
            profile: profile.map(|p| p.slug()),
            label: profile.map(|p| p.label()),
            source,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct ControlsOptionView {
    slug: &'static str,
    label: &'static str,
    describes: &'static str,
    pack: Option<&'static str>,
    /// True for the one the pad suggests, when a pad was found. A suggestion
    /// only: an Xbox pad with GameCube art is a real thing someone wants, since
    /// the game is a GameCube game.
    suggested: bool,
}

/// The three profiles, with the one this machine's pad suggests marked.
///
/// Read only. Offered so an interface never hardcodes the list, and so the
/// suggestion comes from the pad that was actually found rather than a guess in
/// the window.
#[tauri::command]
async fn controls_options(runtime: Option<String>) -> Result<Vec<ControlsOptionView>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let scan = match runtime.as_deref() {
            Some(exe) => {
                let scratch = p
                    .paths
                    .data_root()
                    .map_err(|e| e.to_string())?
                    .join(DataDir::Cache.relative());
                reborn_core::devices::scan_via_runtime(&p, Path::new(exe), &scratch)
                    .unwrap_or_else(|_| DeviceScanner::scan())
            }
            None => DeviceScanner::scan(),
        };

        // No pad means no suggestion, rather than a suggestion nothing backs.
        let saved=reborn_core::settings::Settings::load(&reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let pads=scan.playable_pads();
        let selected=pads.iter().find(|pad|Some(pad.device_string()).as_ref()==saved.primary_controller.as_ref()).or_else(||pads.first());
        let suggested=selected.map(|pad|ControlsProfile::suggested_for(pad.identity().family));

        Ok(ControlsProfile::ALL
            .iter()
            .map(|profile| ControlsOptionView {
                slug: profile.slug(),
                label: profile.label(),
                describes: profile.describes(),
                pack: profile.package_id(),
                suggested: suggested == Some(*profile),
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct PrepRunView {
    step: OnboardingStepView,
    task: &'static str,
    label: &'static str,
    /// What the task actually did, in the words of whatever did it.
    ///
    /// Not a status word. "wrote 5 keys across 2 files" and "nothing to change"
    /// are both success and mean different things, and a screen that showed
    /// both as a tick would be hiding the difference.
    did: String,
    /// Files the task wrote, so a player can see it touched what it said.
    wrote: Vec<String>,
}

/// The first game image in an existing install, if there is one.
///
/// Sorted, so two images do not resolve differently between runs. Named "first"
/// rather than "the" on purpose: a real install here holds seventeen, and
/// picking one is a guess that the caller may want to override.
/// Run one preparation task, then record it.
///
/// The order matters and is the opposite of the usual one: the work happens
/// FIRST and is only recorded once it succeeded. Recording first would leave a
/// journal claiming a step finished when it had not, and preparation is the one
/// stage whose journal is read back as proof.
///
/// Preparation configures. Nothing here installs, copies or downloads: the
/// installer put the files on disk and Dolphin is bundled, so there is nothing
/// left to fetch. This reverses RECONCILIATION_RULINGS 1088, on Adam's
/// instruction.
#[tauri::command]
async fn onboarding_run_task(
    task: String,
    runtime: Option<String>,
    game: Option<String>,
    legacy_root: Option<String>,
) -> Result<PrepRunView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = prep_task_from_slug(&task)?;
        let p = Platform::detect();

        // The before-marker, set by the code that is about to write. That is
        // the whole point of it: an interrupted run has to be able to tell that
        // preparation started, and only the thing that starts it knows when.
        //
        // Here rather than left to the caller. A screen that forgot to reach
        // first would refuse every task with advice about calling reach, which
        // is a sentence written for a programmer and shown to a player.
        {
            let mut flow = open_flow(legacy_root.clone())?;
            if let Advance::Refused(why) = flow
                .reach(Stage::Preparation)
                .map_err(|e| e.to_string())?
            {
                return Err(format!("{} Nothing was changed.", why.message()));
            }
        }

        // Read the answers back out of the journal rather than taking them as
        // arguments. They were recorded for exactly this, and an argument here
        // could configure the machine for a preset nobody chose.
        let flow = open_flow(legacy_root.clone())?;
        let summary = flow.summary();

        let (did, wrote) = match target {
            PrepTask::WriteConfig | PrepTask::CheckEverything => {
                let preset = summary
                    .graphics
                    .map(|g| g.preset())
                    // Nothing chosen means the stage was skipped, which the
                    // ordering guard should have prevented. Refusing beats
                    // writing a preset the player never saw.
                    .ok_or_else(|| {
                        "no graphics preset is recorded, so there is nothing to write. \
                         Answer the graphics step first."
                            .to_string()
                    })?;

                // FOUND, not asked for. On Windows the emulator ships beside
                // the launcher and there is one right answer, so a screen that
                // asked was a screen nobody could get past.
                // The same search a launch makes, hint cleaning included, so
                // first run cannot find a different emulator or image from
                // the one every later launch will use.
                let (found, game_path) = find_runtime_and_game(
                    &p,
                    runtime.as_deref(),
                    game.as_deref(),
                    legacy_root.as_deref().map(Path::new),
                )?;
                let runtime_exe = found.path().to_path_buf();

                let req = LaunchRequest {
                    runtime_exe,
                    game_path,
                    graphics: preset,
                    fullscreen: true,
                    build: GameBuild::Offline,
                    mods_disabled: false,
                    // Find the folder that already exists. Creating a fresh one
                    // here is the bug that booted the game to factory defaults
                    // with none of the player's saves.
                    user_dir: selected_user_dir(&p, None)?,
                    league_match: false,
                };

                // The same call a launch makes, so first run cannot configure
                // the machine differently from the way it will be configured
                // every time afterwards.
                let written = Session::prepare(&p, &req).map_err(|e| e.to_string())?;

                let files: Vec<String> = written
                    .files
                    .iter()
                    .chain(written.unchanged.iter())
                    .map(|f| f.display().to_string())
                    .collect();

                let did = match target {
                    PrepTask::WriteConfig => format!(
                        "set {} key(s) across {} file(s), {} already correct. {} {}",
                        written.keys_set,
                        written.files.len(),
                        written.unchanged.len(),
                        written.user_dir.explain(),
                        // Named every time. A stock emulator boots the game, so
                        // nothing visibly breaks and nothing would otherwise say
                        // that half the launcher's features are inert.
                        found.explain()
                    ),
                    // Running it a second time IS the check: prepare only
                    // writes what differs, so a second pass that changes
                    // nothing proves the first one landed. Anything else means
                    // something is rewriting the config behind us.
                    PrepTask::CheckEverything => {
                        if written.keys_set == 0 {
                            format!(
                                "checked {} file(s), every value already correct",
                                written.unchanged.len()
                            )
                        } else {
                            format!(
                                "PROBLEM: {} key(s) had to be set again, so something \
                                 else is changing the configuration",
                                written.keys_set
                            )
                        }
                    }
                };
                (did, files)
            }
        };

        // Only now.
        let mut flow = open_flow(legacy_root)?;
        let advance = flow.record_task(target).map_err(|e| e.to_string())?;

        Ok(PrepRunView {
            step: step_view(&flow, advance),
            task: target.slug(),
            label: target.label(),
            did,
            wrote,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Mark one preparation task finished.
///
/// This is the door that writes, so the core refuses it while an earlier stage
/// is unanswered. That refusal is passed straight through rather than smoothed
/// over: it means something was about to be applied that the player was never
/// asked about.
#[tauri::command]
async fn onboarding_record_task(
    task: String,
    legacy_root: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = prep_task_from_slug(&task)?;
        let mut flow = open_flow(legacy_root)?;
        let advance = flow.record_task(target).map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Report preparation finished. Refused while any task is outstanding.
#[tauri::command]
async fn onboarding_prepared(legacy_root: Option<String>) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut flow = open_flow(legacy_root)?;
        let advance = flow
            .satisfy(Stage::Preparation, Satisfaction::Prepared)
            .map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Latch first run as complete. Once only, and refused before the player has
/// been shown what was decided.
///
/// Takes a confirm for the same reason the destructive commands do: the latch is
/// one way. Once it is set, onboarding will not offer any of these screens
/// again, so a stray call costs the player the chance to finish setting up.
#[tauri::command]
async fn onboarding_finish(
    confirm: bool,
    legacy_root: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        confirmed(
            confirm,
            "onboarding_finish",
            "latch first run as complete, which cannot be undone",
        )?;
        let mut flow = open_flow(legacy_root)?;
        let advance = flow.finish().map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct OnboardingBackView {
    /// Where Back goes from the named stage, or absent when it cannot go there.
    to: Option<&'static str>,
    /// Why there is no way back, in the core's words. An irreversible previous
    /// stage is the usual reason, and the sentence says so rather than leaving
    /// the interface to grey a button with no explanation.
    refused_because: Option<String>,
}

/// Where Back would go from a stage, changing nothing.
///
/// A query, not a move, which is why it takes the stage the interface is
/// showing. Onboarding has no cursor to rewind: the journal is what happened,
/// and going back means showing an earlier screen again, so the interface does
/// that itself by calling `onboarding_reach`. Asking first is how a Back button
/// gets drawn only when it leads somewhere.
#[tauri::command]
async fn onboarding_back(
    stage: String,
    legacy_root: Option<String>,
) -> Result<OnboardingBackView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let from = stage_from_slug(&stage)?;
        let flow = open_flow(legacy_root)?;
        Ok(match flow.back_from(from) {
            Back::To(s) => OnboardingBackView {
                to: Some(s.slug()),
                refused_because: None,
            },
            Back::Refused(why) => OnboardingBackView {
                to: None,
                refused_because: Some(why.message()),
            },
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct OnboardingRestartView {
    /// Where the old journal went. Absent when there was nothing to archive.
    archived_to: Option<String>,
    entries_archived: usize,
    /// True when a finished setup was just undone.
    was_complete: bool,
    /// Irreversible stages whose work will be repeated, by slug. These copy
    /// files and install the runtime, so this is the cost of starting over.
    will_be_redone: Vec<&'static str>,
    next: NextStageView,
}

/// Discard first run and offer it again from the beginning.
///
/// The only way past the completion latch. Destructive in the sense that
/// matters: the irreversible stages will be offered again and their work
/// repeated, so it takes a confirm. The old journal is archived rather than
/// deleted, and the report says where.
#[tauri::command]
async fn onboarding_start_over(
    confirm: bool,
    legacy_root: Option<String>,
) -> Result<OnboardingRestartView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        confirmed(
            confirm,
            "onboarding_start_over",
            "discard first run, so every stage is offered again and the stages that copy files repeat their work",
        )?;
        let mut flow = open_flow(legacy_root)?;
        let restart = flow.start_over().map_err(|e| e.to_string())?;
        Ok(OnboardingRestartView {
            archived_to: restart.archived_to.map(|p| p.display().to_string()),
            entries_archived: restart.entries_archived,
            was_complete: restart.was_complete,
            will_be_redone: restart.will_be_redone.iter().map(|s| s.slug()).collect(),
            next: next_view(&flow.next()),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Leave the 2.1 install where it is.
///
/// Writes nothing but a journal line, which is why it needs no confirm. It also
/// makes the copy stage moot rather than removing it, so the step numbers do not
/// shift under the player and the skipped step can still be shown as skipped.
#[tauri::command]
async fn onboarding_decline_migration(
    legacy_root: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut flow = open_flow(legacy_root)?;
        let advance = flow
            .satisfy(Stage::MigrationOffer, Satisfaction::MigrationDeclined)
            .map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Choose to bring the 2.1 install over.
///
/// Records the decision and the source. Copies nothing: the source is carried
/// into the journal so the copy stage cannot later be pointed at a folder the
/// player was never shown, and the core checks it against the install actually
/// found rather than trusting the argument.
///
/// The copy itself is `migration_apply`, which takes its own confirm.
#[tauri::command]
async fn onboarding_accept_migration(
    legacy_root: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = legacy_root.map(PathBuf::from).or_else(|| default_legacy_root(&Platform::detect()))
            .ok_or("Choose your previous installation folder before copying saves.")?;
        let mut flow = open_flow(Some(source.display().to_string()))?;
        let advance = flow
            .satisfy(Stage::MigrationOffer, Satisfaction::Migrating { source })
            .map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Slug to stage, so the interface names stages the way the journal does.
///
/// An unknown name lists the real ones. A typo in a stage name would otherwise
/// look like a stage that does not apply to this machine.
fn stage_from_slug(slug: &str) -> Result<Stage, String> {
    Stage::ALL
        .iter()
        .copied()
        .find(|s| s.slug() == slug)
        .ok_or_else(|| {
            format!(
                "no onboarding stage called {slug}. Known: {}",
                Stage::ALL
                    .iter()
                    .map(|s| s.slug())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

fn prep_task_from_slug(slug: &str) -> Result<PrepTask, String> {
    PrepTask::ALL
        .iter()
        .copied()
        .find(|t| t.slug() == slug)
        .ok_or_else(|| {
            format!(
                "no preparation task called {slug}. Known: {}",
                PrepTask::ALL
                    .iter()
                    .map(|t| t.slug())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}


/// Removable drives the Xbox wizard may write to, and why any were refused.
///
/// Nothing is preselected, even when exactly one drive is present. That rule
/// lives in the core and this command does not soften it: the wizard copies
/// about 3 GB, and a fixed disk offered once is that payload written into the
/// middle of somebody's system drive.
#[tauri::command]
async fn removable_drives() -> Result<DrivesView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let volumes = p.drives.enumerate_volumes().map_err(|e| e.to_string())?;
        let removable = volumes.iter().filter(|v| v.is_removable()).count();
        let survey = reborn_core::xbox::survey(&p).map_err(|e| e.to_string())?;

        Ok(DrivesView {
            volumes_seen: volumes.len(),
            removable_seen: removable,
            fixed_seen: volumes.len() - removable,
            offers: survey
                .offers
                .iter()
                .map(|offer| DriveView {
                    mount_point: offer.drive.mount_point.display().to_string(),
                    label: offer.drive.label.clone(),
                    total_bytes: offer.drive.total_bytes,
                    free_bytes: offer.drive.free_bytes,
                    filesystem: offer.drive.filesystem.clone(),
                    volume_identity: offer.drive.volume_identity.clone(),
                    selectable: offer.is_selectable(),
                    why_not: offer.why_not(),
                })
                .collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// The results queue
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct StatsEntryView {
    key: String,
    /// `queued`, `quarantined`, `accepted`, `held`, `rejected` or `stalled`.
    state: &'static str,
    /// Why, for the states that carry a reason.
    reason: Option<String>,
    attempts: u32,
    first_seen_ms: u64,
    last_error: Option<String>,
    session_id: Option<String>,
    /// What the server said about a terminal result, such as "Recorded as
    /// incomplete". Written to the entry and shown to nobody until fix wave 2:
    /// it is the only place a player learns their game was counted with a
    /// qualification, and the state alone says "sent and counted", which is
    /// true and is not the whole truth.
    note: Option<String>,
    /// "online", "offline", "unknown", or absent for the folder older runtimes
    /// wrote. Reported rather than kept internal: "unknown" is the launcher
    /// saying it found a capture in a folder neither it nor the emulator agreed
    /// on, and a naming drift between the two has to be visible somewhere.
    capture_folder: Option<String>,
    captured_at: Option<String>,
    teams: Vec<String>,
    players: Option<usize>,
    /// Absent when the entry carries no summary. A capture whose sides could not
    /// be verified is the case where a quarterback was credited with sacks, so a
    /// screen showing these values needs to be able to mark them suspect.
    sides_verified: Option<bool>,
    needs_review: Option<bool>,
}

#[derive(Debug, Serialize, Default, PartialEq, Eq)]
struct StatsQueueCounts {
    /// What the next submission pass would actually try: `Queued` and
    /// `Stalled`. A pass puts stalled entries back before it starts, so they
    /// are as waiting as anything else, and this is the only number the
    /// settings panel shows.
    depth: usize,
    quarantined: usize,
    stalled: usize,
    accepted: usize,
    held: usize,
    rejected: usize,
    total: usize,
}

#[derive(Debug, Serialize)]
struct StatsQueueView {
    dir: String,
    #[serde(flatten)]
    counts: StatsQueueCounts,
    /// Payload files with no entry record. Reported because the alternative is
    /// bytes on disk that nothing will ever send and nothing mentions.
    orphaned_payloads: usize,
    entries: Vec<StatsEntryView>,
}

/// One queue row, as a page reads it.
///
/// Its own function so it can be checked without a `Platform`, a real queue
/// and a real disk: what a row says about a result is exactly the kind of
/// thing that has to be testable.
fn entry_view(e: &QueueEntry) -> StatsEntryView {
    let (state, reason) = entry_state_label(&e.state);
    StatsEntryView {
        key: e.key.to_string(),
        state,
        reason,
        attempts: e.attempts,
        first_seen_ms: e.first_seen_ms,
        last_error: e.last_error.clone(),
        session_id: e.session_id.clone(),
        note: e.note.clone(),
        capture_folder: e.capture_folder.clone(),
        captured_at: e.summary.as_ref().map(|s| s.captured_at.clone()),
        teams: e
            .summary
            .as_ref()
            .map(|s| s.teams.iter().map(|t| t.name.clone()).collect())
            .unwrap_or_default(),
        players: e.summary.as_ref().map(|s| s.players),
        sides_verified: e.summary.as_ref().map(|s| s.sides_verified),
        needs_review: e.summary.as_ref().map(|s| s.needs_review),
    }
}

fn entry_state_label(state: &EntryState) -> (&'static str, Option<String>) {
    match state {
        EntryState::Queued => ("queued", None),
        EntryState::Quarantined { reason } => ("quarantined", Some(reason.message())),
        EntryState::Accepted => ("accepted", None),
        EntryState::Held { reason } => ("held", Some(reason.clone())),
        EntryState::Rejected { reason } => ("rejected", Some(reason.clone())),
        EntryState::Stalled { reason } => ("stalled", Some(reason.clone())),
    }
}

/// Count the queue by state.
///
/// `depth` is the number of entries a submission pass would try: `Queued` and
/// `Stalled`. Stalled is in there because a pass puts stalled entries back
/// before it starts, so they will be tried; leaving them out meant the one
/// number a player sees read zero while real games sat undelivered.
///
/// `Held` is NOT in there. It means the server stored the result and a person
/// will decide, which is a **success**: counting it as waiting is precisely
/// the mistake that sent the uploader into a retry loop and produced a real
/// outage on this project.
fn queue_counts(entries: &[QueueEntry]) -> StatsQueueCounts {
    let mut counts = StatsQueueCounts {
        total: entries.len(),
        ..Default::default()
    };
    for entry in entries {
        match entry.state {
            EntryState::Queued => counts.depth += 1,
            EntryState::Quarantined { .. } => counts.quarantined += 1,
            EntryState::Stalled { .. } => {
                counts.stalled += 1;
                counts.depth += 1;
            }
            EntryState::Accepted => counts.accepted += 1,
            EntryState::Held { .. } => counts.held += 1,
            EntryState::Rejected { .. } => counts.rejected += 1,
        }
    }
    counts
}

/// How many captured results are waiting, and what state each one is in.
///
/// Read only. The ledger is applied over the entries by the core, so no caller
/// can see a state that would send a game twice.
#[tauri::command]
async fn stats_queue() -> Result<StatsQueueView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let queue = StatsQueue::open(&p).map_err(|e| e.player_message().to_string())?;
        let entries = queue.entries().map_err(|e| e.player_message().to_string())?;
        let orphaned = queue
            .orphaned_payloads()
            .map_err(|e| e.player_message().to_string())?;

        Ok(StatsQueueView {
            dir: queue.dir().display().to_string(),
            counts: queue_counts(&entries),
            orphaned_payloads: orphaned.len(),
            entries: entries.iter().map(entry_view).collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct StatsIngestView {
    /// Where the captures were read from.
    from: String,
    queued: usize,
    quarantined: usize,
    already_queued: usize,
    already_submitted: usize,
    /// Files that could not be read. These were left exactly where they were.
    unreadable: usize,
    /// Whole folders that could not be listed, so nothing in them was looked
    /// at. Counted apart from `unreadable` because how many results are in one
    /// is exactly what was not established.
    unreadable_folders: usize,
    /// Results that were saved, whose file the emulator wrote could not be
    /// removed afterwards. Nothing was lost; the file is read again next time
    /// and the content key makes that a no-op.
    source_kept: usize,
    /// One sentence per capture, from the core, so nothing here paraphrases a
    /// result the player will act on.
    messages: Vec<String>,
}

/// Move the captures Dolphin wrote into the durable queue.
///
/// Takes a confirm because it deletes: a source file is removed once its capture
/// is safely in the queue, so the folder empties and the same games are not
/// re-examined after every launch. Nothing is lost if it is interrupted, because
/// the removal happens after the write and the content key makes a re-read a
/// no-op.
///
/// Holds the single-instance lock, so this cannot run while a game is still
/// writing into that folder.
#[tauri::command]
async fn stats_ingest(
    user_dir: String,
    session_id: Option<String>,
    confirm: bool,
) -> Result<StatsIngestView, String> {
    confirmed(
        confirm,
        "stats_ingest",
        "move captured results into the queue and remove them from Dolphin's \
         capture folder",
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let from = reborn_core::stats::dolphin_capture_dir(Path::new(&user_dir));

        let Startup::Owner(_lock) =
            Supervisor::acquire(&p).map_err(|e| e.player_message().to_string())?
        else {
            return Err(already_running());
        };

        let queue = StatsQueue::open(&p).map_err(|e| e.player_message().to_string())?;
        let results = queue
            .ingest_all(Path::new(&user_dir), session_id.as_deref())
            .map_err(|e| e.player_message().to_string())?;

        let mut view = StatsIngestView {
            from: from.display().to_string(),
            queued: 0,
            quarantined: 0,
            already_queued: 0,
            already_submitted: 0,
            unreadable: 0,
            unreadable_folders: 0,
            source_kept: 0,
            messages: results.iter().map(|r| r.message()).collect(),
        };
        for result in &results {
            match result {
                Enqueued::Queued(_) => view.queued += 1,
                Enqueued::Quarantined(_) => view.quarantined += 1,
                Enqueued::AlreadyQueued(_) => view.already_queued += 1,
                Enqueued::AlreadySubmitted { .. } => view.already_submitted += 1,
                Enqueued::CouldNotRead { .. } => view.unreadable += 1,
                Enqueued::FolderCouldNotBeRead { .. } => view.unreadable_folders += 1,
                Enqueued::SourceKept { .. } => view.source_kept += 1,
            }
        }
        Ok(view)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Send everything queued to the backend. No confirmation: sending is what the queue is for,
/// and the server's idempotency makes a repeat harmless.
#[tauri::command]
async fn stats_submit() -> Result<StatsSubmitView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let url = backend_origin(&p)?;
        let queue = StatsQueue::open(&p).map_err(|e| e.player_message().to_string())?;
        // The lock is read from and given straight back: holding it across the
        // send is what told a player pressing Play that another launcher was
        // running, when the only thing running was this send.
        let history = history_without_holding_the_lock(&p)?;
        match reborn_core::backend::auto_submit(&p, &queue, &history)
            .map_err(|e| e.player_message().to_string())?
        {
            reborn_core::backend::SendAttempt::NotSignedIn => Ok(StatsSubmitView { url, accepted: 0, held: 0, rejected: 0, deferred: 0, stalled: 0, note: "Sign in with Discord to send your results. They are kept until then.".to_string() }),
            reborn_core::backend::SendAttempt::Sent(report) => Ok(StatsSubmitView {
                url, accepted: report.accepted.len() + report.recorded.len(), held: report.held.len(), rejected: report.rejected.len(), deferred: report.deferred.len(), stalled: report.stalled.len(),
                note: if report.waiting_for_sign_in { "The server no longer accepts this sign-in. Sign in again; nothing was lost.".to_string() } else { submit_note(&report) },
            }),
        }
    }).await.map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct StatsSubmitView {
    url: String,
    accepted: usize,
    /// Stored, and a person will decide. A success.
    held: usize,
    rejected: usize,
    /// Could not be delivered this time. Still queued.
    deferred: usize,
    stalled: usize,
    note: String,
}

fn submit_note(r: &reborn_core::stats::SubmitReport) -> String {
    let mut parts = Vec::new();
    let accepted = r.accepted.len() + r.recorded.len();
    if accepted > 0 {
        parts.push(format!("{accepted} accepted"));
    }
    if !r.held.is_empty() {
        parts.push(format!("{} held for commissioner review", r.held.len()));
    }
    if !r.rejected.is_empty() {
        parts.push(format!("{} refused by the server and kept on disk", r.rejected.len()));
    }
    if !r.deferred.is_empty() {
        parts.push(format!("{} could not be delivered and stay queued", r.deferred.len()));
    }
    if !r.stalled.is_empty() {
        // Not "out of attempts, still on disk", which read like a dead end.
        // The next send pass puts these back before it starts.
        parts.push(format!(
            "{} could not be delivered after several tries and will be tried again",
            r.stalled.len()
        ));
    }
    if parts.is_empty() {
        "Nothing was waiting to be sent.".to_string()
    } else {
        format!("{}.", parts.join(", "))
    }
}

fn http_client() -> reborn_core::net::UreqHttp {
    reborn_core::net::UreqHttp::new(&format!("NFL Street Reborn Launcher/{}", crate::launcher_version()))
}

/// Record whether the launcher may update on its own.
///
/// Written to the journal as the first run answer AND to settings.json,
/// which is what a launch reads. Without the second write the answer would
/// be unchangeable without re-walking the wizard, which is the trap the
/// controls profile already had to be dug out of.
#[tauri::command]
async fn onboarding_choose_updates(
    automatic: bool,
    legacy_root: Option<String>,
) -> Result<OnboardingStepView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let file = reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
        reborn_core::settings::set_automatic_updates(&file, automatic).map_err(|e| e.to_string())?;
        let mut flow = open_flow(legacy_root)?;
        let advance = flow
            .satisfy(Stage::Updates, Satisfaction::Updates { automatic })
            .map_err(|e| e.to_string())?;
        Ok(step_view(&flow, advance))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct UpdatePolicyView {
    automatic: bool,
    /// "settings" when set after first run, "first_run" when only the
    /// journal has it, "default" when nobody has answered.
    source: &'static str,
    note: String,
}

/// Whether the launcher updates on its own, and where that was decided.
#[tauri::command]
async fn update_policy() -> Result<UpdatePolicyView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let file = reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
        let stored = reborn_core::settings::Settings::load(&file)
            .map_err(|e| e.to_string())?
            .automatic_updates;
        let flow = Onboarding::open(&p, Environment::nothing_found()).map_err(|e| e.to_string())?;
        let from_first_run = flow.summary().automatic_updates;
        let (automatic, source) = match (stored, from_first_run) {
            (Some(v), _) => (v, "settings"),
            (None, Some(v)) => (v, "first_run"),
            (None, None) => (true, "default"),
        };
        let note = if automatic {
            "Updates are downloaded and applied on their own. A release-to-release patch is about a megabyte.".to_string()
        } else {
            "You are asked before an update is applied. Updates marked required are still applied, because an out of date launcher cannot play online.".to_string()
        };
        Ok(UpdatePolicyView { automatic, source, note })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct TexturePolicyView {
    managed: bool,
    note: String,
}

/// Whether the launcher may point the emulator's texture folder at the tree
/// it composes.
///
/// On for a player: it is how mods reach the game. Off for anyone who
/// AUTHORS textures, because their folder is the source of truth and the
/// composed tree is deleted and rebuilt on every sync.
#[tauri::command]
async fn texture_policy() -> Result<TexturePolicyView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let file = reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
        let managed =
            reborn_core::settings::manage_texture_folder(&file).map_err(|e| e.to_string())?;
        Ok(TexturePolicyView {
            managed,
            note: if managed {
                "The launcher points the emulator's texture folder at the mods you turned on. Whatever was there is renamed aside, never deleted.".to_string()
            } else {
                "The emulator's texture folder is left completely alone. Mods are still composed, they are just not linked in. For anyone who authors textures.".to_string()
            },
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Change it.
#[tauri::command]
async fn texture_policy_set(managed: bool) -> Result<TexturePolicyView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let file = reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
        reborn_core::settings::set_manage_texture_folder(&file, managed).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    texture_policy().await
}

/// Change it later, without re-walking first run.
#[tauri::command]
async fn update_policy_set(automatic: bool) -> Result<UpdatePolicyView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let file = reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
        reborn_core::settings::set_automatic_updates(&file, automatic).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    update_policy().await
}

fn selected_user_dir(platform: &Platform, hint: Option<&str>) -> Result<Option<PathBuf>, String> {
    if let Some(value) = hint.filter(|v| !v.trim().is_empty()) {
        return Ok(Some(reborn_core::session::user_path(value)));
    }
    let file = reborn_core::settings::settings_path(platform).map_err(|e| e.player_message().to_string())?;
    Ok(reborn_core::settings::Settings::load(&file).map_err(|e| e.player_message().to_string())?.user_dir)
}

#[tauri::command]
async fn settings_paths() -> Result<serde_json::Value, String> {
    let p = Platform::detect();
    let file = reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
    let s = reborn_core::settings::Settings::load(&file).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"runtime": s.runtime_exe, "game": s.game_path,
        "userDir": s.user_dir, "legacyRoot": s.legacy_root}))
}

#[tauri::command]
async fn pick_path(kind: String) -> Result<Option<String>, String> {
    let dialog = rfd::AsyncFileDialog::new().set_title("Choose a location for NFL Street Reborn");
    let picked = match kind.as_str() {
        "folder" => dialog.pick_folder().await,
        "game" => dialog.add_filter("Game image", &["iso", "gcm", "rvz", "ciso", "wbfs"]).pick_file().await,
        "runtime" => dialog.pick_file().await,
        "memory_card" => dialog.add_filter("GameCube memory card", &["raw", "gcp"]).pick_file().await,
        "manifest" => dialog.add_filter("Component manifest", &["json"]).pick_file().await,
        "audio" => dialog.set_title("Choose a song").add_filter("Audio", reborn_core::soundtrack::decode::EXTENSIONS).pick_file().await,
        "pack" => dialog.set_title("Choose a soundtrack pack").add_filter("Soundtrack pack", &["nsrmusic", "nsmp"]).pick_file().await,
        "save_pack" => dialog.set_title("Save a soundtrack pack").add_filter("Soundtrack pack", &["nsrmusic"]).set_file_name("my-soundtrack.nsrmusic").save_file().await,
        _ => return Err("Unknown file type".into()),
    };
    Ok(picked.map(|f| f.path().display().to_string()))
}

#[derive(Debug, Serialize)]
struct PathSetView {
    which: String,
    /// What was stored, cleaned. Absent when the box was emptied, which
    /// means the launcher finds it again.
    value: Option<String>,
}

/// Record one of the three paths a person can override.
///
/// Kept on disk rather than in the window alone, so the Play and files
/// shortcuts resolve the same emulator, image and folder the Play button
/// does. An empty value clears it.
#[tauri::command]
async fn settings_set_path(which: String, value: String) -> Result<PathSetView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use reborn_core::settings::PathSetting;
        let setting = match which.as_str() {
            "runtime" => PathSetting::Runtime,
            "game" => PathSetting::Game,
            "user_dir" => PathSetting::UserDir,
            "legacy_root" => PathSetting::LegacyRoot,
            other => return Err(format!("no path setting called {other}. Known: runtime, game, user_dir, legacy_root")),
        };
        let p = Platform::detect();
        let file = reborn_core::settings::settings_path(&p).map_err(|e| e.to_string())?;
        let stored = reborn_core::settings::set_path(&file, setting, &value).map_err(|e| e.to_string())?;
        Ok(PathSetView { which, value: stored.map(|v| v.display().to_string()) })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Who the player is.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct IdentityView {
    signed_in: bool,
    display_name: Option<String>,
    player_id: Option<String>,
    show_offline: Option<bool>,
}
#[derive(Debug, Serialize)]
struct SignInStartView {
    device_code: String,
    user_code: String,
    verify_url: String,
    interval: u64,
    /// Whether the shell took the link. `home.js` no longer draws anything
    /// different either way, because it now shows the address in both cases,
    /// so nothing on screen depends on this. It is still answered, and still
    /// honest for the first time: the opener it replaced ran a file manager,
    /// which always starts, so this used to be `true` even when what opened
    /// was the player's Documents folder.
    opened: bool,
}
#[derive(Debug, Serialize)]
struct SignInPollView {
    state: &'static str,
    display_name: Option<String>,
}

fn identity_view(file: Option<&reborn_core::identity::IdentityFile>) -> IdentityView {
    match file {
        Some(f) => IdentityView { signed_in: true, display_name: Some(f.player.display_name.clone()), player_id: Some(f.player.id.clone()), show_offline: Some(f.player.show_offline) },
        None => IdentityView { signed_in: false, display_name: None, player_id: None, show_offline: None },
    }
}

/// The backend: the setting, or the product origin.
///
/// The reading and the wording both live in `reborn_core::identity`, because
/// its one failure is the launcher's own settings file refusing to open and
/// that has to become a sentence rather than a path. This window has no
/// logger to put the detail in; that module does.
fn backend_origin(p: &Platform) -> Result<String, String> {
    reborn_core::identity::origin(p).map_err(|e| e.to_string())
}

/// Read the session history, and give the launcher lock straight back.
///
/// # Why the lock cannot be held across a send
///
/// It is the single-instance lock. `Session::run` takes it and, failing, tells
/// the player "Another launcher is already running. Close it first." Both send
/// paths used to hold it for the whole network round trip, and a send is one
/// request per queued game with a two minute timeout each. So a player who
/// signed in and then pressed Play was told to close a launcher that does not
/// exist, and their game did not start.
///
/// The history is what the send needs from it: a few lines off disk, read in
/// microseconds. Two sends never overlapping is a separate promise, kept by
/// `backend::auto_submit` itself rather than by whoever happens to hold this
/// lock.
fn history_without_holding_the_lock(
    p: &Platform,
) -> Result<Vec<reborn_core::supervisor::SessionRecord>, String> {
    let Startup::Owner(sup) = Supervisor::acquire(p).map_err(|e| e.player_message().to_string())?
    else {
        return Err(already_running());
    };
    let history = sup.history().map_err(|e| e.player_message().to_string())?;
    drop(sup);
    Ok(history)
}

/// Send whatever is queued, without making anybody wait for it.
///
/// Its own thread on purpose. A send is one network round trip per queued
/// game, each with the client's full timeout, so a backlog is minutes of
/// waiting. Nothing about it belongs on the answer to a command a page is
/// polling.
///
/// Nothing here is reported. The player already knows they signed in, and a
/// send that could not happen leaves every result exactly where it was, to go
/// out at the end of the next game or when they press Send results now.
fn send_backlog_in_background() {
    std::thread::spawn(|| {
        let p = Platform::detect();
        let Ok(queue) = StatsQueue::open(&p) else { return };
        let Ok(history) = history_without_holding_the_lock(&p) else { return };
        let _ = reborn_core::backend::auto_submit(&p, &queue, &history);
    });
}

#[tauri::command]
async fn identity_status() -> Result<IdentityView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let file = reborn_core::identity::status(&p).map_err(|e| e.to_string())?;
        Ok(identity_view(file.as_ref()))
    }).await.map_err(|e| e.to_string())?
}

/// Start a Discord sign-in and open the verify page in the default browser.
#[tauri::command]
async fn identity_sign_in_start() -> Result<SignInStartView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = backend_origin(&p)?;
        let http = http_client();
        let s = reborn_core::identity::start(&http, &origin, &reborn_core::identity::machine_label()).map_err(|e| e.to_string())?;
        let opened = reborn_core::identity::open_in_browser(&p, &s.verify_url).is_ok();
        Ok(SignInStartView { device_code: s.device_code, user_code: s.user_code, verify_url: s.verify_url, interval: s.interval.max(2), opened })
    }).await.map_err(|e| e.to_string())?
}

/// Ask whether the browser step finished. On "done" the token is already in the vault.
#[tauri::command]
async fn identity_sign_in_poll(device_code: String) -> Result<SignInPollView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = backend_origin(&p)?;
        let http = http_client();
        match reborn_core::identity::poll(&http, &origin, &device_code).map_err(|e| e.to_string())? {
            reborn_core::identity::Poll::Pending => Ok(SignInPollView { state: "pending", display_name: None }),
            reborn_core::identity::Poll::Expired => Ok(SignInPollView { state: "expired", display_name: None }),
            reborn_core::identity::Poll::Done { token, player } => {
                let name = player.display_name.clone();
                reborn_core::identity::save_signed_in(&p, &token, player, now_ms()).map_err(|e| e.to_string())?;
                // The backlog goes out the moment a token exists, on its own
                // thread. It used to go out here, so this command did not
                // answer until every queued game had been posted: a network
                // round trip each, with a two minute timeout each. The window
                // polls on an interval, so the ticks that fired during that
                // wait asked about a device code the server had already
                // consumed, and the refusals they got landed on the screen
                // beside the sign-in that had just succeeded.
                send_backlog_in_background();
                Ok(SignInPollView { state: "done", display_name: Some(name) })
            }
        }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn identity_sign_out() -> Result<IdentityView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = backend_origin(&p)?;
        let http = http_client();
        reborn_core::identity::sign_out(&p, &http, &origin).map_err(|e| e.to_string())?;
        Ok(identity_view(None))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn identity_set_show_offline(show: bool) -> Result<IdentityView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = backend_origin(&p)?;
        let http = http_client();
        reborn_core::identity::set_show_offline(&p, &http, &origin, show).map_err(|e| e.to_string())?;
        let file = reborn_core::identity::status(&p).map_err(|e| e.to_string())?;
        Ok(identity_view(file.as_ref()))
    }).await.map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Updates
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct UpdateView {
    current: String,
    source: String,
    /// "up_to_date", "available", "no_asset", "unreachable", "unreadable".
    state: &'static str,
    latest: Option<String>,
    bytes: Option<u64>,
    notes: Option<String>,
    note: String,
}

fn update_source_for(p: &Platform, given: Option<&str>) -> Result<String, String> {
    if let Some(s) = given.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(s.to_string());
    }
    let file = reborn_core::settings::settings_path(p).map_err(|e| e.to_string())?;
    let settings = reborn_core::settings::Settings::load(&file).map_err(|e| e.to_string())?;
    Ok(settings.update_source.filter(|u| !u.trim().is_empty()).unwrap_or_else(|| {
        match p.paths.channel() {
            reborn_core::Channel::Beta => reborn_core::update::DEFAULT_BETA_SOURCE,
            reborn_core::Channel::Stable => reborn_core::update::DEFAULT_SOURCE,
        }
        .to_string()
    }))
}

fn update_view(source: &str, check: &reborn_core::update::UpdateCheck) -> UpdateView {
    use reborn_core::update::UpdateCheck as C;
    let current = crate::launcher_version().to_string();
    let (state, latest, bytes, notes) = match check {
        C::UpToDate { latest, .. } => ("up_to_date", Some(latest.clone()), None, None),
        C::Available { latest, asset, notes, .. } => ("available", Some(latest.clone()), Some(asset.bytes), notes.clone()),
        C::NoAssetForThisPlatform { latest, .. } => ("no_asset", Some(latest.clone()), None, None),
        C::Unreachable { .. } => ("unreachable", None, None, None),
        C::Unreadable { .. } => ("unreadable", None, None, None),
    };
    UpdateView { current, source: source.to_string(), state, latest, bytes, notes, note: check.explain() }
}

/// Ask the release page whether a newer launcher exists. Reads only.
#[tauri::command]
async fn update_check(source: Option<String>) -> Result<UpdateView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let source = update_source_for(&p, source.as_deref())?;
        let check = reborn_core::update::check(&http_client(), &source, crate::launcher_version());
        Ok(update_view(&source, &check))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct UpdateDownloadView {
    check: UpdateView,
    /// The verified installer, ready for update_apply. Absent when there was
    /// nothing to download.
    path: Option<String>,
    bytes: u64,
    sha256: Option<String>,
}

/// Download the newer installer into the downloads folder and verify it.
/// Nothing is installed. A hash mismatch deletes the file and is an error.
#[tauri::command]
async fn update_download(source: Option<String>) -> Result<UpdateDownloadView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let source = update_source_for(&p, source.as_deref())?;
        let http = http_client();
        let check = reborn_core::update::check(&http, &source, crate::launcher_version());
        let view = update_view(&source, &check);
        let reborn_core::update::UpdateCheck::Available { asset, .. } = &check else {
            return Ok(UpdateDownloadView { check: view, path: None, bytes: 0, sha256: None });
        };
        let dir = p.data_dir(DataDir::Downloads).map_err(|e| e.to_string())?;
        let got = reborn_core::update::download(&http, asset, &dir).map_err(|e| e.to_string())?;
        Ok(UpdateDownloadView {
            check: view,
            path: Some(got.path.display().to_string()),
            bytes: got.bytes,
            sha256: Some(got.sha256.as_str().to_string()),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct UpdateApplyView {
    started: bool,
    installer: String,
    note: String,
}

/// Start the downloaded installer and close the launcher so it can replace
/// the program files. The data root is not touched by the installer.
#[tauri::command]
async fn update_apply(path: String, confirm: bool) -> Result<UpdateApplyView, String> {
    confirmed(confirm, "update_apply", "start the installer and close the launcher")?;
    tauri::async_runtime::spawn_blocking(move || {
        let guard=LaunchGuard::acquire()?;
        let p = Platform::detect();
        let installer = reborn_core::session::user_path(&path);
        // Only something the launcher downloaded and verified.
        let downloads = p.data_dir(DataDir::Downloads).map_err(|e| e.to_string())?;
        if !installer.starts_with(&downloads) {
            return Err(format!(
                "{} is not in the downloads folder, so it was not verified by the launcher. \
                 Run update_download first.",
                installer.display()
            ));
        }
        reborn_core::update::apply(&p, &installer).map_err(|e| e.to_string())?;
        // Out of the installer's way. A moment for the reply to reach the page.
        std::thread::spawn(move || {
            let _guard=guard;
            std::thread::sleep(std::time::Duration::from_millis(1200));
            std::process::exit(0);
        });
        Ok(UpdateApplyView {
            started: true,
            installer: installer.display().to_string(),
            note: "The installer is running. The launcher closes now and the installer opens it again when it is done.".to_string(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OpenRuntimeView {
    started: bool,
    runtime: String,
    user_dir_note: String,
}

/// Open the emulator itself, with no game, against the user folder a launch
/// would use. For the player who wants Dolphin's own settings.
///
/// It IS a session. A player who opens the emulator here and then boots a game
/// from Dolphin's own interface has played a game, and Dolphin writes a
/// capture for it exactly as it does for a launch. This command used to spawn
/// and return, holding nothing and waiting for nothing, so those captures were
/// left on disk with no session that explained them: either never delivered,
/// or picked up by a later offline session and stamped with an id, an exit
/// reason and a league intent belonging to a game it did not produce.
///
/// So the record is written before the emulator starts, the lock and the guard
/// are held for as long as it runs, and the results are collected and sent on
/// the way out. That the launcher now reports a session as active for the
/// whole time Dolphin is open is the correct answer to "is a game running",
/// and it stops a second copy being started over the same memory card.
#[tauri::command]
async fn open_runtime(runtime: Option<String>, user_dir: Option<String>) -> Result<OpenRuntimeView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let guard = LaunchGuard::acquire()?;
        let p = Platform::detect();
        let exe = runtime_for(&p, runtime.as_deref()).ok_or_else(|| {
            "No emulator was found. Set its path on the Settings page.".to_string()
        })?;
        let chosen = selected_user_dir(&p,user_dir.as_deref())?;
        let user = reborn_core::session::resolve_user_dir_for(&p, chosen.as_deref(), Some(&exe))
            .map_err(|e| e.player_message().to_string())?;
        let args: Vec<std::ffi::OsString> = vec!["-u".into(), user.path().as_os_str().to_os_string()];
        reborn_core::runtime::ensure_runtime_idle(&exe).map_err(|e| e.player_message().to_string())?;
        reborn_core::dolphin_preferences::resolve_portable_paths(user.path()).map_err(|e| e.player_message().to_string())?;

        // The lock, then anything left unfinished, then the record. The same
        // first steps as a launch, because this ends the same way: with a
        // capture that has to belong to something.
        let Startup::Owner(supervisor) = Supervisor::acquire(&p).map_err(|e| e.player_message().to_string())? else {
            return Err(already_running());
        };
        let _ = supervisor.recover_unfinished().map_err(|e| e.player_message().to_string())?;
        // Offline, and not a league match: nothing was asked, and the emulator
        // is being opened on its own rather than pointed at a fixture. The
        // folder Dolphin writes into still says whether the game was online.
        let record = supervisor
            .begin_with_intent(GameBuild::Offline, Some(SessionIntent { league_match: false }))
            .map_err(|e| e.player_message().to_string())?;

        let env = std::collections::BTreeMap::new();
        let handle = p.processes
            .spawn(&exe, &args, exe.parent(), &env)
            .map_err(|e| e.player_message().to_string())?;
        let user_path = user.path().to_path_buf();
        std::thread::spawn(move || {
            // Held across the wait and across the collection, which is what
            // the lock is for: the queue and the session log are shared state.
            let _guard = guard;
            let exit = p.processes.wait(handle).ok();
            let collected = reborn_core::session::finish_and_collect(&p, &supervisor, record, exit.as_ref(), &user_path);
            // Both given back before the network work. A send is one request
            // per queued game with the client's full timeout each, so against
            // a slow or unreachable backend it is minutes, and for those
            // minutes pressing Play said a game session was already active and
            // the Online page said to close the emulator. The emulator had
            // already exited: both were false.
            drop(supervisor);
            drop(_guard);
            // Never surfaced: the window was told the emulator started, and a
            // game that was played is not a failure however sending it went.
            let _ = collected.send(&p);
        });
        Ok(OpenRuntimeView { started: true, runtime: exe.display().to_string(), user_dir_note: user.explain() })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
struct DiagnosticBundleView {
    folder: String,
    files: Vec<String>,
    skipped: Vec<String>,
}

/// Gather what a person would need to look at a bad session into one folder
/// under recovery/, and name what it holds. Copies only: nothing is moved.
/// No token, no credential, and no memory card ever goes in.
#[tauri::command]
async fn diagnostic_bundle(runtime: Option<String>) -> Result<DiagnosticBundleView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let stamp = now_ms();
        let folder = p
            .data_dir(DataDir::Recovery)
            .map_err(|e| e.to_string())?
            .join(format!("diagnostics-{stamp}"));
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;

        let mut files = Vec::new();
        let mut skipped = Vec::new();
        let mut take = |from: PathBuf, name: &str| match std::fs::copy(&from, folder.join(name)) {
            Ok(_) => files.push(name.to_string()),
            Err(_) => skipped.push(format!("{name} (not at {})", from.display())),
        };

        let data = p.paths.data_root().map_err(|e| e.to_string())?;
        // Both live under config/, not at the root. Guessing the root meant the
        // bundle quietly reported the two most useful files as missing.
        let config = data.join(DataDir::Config.relative());
        take(config.join("settings.json"), "settings.json");
        take(config.join("onboarding.jsonl"), "onboarding.jsonl");
        take(data.join("mods").join("activation.json"), "activation.json");
        take(
            data.join("mods").join("active").join(GameBuild::Offline.game_id()).join("composed.json"),
            "composed.json",
        );

        let exe = runtime_for(&p, runtime.as_deref());
        if let Ok(user) = reborn_core::session::resolve_user_dir_for(&p, None, exe.as_deref()) {
            let config = user.path().join("Config");
            for name in ["Dolphin.ini", "GFX.ini", "GCPadNew.ini"] {
                take(config.join(name), name);
            }
            take(user.path().join("Logs").join("dolphin.log"), "dolphin.log");
        }

        // The launcher's own log, which is where every failure it decided not
        // to show a player went. The bundle could only ever collect the
        // emulator's, because until fix wave 4 the launcher did not have one.
        let logs = data.join(DataDir::Logs.relative());
        take(
            logs.join(reborn_core::logging::LOG_FILE),
            reborn_core::logging::LOG_FILE,
        );
        take(
            logs.join(reborn_core::logging::PREVIOUS_LOG_FILE),
            reborn_core::logging::PREVIOUS_LOG_FILE,
        );

        // What the launcher itself knows, as text.
        let mut status = String::new();
        status.push_str(&format!("launcher {}\n", crate::launcher_version()));
        status.push_str(&format!("channel {}\n", reborn_core::Channel::detect().explain()));
        status.push_str(&format!("data root {}\n", data.display()));
        if let Some(exe) = &exe {
            status.push_str(&format!("emulator {}\n", exe.display()));
        }
        if let Startup::Owner(s) = Supervisor::acquire(&p).map_err(|e| e.to_string())? {
            if let Ok(history) = s.history() {
                status.push_str(&format!("sessions {}\n", history.len()));
                for r in history.iter().rev().take(10) {
                    status.push_str(&format!("  {} {:?} {:?}\n", r.id, r.build, r.end));
                }
            }
        }
        match std::fs::write(folder.join("status.txt"), status) {
            Ok(()) => files.push("status.txt".to_string()),
            Err(e) => skipped.push(format!("status.txt ({e})")),
        }

        Ok(DiagnosticBundleView { folder: folder.display().to_string(), files, skipped })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Music
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// Content and updates
// ---------------------------------------------------------------------------

/// Refuse a component name that is not one of the seven that exist.
///
/// An unknown name is a typo or a tampered file, and answering it with "nothing
/// is installed" would be a plausible-looking result for a component the
/// launcher has never heard of.
fn known_component(name: &str) -> Result<(), String> {
    if ComponentId::parse(name).is_some() {
        return Ok(());
    }
    let known: Vec<&str> = ComponentId::ALL.iter().map(|id| id.as_str()).collect();
    Err(format!(
        "{name:?} is not a component. The seven are: {}.",
        known.join(", ")
    ))
}

#[derive(Debug, Serialize)]
struct InstalledVersionView {
    version: String,
    /// Carried so replay protection survives a restart. Version strings are not
    /// ordered by anything a client can trust.
    sequence: u64,
}

#[derive(Debug, Serialize)]
struct ContentInstalledView {
    component: String,
    /// Absent means nothing is installed. There is deliberately no default: a
    /// fabricated `0.0.0` would satisfy the type and then be shown to a player
    /// as though the launcher knew something it does not.
    installed: Option<InstalledVersionView>,
    /// What a rollback would go back to. Absent after a first install and after
    /// a rollback, because exactly one prior version is retained and naming a
    /// version whose files are gone would be a lie the player acts on.
    previous: Option<InstalledVersionView>,
    /// In-flight operations found in the journal, meaning an update was
    /// interrupted. Resolved by `content_recover`, which is not exposed yet.
    interrupted: usize,
}

/// Which version of a component is live.
#[tauri::command]
async fn content_installed(component: String) -> Result<ContentInstalledView, String> {
    known_component(&component)?;
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let service = ContentService::for_platform(&p).map_err(|e| e.to_string())?;
        let pointer = service.installed(&component).map_err(|e| e.to_string())?;
        let journal = reborn_core::content::Journal::load(&service.layout().journal_path());

        Ok(ContentInstalledView {
            component: component.clone(),
            installed: pointer.as_ref().map(|p| InstalledVersionView {
                version: p.current.version.clone(),
                sequence: p.current.sequence,
            }),
            previous: pointer.as_ref().and_then(|p| {
                p.previous.as_ref().map(|v| InstalledVersionView {
                    version: v.version.clone(),
                    sequence: v.sequence,
                })
            }),
            interrupted: journal
                .interrupted()
                .iter()
                .filter(|op| op.component == component)
                .count(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Check for and install an update.
///
/// Refuses. `nsr.release.v1` is the only signed object in the system and
/// `TrustedRelease` cannot be constructed without a signature that verified
/// over the exact bytes served. No verifier is implemented, on purpose: a
/// built-in one that returned true would make every call site look correct while
/// nothing was checked. There is also no release URL and no blob source in this
/// crate.
///
/// Exposed so an Updates screen can say "not built" with the reason, rather than
/// drawing an empty list that implies the check ran and found nothing.
#[tauri::command]
async fn content_update(component: String, confirm: bool) -> Result<ContentInstalledView, String> {
    confirmed(
        confirm,
        "content_update",
        "download and promote new files over the installed ones",
    )?;
    known_component(&component)?;
    Err(format!(
        "Cannot update {component}: no signature verifier and no release source \
         exist on this machine. A release manifest cannot be opened without a \
         verified signature, and none is implemented while key custody is \
         unresolved. content_installed still reports what is live."
    ))
}

// ---------------------------------------------------------------------------
// Driving the launcher with a pad
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct PadButtonView {
    /// The stable key, for example `left_bumper`.
    button: &'static str,
    /// What to print in the hint strip for the connected pad's family.
    label: &'static str,
    /// Context key to action, for example `page` to `activate`. A context that
    /// binds nothing is absent rather than present and empty: an unbound slot
    /// shows nothing at all, because a player who presses X because the strip
    /// said "Options" and gets nothing has been lied to.
    actions: std::collections::BTreeMap<&'static str, String>,
}

#[derive(Debug, Serialize)]
struct DestructiveActionView {
    slug: &'static str,
    /// The wording from the specification, so the product and the document say
    /// the same thing.
    describe: &'static str,
}

#[derive(Debug, Serialize)]
struct GamepadContractView {
    /// The family the labels were resolved for, so a caller that sent something
    /// unrecognised can see what it got instead.
    family: &'static str,
    buttons: Vec<PadButtonView>,
    /// The five things, and only these five, that may sit behind a hold. Closed
    /// on purpose: a hold gate that spreads onto every button trains players to
    /// hold everything and is then meaningless where it matters.
    destructive: Vec<DestructiveActionView>,
    hold_to_confirm_ms: u32,
    /// Wait after the first step before repeating at all.
    repeat_initial_delay_ms: u32,
    repeat_interval_ms: u32,
    /// The poll gap above which a hold is no longer considered observed. A hold
    /// that spanned a stall must not be credited.
    stall_gap_ms: u64,
}

fn pad_button_slug(button: PadButton) -> &'static str {
    match button {
        PadButton::A => "a",
        PadButton::B => "b",
        PadButton::X => "x",
        PadButton::Y => "y",
        PadButton::LeftBumper => "left_bumper",
        PadButton::RightBumper => "right_bumper",
        PadButton::LeftTrigger => "left_trigger",
        PadButton::RightTrigger => "right_trigger",
        PadButton::View => "view",
        PadButton::Menu => "menu",
        PadButton::Guide => "guide",
        PadButton::DPadUp => "dpad_up",
        PadButton::DPadDown => "dpad_down",
        PadButton::DPadLeft => "dpad_left",
        PadButton::DPadRight => "dpad_right",
    }
}

fn direction_slug(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Down => "down",
        Direction::Left => "left",
        Direction::Right => "right",
    }
}

fn nav_action_slug(action: NavAction) -> String {
    match action {
        NavAction::Activate => "activate".to_string(),
        NavAction::Back => "back".to_string(),
        NavAction::Contextual(ContextualSlot::X) => "contextual_x".to_string(),
        NavAction::Contextual(ContextualSlot::Y) => "contextual_y".to_string(),
        NavAction::Section(Step::Previous) => "section_previous".to_string(),
        NavAction::Section(Step::Next) => "section_next".to_string(),
        NavAction::Page(Step::Previous) => "page_previous".to_string(),
        NavAction::Page(Step::Next) => "page_next".to_string(),
        NavAction::MoveFocus(d) => format!("move_focus_{}", direction_slug(d)),
        NavAction::OpenSearch => "open_search".to_string(),
        NavAction::OpenOverflow => "open_overflow".to_string(),
    }
}

fn destructive_slug(what: Destructive) -> &'static str {
    match what {
        Destructive::DeleteMod => "delete_mod",
        Destructive::RestoreSoundtrack => "restore_soundtrack",
        Destructive::ResetSettingsGroup => "reset_settings_group",
        Destructive::FormatUsbDrive => "format_usb_drive",
        Destructive::DeleteAccountData => "delete_account_data",
    }
}

fn family_from(name: &str) -> DeviceFamily {
    match name.to_lowercase().as_str() {
        "playstation" => DeviceFamily::PlayStation,
        "nintendo" => DeviceFamily::Nintendo,
        "steamdeck" | "steam_deck" => DeviceFamily::SteamDeck,
        "gamecube" | "gamecubeadapter" | "gamecube_adapter" => DeviceFamily::GameCubeAdapter,
        "keyboard" => DeviceFamily::Keyboard,
        "xbox" => DeviceFamily::Xbox,
        _ => DeviceFamily::Generic,
    }
}

/// The whole pad navigation contract, so the window and the core cannot
/// disagree about what a button does.
///
/// Pure and synchronous: every value here is a constant or a table lookup in the
/// core, so there is nothing to block on.
///
/// The launcher's own navigation is not the emulator's controller binding.
/// Nothing here is written to disk for the game.
#[tauri::command]
fn gamepad_contract(family: Option<String>) -> Result<GamepadContractView, String> {
    let family = family
        .as_deref()
        .map(family_from)
        .unwrap_or(DeviceFamily::Generic);
    let contexts = [
        ("page", InputContext::Page),
        ("sheet", InputContext::Sheet),
        ("dialog", InputContext::Dialog),
        ("confirm", InputContext::Confirm),
        ("blocking", InputContext::Blocking),
    ];

    let buttons = PadButton::ALL
        .iter()
        .map(|button| {
            let mut actions = std::collections::BTreeMap::new();
            for (key, context) in contexts {
                if let Some(action) = action_for(*button, context) {
                    actions.insert(key, nav_action_slug(action));
                }
            }
            PadButtonView {
                button: pad_button_slug(*button),
                label: button.label(family),
                actions,
            }
        })
        .collect();

    let timing = RepeatTiming::default().clamped();
    Ok(GamepadContractView {
        family: family.label(),
        buttons,
        destructive: Destructive::ALL
            .iter()
            .map(|d| DestructiveActionView {
                slug: destructive_slug(*d),
                describe: d.describe(),
            })
            .collect(),
        hold_to_confirm_ms: reborn_core::gamepad::HOLD_TO_CONFIRM_MS,
        repeat_initial_delay_ms: timing.initial_delay_ms,
        repeat_interval_ms: timing.interval_ms,
        stall_gap_ms: reborn_core::gamepad::STALL_GAP_MS,
    })
}

// ---------------------------------------------------------------------------
// Reading a key out of an INI the launcher does not own
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct IniValueView {
    path: String,
    /// False when there is no such file. Different from a missing key, and a
    /// Settings page that conflates them tells the player their setting is
    /// unset when their whole config is somewhere else.
    file_exists: bool,
    section: String,
    key: String,
    /// Absent when the file has no such key. Never an empty string, because
    /// `key=` is a real value and a real setting.
    value: Option<String>,
}

/// Read one key out of an INI file, changing nothing.
///
/// The launcher writes keys and never files, so a config it has touched still
/// holds everything it has no opinion about. This is how a screen shows one of
/// those values without the launcher claiming to own it. A comment is never read
/// as a pair, so a commented-out setting does not appear here as active.
#[tauri::command]
async fn ini_value(path: String, section: String, key: String) -> Result<IniValueView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let file = PathBuf::from(&path);
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(IniValueView {
                    path: file.display().to_string(),
                    file_exists: false,
                    section,
                    key,
                    value: None,
                })
            }
            Err(e) => return Err(format!("{} could not be read: {e}", file.display())),
        };
        let document = IniDocument::parse(&text);
        Ok(IniValueView {
            value: document.get(&section, &key).map(str::to_string),
            path: file.display().to_string(),
            file_exists: true,
            section,
            key,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// The player's real JSGME mod library
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct JsgmeSkippedView {
    mirror_relative: String,
    bytes: u64,
    why: String,
}

#[derive(Debug, Serialize)]
struct JsgmeModView {
    /// The folder name, verbatim. "Patriots Throwback" is 19 characters and
    /// "Aqua Football" is 13, and a row that only works at one length is a row
    /// that breaks on contact.
    name: String,
    /// A slug of the folder name, namespaced so an imported mod can never
    /// collide with a curated package of the same name.
    id: String,
    folder: String,
    /// The builds this mod actually carries payload for, read off the payload
    /// rather than off a directory name.
    targets: Vec<String>,
    files: usize,
    bytes: u64,
    /// Files in the folder that cannot become payload entries, each with the
    /// reason. Reported because a mod that imports 19 of 20 files without
    /// saying so is a mod that half works.
    skipped: Vec<JsgmeSkippedView>,
    /// Entries that are neither a plain file nor a directory. A symlink in here
    /// would make a walk follow itself.
    unsupported: Vec<String>,
    /// True when JSGME's install log says this mod is currently applied.
    applied: bool,
}

#[derive(Debug, Serialize)]
struct JsgmeOverlapView {
    relative: String,
    /// Every mod that writes this path, not a pair. "7 mods write this file" is
    /// the useful sentence, and JSGME's own answer is to copy in order and let
    /// the last one win with nothing said to the player.
    mods: Vec<String>,
    /// True when the mods disagree about letter case, which is two files on
    /// Linux and one on Windows.
    differing_case: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct JsgmeAppliedModView {
    name: String,
    recorded_paths: usize,
    backup_paths: usize,
    /// Paths the log records with nothing in `!BACKUP`. Not proof of a lost
    /// original: JSGME only stashes a file it displaced, so a mod that only
    /// adds files leaves no backup at all.
    recorded_without_backup: usize,
    /// False when the log names a mod whose folder is gone.
    folder_present: bool,
    unreadable: Option<String>,
}

#[derive(Debug, Serialize)]
struct JsgmeLibraryView {
    root: String,
    mods: Vec<JsgmeModView>,
    /// Contested paths across the whole library. Measured on this machine: 20,
    /// because the seven football mods all write one texture and the two control
    /// mods write the same 19 button prompts.
    overlaps: Vec<JsgmeOverlapView>,
    nothing_applied: bool,
    instlogs_exists: bool,
    backup_exists: bool,
    backup_entries: usize,
    applied: Vec<JsgmeAppliedModView>,
    /// Set when mods are applied and the originals cannot be restored. This is
    /// the state migration refuses to run through, and it is a narrower check
    /// than "some log has no backup".
    blocker: Option<String>,
    /// The applied order, from `JSGME.ini`.
    applied_order: Vec<String>,
    order_unreadable: Option<String>,
    /// Folder names that produce the same id, which would make one mod turn the
    /// other off.
    duplicate_ids: Vec<String>,
    /// Folders whose name cannot become an id, with the reason.
    unusable_names: Vec<MigrationSkipView>,
    /// Entries under `MODS` that are not mods, JSGME's own bookkeeping aside.
    ignored: Vec<String>,
}

/// Read the player's JSGME mod library and describe it in composition's terms.
///
/// Read only. This module never writes: `MODS` is the only copy of the mod
/// payloads, and `MODS/!BACKUP` is the only copy of any original they displaced.
///
/// `games` is required and is not defaulted. A folder under `Textures/` is not
/// necessarily a build: `Xbox Controls` carries `User/Load/Textures/v2/`, and
/// `v2` parses as a game id on shape alone, so a scan that trusted the
/// directory name would publish a mod claiming to target a build that does not
/// exist. The three ids that exist today are `GN7E69`, `FR4N31` and `CR4E12`,
/// and the caller says which of them this install actually has.
///
/// Blocking: every file in every mod is hashed, and the real library is 11 mods.
#[tauri::command]
async fn jsgme_library(mods_root: String, games: Vec<String>) -> Result<JsgmeLibraryView, String> {
    if games.is_empty() {
        return Err(
            "jsgme_library needs the list of builds this install has, because a \
             folder under Textures is not necessarily a build: `v2` parses as a \
             game id and is not one. Pass the ids you have, for example GN7E69 \
             for the offline build."
                .to_string(),
        );
    }
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(mods_root);
        let mut recognized = Vec::new();
        for game in &games {
            recognized.push(GameId::parse(game).map_err(|e| e.to_string())?);
        }

        let library = reborn_core::jsgme::JsgmeLibrary::scan(&root, &recognized)
            .map_err(|e| e.to_string())?;
        let applied_names: Vec<&str> = library
            .applied
            .applied
            .iter()
            .map(|a| a.name.as_str())
            .collect();

        Ok(JsgmeLibraryView {
            root: library.root.display().to_string(),
            mods: library
                .mods
                .iter()
                .map(|m| JsgmeModView {
                    name: m.name.clone(),
                    id: m.id.to_string(),
                    folder: m.folder.display().to_string(),
                    targets: m.targets().iter().map(|g| g.to_string()).collect(),
                    files: m.files.len(),
                    bytes: m.total_bytes(),
                    skipped: m
                        .skipped
                        .iter()
                        .map(|s| JsgmeSkippedView {
                            mirror_relative: s.mirror_relative.clone(),
                            bytes: s.bytes,
                            why: s.why.message(),
                        })
                        .collect(),
                    unsupported: m
                        .unsupported
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect(),
                    applied: applied_names
                        .iter()
                        .any(|a| a.eq_ignore_ascii_case(&m.name)),
                })
                .collect(),
            overlaps: library
                .overlaps
                .iter()
                .map(|o| JsgmeOverlapView {
                    relative: o.relative.clone(),
                    mods: o.mods.clone(),
                    differing_case: o.differing_case,
                    message: o.message(),
                })
                .collect(),
            nothing_applied: library.applied.nothing_applied(),
            instlogs_exists: library.applied.instlogs_exists,
            backup_exists: library.applied.backup_exists,
            backup_entries: library.applied.backup_entries.len(),
            applied: library
                .applied
                .applied
                .iter()
                .map(|a| JsgmeAppliedModView {
                    name: a.name.clone(),
                    recorded_paths: a.recorded_paths.len(),
                    backup_paths: a.backup_paths.len(),
                    recorded_without_backup: a.recorded_without_backup().len(),
                    folder_present: a.folder_present,
                    unreadable: a.unreadable.clone(),
                })
                .collect(),
            blocker: library.applied.blocker(),
            applied_order: library
                .applied_order
                .iter()
                .map(|(name, index)| format!("{index}: {name}"))
                .collect(),
            order_unreadable: library.order_unreadable.clone(),
            duplicate_ids: library
                .duplicate_ids
                .iter()
                .map(|(id, folders)| format!("{id}: {}", folders.join(", ")))
                .collect(),
            unusable_names: library
                .unusable_names
                .iter()
                .map(|(name, why)| MigrationSkipView {
                    what: name.clone(),
                    why: why.clone(),
                })
                .collect(),
            ignored: library
                .ignored
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// The component identities
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ComponentView {
    /// The name that goes in a manifest and in a release's component map.
    id: &'static str,
    /// Whether the player is allowed to change these files locally. True for
    /// `offline-content` only, which is what keeps a rebuilt soundtrack from
    /// being treated as corruption.
    customizable: bool,
    /// Where the files live, relative to the data root. Absent for the three
    /// where it does not follow from the name alone: `textures` needs a game id,
    /// `launcher` installs into the install root, and `controller-db` has no
    /// artifact yet.
    install_root: Option<&'static str>,
}

/// The seven components that exist, so nothing has to guess a name.
///
/// Pure: this is the core's own list, not a copy of it. Two design documents
/// say eight; the eighth was news and configuration, which is not a content
/// component.
#[tauri::command]
fn components() -> Result<Vec<ComponentView>, String> {
    Ok(ComponentId::ALL
        .iter()
        .map(|id| ComponentView {
            id: id.as_str(),
            customizable: id.is_customizable(),
            install_root: id.install_root(),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Stored credentials
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct CredentialView {
    account: String,
    /// Whether a secret is stored for this account.
    ///
    /// There is deliberately no field carrying the secret. The whole reason this
    /// seam exists is that the shipped Dolphin has a bearer token compiled into
    /// it, readable by anyone with the executable; handing the same token to a
    /// web view would move the problem rather than fix it. The window needs to
    /// know whether the player is signed in, and nothing more.
    stored: bool,
}

/// Whether a secret is stored for an account, without returning it.
///
/// Blocking: this asks the operating system's credential store, which on Windows
/// is Credential Manager and is a real inter-process call.
#[tauri::command]
async fn credential_stored(account: String) -> Result<CredentialView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let stored = p
            .credentials
            .retrieve(&account)
            .map_err(|e| e.to_string())?
            .is_some();
        Ok(CredentialView { account, stored })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Starting without the window
// ---------------------------------------------------------------------------

/// The paths a person set, or nothing, which means find them.
fn saved_paths(platform: &Platform) -> Result<reborn_core::settings::Settings, String> {
    let file = reborn_core::settings::settings_path(platform).map_err(|e| e.player_message().to_string())?;
    reborn_core::settings::Settings::load(&file).map_err(|e| e.player_message().to_string())
}

/// What the shortcuts ask for.
///
/// Both go through the launcher rather than around it. A shortcut straight to
/// Dolphin would boot a game with no mods composed, no configuration written
/// and no match results collected, and it would look like it worked.
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    /// The window, as normal.
    Window,
    /// Start the game and wait for it, showing no window of our own.
    Play,
    /// Open the folder holding the player's saves and textures.
    Folder,
    /// An invite link a friend sent (`nflstreetreborn://join/<code>`): remember
    /// the room code, then open the window on the Online page.
    Join(String),
}

fn mode_from(args: impl Iterator<Item = String>) -> Mode {
    for a in args {
        match a.as_str() {
            "--play" => return Mode::Play,
            "--folder" => return Mode::Folder,
            other => {
                if let Some(code) = join_code_from(other) {
                    return Mode::Join(code);
                }
            }
        }
    }
    Mode::Window
}

/// The room code inside an invite link, `nflstreetreborn://join/<code>`,
/// as Windows hands it to the launcher when a friend's link is opened. The
/// scheme is matched without regard to case; the code keeps its own. Only
/// letters, digits and dashes count, so a link cannot smuggle anything else
/// onto the Online page.
fn join_code_from(arg: &str) -> Option<String> {
    let arg = arg.trim().trim_matches('"');
    let lower = arg.to_ascii_lowercase();
    let rest = lower.strip_prefix("nflstreetreborn://join/")?;
    let code: String = arg[arg.len() - rest.len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    (!code.is_empty() && code.len() <= 64).then_some(code)
}

/// Where an invite's room code waits for the Online page: a small file in
/// the data root, read and cleared by the page, so the window can open on
/// Play as usual and still land the player in the right room.
fn pending_join_path(platform: &Platform) -> Result<PathBuf, String> {
    Ok(platform
        .paths
        .data_root()
        .map_err(|e| e.to_string())?
        .join(reborn_core::DataDir::Config.relative())
        .join("pending-join.json"))
}

fn remember_pending_join(platform: &Platform, code: &str) -> Result<(), String> {
    let path = pending_join_path(platform)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = serde_json::json!({"code": code, "at": reborn_core::utc_now_iso()});
    std::fs::write(&path, serde_json::to_vec_pretty(&body).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// The room code an invite link left behind, if any. Not cleared here: the
/// Online page clears it once it has filled the form, so a page that only
/// glanced (Play, deciding whether to redirect) does not lose it.
#[tauri::command]
async fn netplay_pending_join() -> Result<Option<serde_json::Value>, String> {
    let path = pending_join_path(&Platform::detect())?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn netplay_pending_join_clear() -> Result<(), String> {
    let path = pending_join_path(&Platform::detect())?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// The launch a Play shortcut makes.
///
/// Returns `Err` with a sentence when nothing started. The caller then opens
/// the window instead of dying quietly: a shortcut that does nothing at all,
/// with no window and no message, is indistinguishable from a broken install.
fn play_now(platform: &Platform) -> Result<String, String> {
    let saved = saved_paths(platform)?;
    let legacy = default_legacy_root(platform);
    let (found, game_path) = find_runtime_and_game(
        platform,
        saved.runtime_exe.as_deref().and_then(Path::to_str),
        saved.game_path.as_deref().and_then(Path::to_str),
        legacy.as_deref(),
    )?;

    // The preset the player chose, or first run's answer. Never a guess: a
    // shortcut that quietly boots at a different resolution than the Play
    // button did is a difference nobody would think to look for.
    let flow = Onboarding::open(platform, Environment::nothing_found()).map_err(|e| e.to_string())?;
    let preset = flow
        .summary()
        .graphics
        .map(|g| g.preset())
        .unwrap_or(GraphicsPreset::Balanced);

    let req = LaunchRequest {
        runtime_exe: found.path().to_path_buf(),
        game_path,
        graphics: preset,
        fullscreen: true,
        build: GameBuild::Offline,
        mods_disabled: false,
        user_dir: saved.user_dir.clone(),
        league_match: false,
    };
    match Session::run(platform, &req).map_err(|e| e.to_string())? {
        Launched::Refused(r) => Err(r.message()),
        Launched::Finished { minutes, results, .. } => {
            // The shortcut holds no launch guard and `Session::run` has given
            // the lock back, so this is simply the send. Its report is not
            // shown: this mode draws no window. Skipping it would leave the
            // player's game sitting in the queue until they next opened the
            // launcher, and the whole point of the shortcut is that they may
            // not.
            let _ = results.send(platform);
            Ok(format!("Session ended after {minutes:.0} minutes."))
        }
    }
}

/// Open the folder the game is installed in.
///
/// The folder HOLDING `User`, not `User` itself: that is the one with the
/// emulator, `Games` and, on a 2.1 install, `MODS` beside the saves, and it
/// is what a player means by where the game lives. Adam: "I think I want it
/// to be the install directory, not just the user folder."
///
/// Resolved now rather than written into the shortcut at install time,
/// because which folder that is depends on the emulator in use and on
/// whether a 2.1 install is present. A shortcut carrying a path decided
/// months earlier would open the wrong folder and look correct.
fn open_game_folder(platform: &Platform) -> Result<PathBuf, String> {
    let saved = saved_paths(platform)?;
    let legacy = default_legacy_root(platform);
    let hint = saved.runtime_exe.as_deref().filter(|p| p.is_file());
    let exe = reborn_core::session::resolve_runtime(platform, hint, legacy.as_deref())
        .ok()
        .map(|r| r.path().to_path_buf());
    let user = reborn_core::session::resolve_user_dir_for(
        platform,
        saved.user_dir.as_deref(),
        exe.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    // Up one only when the folder is literally called "User", which is the
    // <install>\User shape both a 2.1 install and a portable Dolphin have.
    //
    // Going up unconditionally was wrong, seen while checking this: a stock
    // Dolphin keeps everything in %APPDATA%\Dolphin Emulator, whose parent is
    // plain Roaming, and a folder named outright in Settings is the folder
    // rather than a child of one. A folder at the root of a drive has no
    // parent either, and opening the drive would be worse than opening it.
    let resolved = user.path();
    let named_user = resolved
        .file_name()
        .map(|n| n.eq_ignore_ascii_case("user"))
        .unwrap_or(false);
    let path = match resolved.parent() {
        Some(parent) if named_user => parent.to_path_buf(),
        _ => resolved.to_path_buf(),
    };
    if !path.is_dir() {
        return Err(format!(
            "The game folder is not there yet: {}. Start the game once and it will be.",
            path.display()
        ));
    }
    open_in_file_manager(platform, &path)?;
    Ok(path)
}

/// Show a folder in the system's file manager.
fn open_in_file_manager(platform: &Platform, path: &Path) -> Result<(), String> {
    let program: &str = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let args = vec![path.as_os_str().to_os_string()];
    let env = std::collections::BTreeMap::new();
    // Explorer answers a non-zero exit code even when it opened the window, so
    // the handle is dropped rather than waited on.
    platform
        .processes
        .spawn(Path::new(program), &args, None, &env)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;

    #[test]
    fn another_launch_is_refused_until_the_session_guard_is_released() {
        assert!(!session_active());
        {
            let _guard = LaunchGuard::acquire().unwrap();
            assert!(session_active());
            assert!(LaunchGuard::acquire().is_err());
        }
        assert!(!session_active());
        assert!(LaunchGuard::acquire().is_ok());
    }

    #[test]
    fn the_shortcut_switches_are_recognized_and_nothing_else_is() {
        let m = |args: &[&str]| mode_from(args.iter().map(|s| s.to_string()));
        // argv[0] is the executable, so a switch is never the first item.
        assert_eq!(m(&["reborn-launcher.exe"]), Mode::Window);
        assert_eq!(m(&["reborn-launcher.exe", "--play"]), Mode::Play);
        assert_eq!(m(&["reborn-launcher.exe", "--folder"]), Mode::Folder);
        // Windows starts a shortcut with its own working directory and
        // sometimes extra arguments; an unknown one must not swallow ours.
        assert_eq!(m(&["exe", "--verbose", "--play"]), Mode::Play);
        // Anything we do not know is the window, never a guess.
        assert_eq!(m(&["exe", "--pla"]), Mode::Window);
        assert_eq!(m(&["exe", "play"]), Mode::Window);
        assert_eq!(m(&["exe", "-play"]), Mode::Window);
    }
}

fn main() {
    // The log, first, before anything can produce a warning worth keeping.
    //
    // Every module in the core answers a failure it will not show a player by
    // sending the detail to `tracing::warn!`. Until this call existed there
    // was no subscriber anywhere in the workspace, and a `tracing` event with
    // no subscriber is dropped: the detail was not moved off the screen, it
    // was deleted. It says nothing and it cannot fail a start.
    reborn_core::logging::start(&Platform::detect());

    // A credential this product retired, taken off the machine. `nsr.league.ingest`
    // was the bearer token Dolphin's own uploader posted with; the module that
    // used it is gone, and this branch removed the commands that could forget
    // it, so every installation that ever stored one would otherwise keep it in
    // the Windows credential vault forever with no way to remove it from the
    // product. It is a token this project has already had to rotate.
    //
    // Here rather than in a command, so it happens on every start including the
    // two shortcut modes below, which return before any window exists. Best
    // effort: it says nothing and it cannot fail a start.
    reborn_core::identity::forget_retired_league_token(&Platform::detect());

    // The shortcuts, before any window exists. Both fall through to the
    // window when they cannot do their job, so a shortcut never does
    // nothing at all with nothing to show for it.
    match mode_from(std::env::args()) {
        Mode::Join(code) => {
            // Remembered, then the window opens as usual; Play sends the
            // player straight on to Online, which fills in the room.
            let p = Platform::detect();
            if let Err(why) = remember_pending_join(&p, &code) {
                eprintln!("Could not keep the invite's room code: {why}");
            }
        }
        Mode::Play => {
            let p = Platform::detect();
            match play_now(&p) {
                Ok(_) => return,
                Err(why) => {
                    // The window opens and the Play page shows the same
                    // refusal, because it re-runs the same search.
                    eprintln!("Could not start the game: {why}");
                }
            }
        }
        Mode::Folder => {
            let p = Platform::detect();
            match open_game_folder(&p) {
                // Printed as well as opened. A shortcut sends this nowhere, but
                // run from a shell it is how anyone checks WHICH folder was
                // resolved, and Explorer's own window list cannot answer that:
                // Windows 11 opens a folder as a tab in an existing window.
                Ok(path) => {
                    println!("{}", path.display());
                    return;
                }
                Err(why) => eprintln!("Could not open the game folder: {why}"),
            }
        }
        Mode::Window => {}
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            // Already here.
            dolphin_settings::netplay_open,
            dolphin_settings::netplay_status,
            dolphin_settings::netplay_action,
            dolphin_settings::dolphin_preferences,
            dolphin_settings::dolphin_preference_set,
            dolphin_settings::dolphin_saves_folder,
            setup_tools::setup_install_choice,
            setup_tools::setup_install_info,
            shell_tools::setup_window_lock,
            shell_tools::open_website,
            shell_tools::uninstall_launcher,
            shell_tools::update_progress,
            shell_tools::notices_pending,
            shell_tools::notice_dismiss,
            netplay_pending_join,
            netplay_pending_join_clear,
            stats_view::stats_games,
            setup_tools::setup_import_existing_game,
            setup_tools::setup_migrate_install,
            setup_tools::setup_graphics_options,
            setup_tools::setup_save_graphics_options,
            setup_tools::setup_hardware_info,
            setup_tools::setup_import_game,
            setup_tools::setup_use_existing,
            setup_tools::controller_slots,
            setup_tools::controller_assign_player,
            setup_tools::setup_controller_mapping,
            setup_tools::setup_save_mapping,
            paths,
            devices,
            sessions,
            mods,
            home_state,
            prepare_launch,
            launch,
            launch_check,
            session_active,
            // Where the game will boot from.
            user_dir,
            // The launcher's own mod library and composition.
            mod_packages,
            mods_plan,
            mods_apply,
            mods_unapply,
            mods_verify,
            mods_sync,
            mods_toggle,
            // Bringing a 2.1 install over.
            migration_plan,
            migration_apply,
            // Verify and repair.
            library_verify,
            library_repair,
            // First run. Reading where it got to, then answering each stage.
            onboarding_progress,
            onboarding_reach,
            onboarding_back,
            onboarding_acknowledge,
            onboarding_decline_migration,
            onboarding_accept_migration,
            onboarding_confirm_game,
            onboarding_choose_graphics,
            onboarding_choose_controller,
            onboarding_decline_controller,
            onboarding_choose_controls,
            onboarding_choose_updates,
            update_policy,
            update_policy_set,
            // Content: textures, runtime, mods, game patches, the roster.
            content_updates::content_status,
            content_updates::content_check,
            content_updates::content_fetch,
            content_updates::content_promote,
            content_updates::content_dismiss,
            texture_policy,
            texture_policy_set,
            controls_set,
            controls_current,
            controls_options,
            onboarding_run_task,
            onboarding_record_task,
            onboarding_prepared,
            onboarding_finish,
            onboarding_start_over,
            // The Xbox wizard's drive picker.
            removable_drives,
            // Who the player is.
            identity_status,
            identity_sign_in_start,
            identity_sign_in_poll,
            identity_sign_out,
            identity_set_show_offline,
            // Captured results.
            stats_queue,
            stats_ingest,
            stats_submit,
            settings_set_path,
            settings_paths,
            pick_path,
            update_check,
            update_download,
            update_apply,
            open_runtime,
            diagnostic_bundle,
            // Custom soundtrack.
            music_page::music_status,
            music_page::music_inspect,
            music_page::music_lookup,
            music_page::music_stage_file,
            music_page::music_stage_stock,
            music_page::music_stage_details,
            music_page::music_unstage,
            music_page::music_pack_preview,
            music_page::music_stage_pack,
            music_page::music_export,
            music_page::music_set_enabled,
            music_page::music_apply,
            music_page::music_reset_all,
            music_page::music_dismiss,
            // Installed versions and updates.
            content_installed,
            content_update,
            // Driving the launcher with a pad.
            gamepad_contract,
            // Reading a key out of a config the launcher does not own.
            ini_value,
            // The player's real JSGME library.
            jsgme_library,
            // The component identities, so nothing guesses a name.
            components,
            // Whether an account secret is stored. Never the secret.
            credential_stored
        ])
        .run(tauri::generate_context!())
        .expect("the launcher window could not start");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text of one command, from its signature to the start of the next
    /// `#[tauri::command]`.
    fn command_body<'a>(source: &'a str, signature: &str) -> &'a str {
        let from = source
            .find(signature)
            .unwrap_or_else(|| panic!("{signature} is not in this file any more"));
        let rest = &source[from..];
        let to = rest.find("\n#[tauri::command]").unwrap_or(rest.len());
        &rest[..to]
    }

    /// The text of one item, from `start` to the first following `stop`.
    ///
    /// Like `command_body`, but for the free functions between commands,
    /// which have no shared marker like `#[tauri::command]` to stop at. The
    /// caller names the next item's own text instead.
    fn body_until<'a>(source: &'a str, start: &str, stop: &str) -> &'a str {
        let from = source
            .find(start)
            .unwrap_or_else(|| panic!("{start} is not in this file any more"));
        let rest = &source[from..];
        let to = rest
            .find(stop)
            .unwrap_or_else(|| panic!("{stop} is not in this file any more, or moved before {start}"));
        &rest[..to]
    }

    /// A `spawn_blocking` command's body, with the trailing `.await` /
    /// `map_err` that turns a `JoinError` into a string trimmed off.
    ///
    /// Every command built on `spawn_blocking` ends with that pair, and it
    /// has nothing to do with `reborn_core::Error`: it only fires if the
    /// closure itself panicked. Left in, it would satisfy any check for "no
    /// more turning an error into raw text" by accident, on every command in
    /// this file, whether or not the real fix was made.
    fn inner_body(body: &str) -> &str {
        &body[..body.find("\n    .await").unwrap_or(body.len())]
    }

    /// Every way of starting the emulator records a session and collects what
    /// that session produced.
    ///
    /// Two commands start the emulator without going through `Session::run`:
    /// the NetPlay dialog, which is the launcher's ONLY path to a
    /// `GameStats\Online` capture, and the button that opens Dolphin on its
    /// own. Both used to spawn and walk away. Every online game played through
    /// the launcher was written to disk and then either never delivered or
    /// adopted by the next offline session, which stamped it with an id, an
    /// exit reason and a league intent from a game it did not produce.
    ///
    /// This reads the source of those two commands, because that is exactly
    /// how the defect was found and exactly how it would come back: everything
    /// compiled, the game started perfectly, and the wiring simply was not
    /// there. What the collection then does with a real capture cannot be
    /// observed without a real emulator and a real game, and a test that
    /// pretended otherwise would be asserting nothing.
    #[test]
    fn every_way_of_starting_the_emulator_records_a_session_and_collects_it() {
        for (name, body) in [
            (
                "open_runtime",
                command_body(include_str!("main.rs"), "async fn open_runtime"),
            ),
            (
                "netplay_open",
                command_body(include_str!("dolphin_settings.rs"), "pub async fn netplay_open"),
            ),
        ] {
            assert!(
                body.contains("Supervisor::acquire"),
                "{name} starts the emulator without taking the launcher lock"
            );
            assert!(
                body.contains("begin_with_intent"),
                "{name} starts the emulator without writing down what the session is, \
                 so anything it captures has no session that explains it"
            );
            assert!(
                body.contains("finish_and_collect"),
                "{name} never collects the results the session produced, so they sit \
                 in Dolphin's folder until some later session adopts them"
            );
            // The collection has to happen inside the waiting thread, with the
            // guard still held, the way Session::run keeps its lock across the
            // same work.
            let guard = body
                .find("let _guard")
                .expect("the waiting thread must take the launch guard");
            let collect = body.find("finish_and_collect").expect("checked above");
            assert!(
                guard < collect,
                "{name} collects results outside the guard that keeps a second \
                 session from starting in the middle of it"
            );
        }
    }

    /// A sentence the server wrote about a player's own game has to reach them.
    ///
    /// `QueueEntry::note` is set by exactly one answer: `Recorded`, which is
    /// the server saying it stored the result and counted it as incomplete
    /// because the winner did not reach 36 points. It was written to the entry
    /// and shown to nobody, and the row it belongs to says "sent and counted",
    /// which is true and is not the whole truth.
    ///
    /// Kept rather than deleted, and surfaced: it is the only place a player
    /// learns their game was counted with a qualification, and that is a fact
    /// about a game they played.
    #[test]
    fn a_result_recorded_with_a_qualification_shows_what_the_server_said() {
        let note = "Recorded as incomplete: the winner did not reach 36 points.";
        let entry = QueueEntry {
            key: CaptureKey::of_bytes(b"a"),
            state: EntryState::Accepted,
            attempts: 0,
            first_seen_ms: 1,
            last_attempt_ms: None,
            finished_ms: Some(2),
            last_error: None,
            source_path: None,
            session_id: Some("s1".into()),
            capture_folder: Some("offline".into()),
            note: Some(note.to_string()),
            refusal_code: None,
            summary: None,
        };
        assert_eq!(
            entry_view(&entry).note.as_deref(),
            Some(note),
            "the one sentence the server wrote about this player's own game is stored and \
             shown to nobody"
        );
    }

    /// A queue row never carries this launcher's own flag codes out to a
    /// page.
    ///
    /// `entry_view` used to copy `CaptureSummary::flags` (`"wrong-schema"`,
    /// `"no-players"`, and so on) onto the row verbatim. Nothing renders
    /// them as a message; they are internal vocabulary a player was never
    /// taught, whether or not a page happens to print them today. The
    /// meaningful signal for a player, whether a result needs a person to
    /// look at it, is already on the row as `needs_review`, so the codes are
    /// dropped rather than translated: there is nothing left worth adding a
    /// vocabulary for.
    #[test]
    fn a_queue_row_never_carries_this_launchers_internal_flag_codes() {
        let entry = QueueEntry {
            key: CaptureKey::of_bytes(b"b"),
            state: EntryState::Quarantined {
                reason: QuarantineReason::FailedValidation {
                    flags: vec![reborn_core::stats::codes::WRONG_SCHEMA.to_string()],
                },
            },
            attempts: 0,
            first_seen_ms: 1,
            last_attempt_ms: None,
            finished_ms: None,
            last_error: None,
            source_path: None,
            session_id: None,
            capture_folder: None,
            note: None,
            refusal_code: Some("hash-mismatch".to_string()),
            summary: Some(reborn_core::stats::CaptureSummary {
                captured_at: "2026-01-01T00:00:00Z".to_string(),
                teams: Vec::new(),
                players: 2,
                sides_verified: false,
                flags: vec![
                    reborn_core::stats::codes::WRONG_SCHEMA.to_string(),
                    reborn_core::stats::codes::NO_PLAYERS.to_string(),
                ],
                needs_review: true,
            }),
        };
        let view = entry_view(&entry);
        let json = serde_json::to_value(&view).expect("StatsEntryView serializes");
        assert!(
            json.get("flags").is_none(),
            "a queue row still carries this launcher's own flag codes to a page: {json}"
        );
        assert_eq!(
            view.needs_review,
            Some(true),
            "the meaningful signal a player can use has to survive removing the codes"
        );
        // The same rule for the server's vocabulary. `refusal_code` is kept on
        // the entry for the person running `stats retry --apply`, at a command
        // line; it is not a word a player was ever taught.
        assert!(
            !serde_json::to_string(&view).unwrap().contains("hash-mismatch"),
            "the server's own word for a refusal reached a page: {json}"
        );
    }

    /// A command must not declare a parameter it ignores.
    ///
    /// `stats_submit` took `confirm: Option<bool>` and threw it away on the
    /// first line, while the harness row still sent `confirm: true` under a
    /// description of behavior the command no longer had. A parameter that is
    /// declared and ignored teaches a caller that the flag means something,
    /// and the next person to read the page believes the description.
    #[test]
    fn stats_submit_declares_no_parameter_it_ignores() {
        let body = command_body(include_str!("main.rs"), "async fn stats_submit");
        let signature = &body[..body.find('{').unwrap_or(body.len())];
        assert!(
            !signature.contains("confirm"),
            "stats_submit still takes a confirm it does not use: {signature}"
        );
        assert!(
            !body.contains("let _ = confirm;"),
            "stats_submit still throws away an argument it declared"
        );
    }

    /// The retired league credential is actually taken off the machine.
    ///
    /// Naming it for removal in the core is half a fix. Something has to call
    /// the removal, and the only thing every player runs is this window.
    #[test]
    fn every_start_of_the_launcher_forgets_the_retired_league_credential() {
        let src = include_str!("main.rs");
        let from = src.find("\nfn main() {").expect("main is not in this file any more");
        // Bounded at the test module that follows, or this test's own text
        // would satisfy it.
        let rest = &src[from..];
        let body = &rest[..rest.find("\n#[cfg(test)]").unwrap_or(rest.len())];
        assert!(
            body.contains("forget_retired_league_token"),
            "a token this project has already had to rotate stays in the credential vault \
             forever, on every machine that ever stored one"
        );
    }

    /// The lock and the guard go back before the network work, and the online
    /// start waits for a lock that is about to be given back.
    ///
    /// Opening the emulator from Settings, and starting an online session,
    /// held the launch guard and the single-instance lock across the whole
    /// collect-and-send. A send is one request per queued game with the
    /// client's full timeout each, so on a slow or unreachable backend that is
    /// minutes. During it, pressing Play said a game session was already
    /// active and the Online page said to close the emulator first. Both were
    /// false: the emulator had already exited.
    ///
    /// The order that has to hold is collect, then give both back, then send.
    /// Collecting is the part that needs the lock, because the queue and the
    /// session log are shared state; sending needs neither.
    ///
    /// The second half: the online start took the plain `acquire`, so a lock
    /// held for a moment by this launcher's own background work refused it
    /// outright. The Play path was given the waiting acquire for exactly that
    /// reason and this path was not.
    ///
    /// The third path is the one every player takes. `launch` is the Play
    /// button. It holds the launch guard across the whole of `Session::run`,
    /// and `Session::run` collected AND sent while still holding the
    /// single-instance lock, so on the primary path both of those false
    /// sentences were still being said for the length of a send. The two
    /// commands above were the only ones fixed. Its markers differ because the
    /// work is split across two crates: `Session::run` collects and hands the
    /// lock back by returning, this command hands the guard back, and only
    /// then does anything reach the network. What `Session::run` itself does
    /// under the lock is held by
    /// `reborn_core::session::tests::the_launch_gives_the_lock_back_before_the_network_work`.
    ///
    /// # How this is bounded so it cannot find itself
    ///
    /// `open_runtime` and `launch` are read with `command_body`, which stops
    /// at the next `#[tauri::command]`, and `netplay_open` is read out of
    /// `dolphin_settings.rs`, a different file from this one. All three slices
    /// end thousands of lines before this test, so the names below are never
    /// inside what is searched.
    #[test]
    fn the_lock_and_the_guard_go_back_before_the_network_work() {
        for (name, body, collects, releases) in [
            (
                "open_runtime",
                command_body(include_str!("main.rs"), "async fn open_runtime"),
                "finish_and_collect",
                "drop(supervisor)",
            ),
            (
                "netplay_open",
                command_body(include_str!("dolphin_settings.rs"), "pub async fn netplay_open"),
                "finish_and_collect",
                "drop(supervisor)",
            ),
            (
                "launch",
                command_body(include_str!("main.rs"), "async fn launch("),
                "Session::run",
                "drop(_guard)",
            ),
        ] {
            let collect = body
                .find(collects)
                .unwrap_or_else(|| panic!("{name} no longer collects what the session produced"));
            let release = body.find(releases).unwrap_or_else(|| {
                panic!(
                    "{name} never gives the single-instance lock and the launch guard back, so \
                     every other screen is told a launcher is running for as long as the send \
                     takes"
                )
            });
            let send = body.find(".send(&p)").unwrap_or_else(|| {
                panic!("{name} no longer sends what it collected, so results sit in the queue")
            });
            assert!(
                collect < release,
                "{name} gives the lock back before the results are safely collected"
            );
            assert!(
                release < send,
                "{name} holds the lock and the guard across the network work, so Play says a \
                 game session is active and the Online page says to close an emulator that \
                 has already exited"
            );
        }

        let netplay = command_body(
            include_str!("dolphin_settings.rs"),
            "pub async fn netplay_open",
        );
        assert!(
            netplay.contains("acquire_waiting"),
            "starting an online session is still refused outright by a lock that is about to \
             be given back"
        );
    }

    /// Nothing the Play page or the Settings page can reach turns a core
    /// error into raw text either.
    ///
    /// `home.js` renders what `devices` and `mod_packages` reject with, and
    /// `settings.js` renders what the send button's `stats_submit` rejects
    /// with. All four still stringified a `reborn_core::Error`, so a
    /// permission problem reading the mods folder put a full path on the Play
    /// card and a locked settings file put a path and an error number under
    /// the send button.
    ///
    /// # How this is bounded so it cannot find itself
    ///
    /// Each function's text is taken between two anchors that both appear in
    /// production code, thousands of lines before this test, and `.find`
    /// returns the first match, so the copies of those anchors in this test's
    /// own arguments are never reached. The check then runs only over those
    /// slices, never over the whole file. The trailing `.await` pair every
    /// `spawn_blocking` command ends with is removed first: it converts a
    /// `JoinError` from a panicked closure, not a core error, and left in it
    /// would make this impossible to pass whether or not the fix was made.
    #[test]
    fn nothing_the_play_and_settings_pages_call_turns_a_core_error_into_raw_text() {
        let src = include_str!("main.rs");
        // Each stop is the next item's own first line, not a shared marker
        // like `#[tauri::command]`: between two commands this file keeps free
        // functions that belong to other pages, and stopping at the marker
        // would drag them in and make the check fail on somebody else's code.
        let bodies = [
            (
                "devices",
                body_until(src, "async fn devices(", "
#[tauri::command]
fn sessions()"),
            ),
            (
                "read_devices",
                body_until(src, "fn read_devices(", "
#[tauri::command]
fn sessions()"),
            ),
            (
                "mod_packages",
                body_until(src, "async fn mod_packages(", "
struct ModPlanView {"),
            ),
            (
                "stats_submit",
                body_until(src, "async fn stats_submit(", "
struct StatsSubmitView {"),
            ),
            (
                "history_without_holding_the_lock",
                body_until(
                    src,
                    "fn history_without_holding_the_lock(",
                    "
/// Send whatever is queued",
                ),
            ),
        ];
        for (name, body) in bodies {
            let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
            let own = compact.replace(".await.map_err(|e|e.to_string())", "");
            assert!(
                !own.contains(".map_err(|e|e.to_string())"),
                "{name} still turns a core error straight into its raw Display text, so a \
                 path and an operating system error number reach a page"
            );
            assert!(
                own.contains("player_message"),
                "{name} no longer maps a core error to a sentence at all"
            );
        }
    }

    /// Nothing the Online page can reach turns a core error into raw text.
    ///
    /// `online.js` renders what these commands reject with, verbatim. Five
    /// call sites in `dolphin_settings.rs` stringified a
    /// `reborn_core::Error` -- three of them added by fix wave 1 -- and the
    /// raw `std::io::Error`s beside them did the same, so a settings file
    /// another program had open, or a full disk, put a full path and an
    /// operating system error number on the Online page.
    ///
    /// # How this is bounded so it cannot find itself
    ///
    /// It scans `dolphin_settings.rs`, a different file from the one this test
    /// lives in, so the patterns written below are not in what is searched at
    /// all. That file's own inline test module is cut out as well, by the two
    /// anchors below, so a future test added there cannot satisfy this either.
    /// The trailing `.await` pair every `spawn_blocking` command ends with is
    /// removed before the check: it converts a `JoinError` from a panicked
    /// closure, which is not a core error, and left in it would make this
    /// check impossible to pass whether or not the real fix was made.
    #[test]
    fn nothing_the_online_page_can_call_turns_a_core_error_into_raw_text() {
        let src = include_str!("dolphin_settings.rs");
        let before = &src[..src.find("\n#[cfg(test)]").expect("the inline test module moved")];
        let after_at = src
            .find("\n/// Start Dolphin with NetPlay configured")
            .expect("netplay_open's doc comment moved");
        let production = format!("{before}{}", &src[after_at..]);

        let compact: String = production.chars().filter(|c| !c.is_whitespace()).collect();
        let commands_only = compact.replace(".await.map_err(|e|e.to_string())", "");
        assert!(
            !commands_only.contains(".map_err(|e|e.to_string())"),
            "the Online page still gets a core error's raw Display text, so a locked file \
             or a full disk puts a path and an error number on the screen"
        );
        assert!(
            !commands_only.contains("Err(e.to_string())"),
            "the Online page still gets an operating system error's own wording"
        );
        assert!(
            commands_only.contains("player_message"),
            "no core error on this page is mapped to a sentence at all"
        );
    }

    /// The launcher writes a log, and the diagnostic bundle can collect it.
    ///
    /// Three fix waves moved technical detail off the screens a player reads
    /// and said it goes to the log. There was no log: 56 `tracing::warn!` call
    /// sites and no subscriber anywhere in the workspace, and a `tracing`
    /// event with no subscriber is dropped. So the detail was not moved, it
    /// was deleted, and the bundle collected the emulator's log and could not
    /// collect the launcher's, because there was not one.
    ///
    /// That a warning really reaches a file on disk is proved in
    /// `reborn_core::logging`, against a real file. What is checked here is
    /// the wiring this binary owns: the window starts the log before anything
    /// can warn, and the bundle picks the file up.
    ///
    /// # How this is bounded so it cannot find itself
    ///
    /// `fn main`'s text stops at the `#[cfg(test)]` that begins this module,
    /// and `diagnostic_bundle`'s stops at the next `#[tauri::command]`. Both
    /// cuts are before this test's own line, so the names written below are
    /// never inside what is searched.
    #[test]
    fn the_launcher_writes_a_log_and_the_bundle_collects_it() {
        let src = include_str!("main.rs");
        let from = src.find("\nfn main() {").expect("main is not in this file any more");
        let rest = &src[from..];
        let main_body = &rest[..rest.find("\n#[cfg(test)]").unwrap_or(rest.len())];
        assert!(
            main_body.contains("logging::start"),
            "the window never starts a log, so every warning the core raises is dropped \
             and the detail three fix waves moved off the screen is simply gone"
        );

        let bundle = command_body(src, "async fn diagnostic_bundle");
        assert!(
            bundle.contains("logging::LOG_FILE"),
            "the bundle collects the emulator's log and not the launcher's, which is the \
             only place the launcher's own failures are written down"
        );
        assert!(
            bundle.contains("logging::PREVIOUS_LOG_FILE"),
            "the bundle takes the live log and not the rolled one, so anything that \
             happened before the log filled is left behind"
        );
    }

    /// The launcher's own background send never makes Play refuse.
    ///
    /// Both send paths took the single-instance lock and held it for the whole
    /// network round trip. `Session::run` takes the same lock, and failing, it
    /// tells the player "Another launcher is already running. Close it first."
    /// So a player who signed in and then pressed Play was told to close a
    /// launcher that does not exist, and the game did not start. The client's
    /// timeout is two minutes per request and a backlog is one request per
    /// game, so the window was minutes wide.
    ///
    /// The two spawn paths fix wave 1 added, `open_runtime` and `netplay_open`,
    /// hold the same lock, and they are deliberately not in this list: they
    /// hold it while the emulator is actually open, which is exactly what the
    /// lock is for and exactly what the message says.
    #[test]
    fn a_background_send_never_makes_play_refuse() {
        let src = include_str!("main.rs");
        for (name, body) in [
            ("stats_submit", command_body(src, "async fn stats_submit")),
            ("send_backlog_in_background", command_body(src, "fn send_backlog_in_background")),
        ] {
            assert!(
                !body.contains("Supervisor::acquire"),
                "{name} takes the launcher lock itself, so it holds it across the send and \
                 Play refuses with a launcher that does not exist"
            );
            assert!(
                body.contains("history_without_holding_the_lock"),
                "{name} no longer reads the session history the one way that gives the \
                 lock straight back"
            );
            assert!(
                body.contains("auto_submit"),
                "{name} no longer sends anything at all"
            );
        }
    }

    /// Signing in answers the window before the backlog goes out.
    ///
    /// `identity_sign_in_poll` used to save the sign-in and then send the whole
    /// queue before it returned: one network round trip per game, each with the
    /// client's full two minute timeout. The window polls this command on an
    /// interval, so every tick that fired during that wait asked about a device
    /// code the server had already consumed, and the refusals landed on the
    /// screen beside the sign-in that had just succeeded.
    #[test]
    fn signing_in_answers_the_window_before_the_backlog_is_sent() {
        let body = command_body(include_str!("main.rs"), "async fn identity_sign_in_poll");
        assert!(
            !body.contains("auto_submit"),
            "the sign-in poll still sends the backlog before it answers, so the window \
             keeps polling with a code the server has already taken"
        );
        assert!(
            body.contains("send_backlog_in_background"),
            "signing in no longer starts the backlog send at all"
        );
    }

    /// No core error's raw `Display` reaches a player through the launch or
    /// open-emulator path.
    ///
    /// `reborn_core::Error::Io` displays as `io error at {path}: {source}`,
    /// and every one of these functions used to hand that straight to a
    /// caller with `.map_err(|e| e.to_string())`: a permission problem or a
    /// full disk starting a game put a full file path and an operating
    /// system error number on the Play page, because `home.js` renders
    /// whatever `launch`, `launch_check` and `open_runtime` hand back with no
    /// wording of its own. `Error::player_message` lives in `reborn-core`,
    /// not here, because the window has no logger to put the real detail in
    /// (see `identity::origin`'s doc comment for the same reasoning); this
    /// checks that every command on the path actually calls it, once, at the
    /// boundary all of them share.
    #[test]
    fn launch_and_open_runtime_never_turn_a_core_error_into_raw_text() {
        let src = include_str!("main.rs");
        let bodies: [(&str, &str); 7] = [
            (
                "find_runtime_and_game",
                body_until(src, "\nfn find_runtime_and_game(", "\nfn find_runtime_and_game_for("),
            ),
            (
                "find_runtime_and_game_for",
                body_until(src, "\nfn find_runtime_and_game_for(", "\n/// The emulator a hint resolves to"),
            ),
            (
                "saved_paths",
                body_until(src, "\nfn saved_paths(", "\n/// What the shortcuts ask for."),
            ),
            (
                "selected_user_dir",
                body_until(src, "\nfn selected_user_dir(", "\n#[tauri::command]\nasync fn settings_paths"),
            ),
            ("launch_check", inner_body(command_body(src, "async fn launch_check"))),
            ("launch", inner_body(command_body(src, "async fn launch("))),
            ("open_runtime", inner_body(command_body(src, "async fn open_runtime"))),
        ];
        for (name, body) in bodies {
            let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
            assert!(
                !compact.contains(".map_err(|e|e.to_string())"),
                "{name} still turns a core error straight into its raw Display text"
            );
        }
        for name in [
            "find_runtime_and_game_for",
            "saved_paths",
            "selected_user_dir",
            "launch_check",
            "launch",
            "open_runtime",
        ] {
            let (_, body) = bodies.iter().find(|(n, _)| *n == name).expect("listed above");
            assert!(
                body.contains("player_message"),
                "{name} no longer maps a core error to a sentence at all"
            );
        }
    }

    #[test]
    fn an_invite_link_argument_yields_its_room_code_and_nothing_else() {
        assert_eq!(join_code_from("nflstreetreborn://join/AB12CD"), Some("AB12CD".into()));
        assert_eq!(join_code_from("NFLSTREETREBORN://join/ab-12"), Some("ab-12".into()));
        assert_eq!(join_code_from("\"nflstreetreborn://join/AB12CD\""), Some("AB12CD".into()), "quoted, as Windows may pass it");
        assert_eq!(join_code_from("nflstreetreborn://join/AB12CD/../x?y"), Some("AB12CD".into()), "only the code survives");
        assert_eq!(join_code_from("nflstreetreborn://join/"), None);
        assert_eq!(join_code_from("nflstreetreborn://host/AB12CD"), None);
        assert_eq!(join_code_from("--play"), None);
        assert!(matches!(mode_from(["x.exe".to_string(), "nflstreetreborn://join/Q1".to_string()].into_iter()), Mode::Join(c) if c == "Q1"));
        assert!(matches!(mode_from(["x.exe".to_string()].into_iter()), Mode::Window));
    }

    #[test]
    fn identity_view_from_a_file_and_from_nothing() {
        let file = reborn_core::identity::IdentityFile {
            player: reborn_core::identity::PlayerFacts { id: "p1".into(), display_name: "Adam".into(), avatar_hash: None, show_offline: true },
            signed_in_at_ms: 1,
        };
        let v = identity_view(Some(&file));
        assert!(v.signed_in);
        assert_eq!(v.display_name.as_deref(), Some("Adam"));
        assert_eq!(v.show_offline, Some(true));
        let none = identity_view(None);
        assert!(!none.signed_in && none.player_id.is_none());
    }

    #[test]
    fn setup_image_rejects_missing_empty_directory_and_unreadable_content() {
        let root = std::env::temp_dir().join(format!("reborn-setup-image-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir(&root).unwrap();
        let game = root.join("game.iso");
        assert!(checked_setup_image(&game).is_err());
        std::fs::write(&game, []).unwrap();
        assert!(checked_setup_image(&game).is_err());
        std::fs::write(&game, b"test image bytes").unwrap();
        assert!(checked_setup_image(&game).is_err());
        let wrong = root.join("game.txt");
        std::fs::write(&wrong, b"test image bytes").unwrap();
        assert!(checked_setup_image(&wrong).is_err());
        let dir = root.join("folder.iso");
        std::fs::create_dir(&dir).unwrap();
        assert!(checked_setup_image(&dir).is_err());
        std::fs::remove_file(game).unwrap();
        std::fs::remove_file(wrong).unwrap();
        std::fs::remove_dir(dir).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    use reborn_core::manifest::{ManifestFile, Sha256Hex};
    use reborn_core::migration::LegacyInstall;
    use reborn_core::mods::ComposeRefusal;
    use reborn_core::stats::{CaptureKey, EntryState, QuarantineReason, QueueEntry};

    fn legacy(applied: bool, backup: bool) -> LegacyInstall {
        LegacyInstall {
            root: PathBuf::from("legacy-root"),
            has_memory_cards: true,
            has_screenshots: true,
            has_textures: true,
            has_applied_mods: applied,
            has_jsgme_backup: backup,
            mod_blocker: None,
        }
    }

    fn entry(state: EntryState) -> QueueEntry {
        QueueEntry {
            key: CaptureKey::of_bytes(format!("{state:?}").as_bytes()),
            state,
            attempts: 0,
            first_seen_ms: 0,
            last_attempt_ms: None,
            finished_ms: None,
            last_error: None,
            source_path: None,
            session_id: None,
            summary: None,
            capture_folder: None,
            note: None,
            refusal_code: None,
        }
    }

    /// A destructive command called without a confirm must refuse, and the
    /// refusal has to name the flag.
    ///
    /// The failure this pins: a caller that gets a bare "not confirmed" cannot
    /// tell what to send next, so it sends something else, and the eventual fix
    /// is somebody removing the check.
    #[test]
    fn an_unconfirmed_command_refuses_and_says_what_to_send() {
        let err = confirmed(false, "mods_apply", "delete the composed tree")
            .expect_err("an unconfirmed destructive call must not be allowed");
        assert!(err.contains("mods_apply"), "{err}");
        assert!(err.contains("confirm"), "{err}");
        assert!(
            err.contains("Nothing was changed"),
            "the caller has to know the state of the machine: {err}"
        );
        assert!(confirmed(true, "mods_apply", "x").is_ok());
    }

    /// Mods applied with no JSGME backup must stop a migration, and the core's
    /// own reason must reach the caller unchanged.
    ///
    /// Copying then would take the modded textures as the pristine tree, and
    /// nothing afterwards can tell the difference: every later verify would call
    /// the modded files correct.
    #[test]
    fn a_blocked_migration_is_refused_with_the_reason_intact() {
        let install = legacy(true, false);
        let plan = MigrationPlan::build(&install);
        // Cloned rather than moved: the plan is still needed below, and a
        // partial move of an owned reason would take it.
        let reason = plan
            .blocked
            .clone()
            .expect("no backup with mods applied is blocked");
        let refusal = migration_refusal(&plan).expect("a blocked plan must be refused");
        assert_eq!(refusal, reason, "the reason must not be re-worded here");

        let view = plan_to_view(Path::new("legacy-root"), Some(&plan));
        assert!(!view.can_apply);
        assert_eq!(view.cannot_apply_because.as_deref(), Some(reason.as_str()));
    }

    /// Applied mods stop a migration even when a backup exists, because nothing
    /// in this build can un-apply them.
    ///
    /// The core's plan sets `needs_mod_unapply` and orders textures after an
    /// un-apply step that no code performs: `jsgme` only reads, and `migration`
    /// has no such step. The command line tool copies anyway. This is the check
    /// that keeps the window from repeating that.
    #[test]
    fn applied_mods_stop_a_migration_because_nothing_can_un_apply_them() {
        let plan = MigrationPlan::build(&legacy(true, true));
        assert!(plan.blocked.is_none(), "a backup exists, so not blocked");
        assert!(plan.needs_mod_unapply);
        let refusal =
            migration_refusal(&plan).expect("applied mods must stop the copy, not be ignored");
        assert!(refusal.contains("JSGME"), "{refusal}");
        assert!(
            plan.steps.iter().any(|s| s.what == "Textures"),
            "the plan does list textures, which is what makes this dangerous"
        );

        // A clean install must still be allowed, or the check is just a wall.
        assert!(migration_refusal(&MigrationPlan::build(&legacy(false, false))).is_none());
    }

    /// A result the server accepted or held must never be counted as waiting.
    ///
    /// Held means stored, with a person to decide, and it is a success. Counting
    /// it as pending is exactly the mistake that sent this project's uploader
    /// into a retry loop and produced a real outage.
    #[test]
    fn a_sent_result_is_never_counted_as_waiting() {
        let counts = queue_counts(&[
            entry(EntryState::Queued),
            entry(EntryState::Queued),
            entry(EntryState::Accepted),
            entry(EntryState::Held {
                reason: "under review".to_string(),
            }),
            entry(EntryState::Rejected {
                reason: "schema".to_string(),
            }),
            entry(EntryState::Stalled {
                reason: "offline".to_string(),
            }),
            entry(EntryState::Quarantined {
                reason: QuarantineReason::NoSession,
            }),
        ]);
        assert_eq!(
            counts.depth, 3,
            "two queued and one stalled: a pass requeues a stall before it \
             starts, so all three are what it would try"
        );
        assert_eq!(counts.accepted, 1);
        assert_eq!(counts.held, 1);
        assert_eq!(counts.rejected, 1);
        assert_eq!(counts.stalled, 1);
        assert_eq!(counts.quarantined, 1);
        assert_eq!(counts.total, 7);
    }

    /// A stalled capture is a game that was played and has not been delivered.
    /// The settings panel shows one number, "Waiting to send", and it read zero
    /// while real games sat there, so nothing on any screen mentioned them.
    #[test]
    fn a_stalled_result_is_counted_as_waiting_because_the_next_pass_will_try_it() {
        let counts = queue_counts(&[entry(EntryState::Stalled {
            reason: "offline".to_string(),
        })]);
        assert_eq!(counts.depth, 1, "the player must be able to see it");
        assert_eq!(counts.stalled, 1);
    }

    /// The send note said "Nothing was waiting to be sent" over a queue with
    /// stalled games in it, which was false.
    #[test]
    fn the_send_note_never_says_nothing_was_waiting_when_something_was() {
        let key = reborn_core::stats::CaptureKey::of_bytes(b"a game that was played");
        let mut report = reborn_core::stats::SubmitReport::default();
        report.stalled.push(key.clone());
        let note = submit_note(&report);
        assert!(
            !note.contains("Nothing was waiting"),
            "a stalled result IS waiting: {note}"
        );
        assert!(
            note.contains("again"),
            "and it must say it will be tried again: {note}"
        );

        report.stalled.clear();
        report.deferred.push(key);
        assert!(!submit_note(&report).contains("Nothing was waiting"));
    }

    /// A file the player customized must never appear in the repair set.
    ///
    /// A rebuilt offline image never matches its shipped hash, because a song
    /// changed. Reporting it as damaged is how a repair re-fetches 1.5 GB over
    /// somebody's own soundtrack.
    #[test]
    fn a_user_modified_file_is_never_offered_for_repair() {
        let component = Component {
            schema: "nsr.component.v1".to_string(),
            component: "offline-content".to_string(),
            version: "1.0.0".to_string(),
            sequence: 1,
            install_root: DataDir::LibraryOffline.relative().to_string(),
            customizable: true,
            files: vec![
                ManifestFile {
                    path: "game.iso".to_string(),
                    bytes: 1_500_000_000,
                    sha256: Sha256Hex::from_bytes(b"shipped"),
                },
                ManifestFile {
                    path: "readme.txt".to_string(),
                    bytes: 10,
                    sha256: Sha256Hex::from_bytes(b"readme"),
                },
            ],
        };
        let mut verdicts = std::collections::BTreeMap::new();
        verdicts.insert("game.iso".to_string(), FileVerdict::UserModified);
        verdicts.insert("readme.txt".to_string(), FileVerdict::Missing);
        let report = VerifyReport {
            component: "offline-content".to_string(),
            verdicts,
        };

        let view = verify_view(&report, &component, Path::new("library/offline"));
        assert_eq!(view.user_modified, 1);
        assert_eq!(view.user_modified_paths, vec!["game.iso".to_string()]);
        assert_eq!(view.missing_paths, vec!["readme.txt".to_string()]);
        assert!(
            !view.missing_paths.contains(&"game.iso".to_string()),
            "the customized image must not be in the repair set"
        );
        assert_eq!(
            view.repair_bytes, 10,
            "only the 10 byte file would be re-fetched, not 1.5 GB"
        );
        assert!(!view.healthy, "a missing file is damage");
        assert!(
            !view.repair_available,
            "nothing can fetch bytes yet, and the view must not imply otherwise"
        );
    }

    /// A refused composition must never look like a plan.
    ///
    /// `planned` false with every count at zero is a state a screen can read.
    /// A refusal reported as a plan of zero files would render as "0 mods, all
    /// good" over a tree nothing checked.
    #[test]
    fn a_refused_composition_is_not_an_empty_plan() {
        let game = GameId::parse("GN7E69").expect("a real disc id");
        let refusal = ComposeRefusal::NoPristineTree {
            path: PathBuf::from("library/textures/GN7E69"),
        };
        let expected = refusal.message();
        let view = plan_view(&game, &Composition::Refused(refusal));

        assert!(!view.planned);
        assert_eq!(view.refusal.as_deref(), Some(expected.as_str()));
        assert!(view.composed_root.is_none(), "no plan means no path");
        assert!(view.method.is_none());
        assert_eq!(view.total_entries, 0);
        assert_eq!(view.game, "GN7E69");
    }

    /// A fresh user directory has to announce what will be missing.
    ///
    /// Booting against an empty folder is a legitimate state and looks identical
    /// to the bug where the launcher created its own and handed that to Dolphin,
    /// which showed the player no save data at all.
    #[test]
    fn a_fresh_user_directory_says_what_will_be_missing() {
        let view = user_dir_view(&UserDir::Fresh(PathBuf::from("runtime/user")));
        assert_eq!(view.kind, "fresh");
        assert!(!view.populated);
        assert!(view.note.contains("memory cards"), "{}", view.note);

        let existing = user_dir_view(&UserDir::Existing(PathBuf::from("real/User")));
        assert_eq!(existing.kind, "existing");
        assert!(existing.populated);
    }

    /// An unknown component name must be refused, not answered.
    ///
    /// The failure this pins: `content_installed("offline_content")`, with an
    /// underscore instead of the hyphen the manifest uses, answering "nothing
    /// is installed". That reads as a missing install of a component that is
    /// actually there, and the next thing the player is offered is a download.
    #[test]
    fn an_unknown_component_name_is_refused_rather_than_answered() {
        let err = known_component("offline_content")
            .expect_err("a name that is not one of the seven must not be answered");
        assert!(err.contains("offline-content"), "name the real ones: {err}");
        assert!(known_component("offline-content").is_ok());
        assert!(known_component("textures").is_ok());
    }

    /// An unbound button must be absent from the contract, never present with a
    /// placeholder.
    ///
    /// The hint strip is the only place a binding is stated, so a slot that
    /// shows a label and does nothing is a lie the player acts on. Guide belongs
    /// to the operating system and is bound in no context at all.
    #[test]
    fn the_contract_never_invents_a_binding() {
        let contract = gamepad_contract(Some("playstation".to_string())).expect("pure lookup");

        let guide = contract
            .buttons
            .iter()
            .find(|b| b.button == "guide")
            .expect("guide is in the list, it is just bound to nothing");
        assert!(
            guide.actions.is_empty(),
            "guide belongs to the OS: {:?}",
            guide.actions
        );
        assert_eq!(guide.label, "PS", "labels follow the connected family");

        let x = contract
            .buttons
            .iter()
            .find(|b| b.button == "x")
            .expect("x is in the list");
        assert!(x.actions.contains_key("page"));
        assert!(
            !x.actions.contains_key("dialog"),
            "a contextual action belongs to the page behind the scrim"
        );

        assert_eq!(contract.hold_to_confirm_ms, 800);
        assert_eq!(
            contract.destructive.len(),
            5,
            "the hold gate covers five things and nothing else"
        );

        // An unrecognised family must report what it resolved to, so a caller is
        // never silently given a different glyph set than it asked for.
        let fallback = gamepad_contract(Some("dualshock".to_string())).expect("pure lookup");
        assert_ne!(fallback.family, "PlayStation controller");
    }
}
