//! Starting the game, and staying with it until it stops.
//!
//! # Why this is in the core and not in a front end
//!
//! The launch sequence used to live only in the command line tool. That meant
//! the window could show a Play button and had no way to press it: the
//! interface had five commands and every one of them was read-only. A sequence
//! that only one caller can perform is a sequence the other caller will
//! eventually reimplement slightly differently, and then the two will disagree
//! about which controller file was written.
//!
//! So the order lives here, once, and both the window and the command line ask
//! for it.
//!
//! # The order, and why it is this order
//!
//! 1. **Take the single-instance lock first.** Two copies of the game booting
//!    the same portable user directory write the same memory card, and the
//!    loser's franchise is whatever happens to be flushed last.
//! 2. **Recover anything left unfinished.** If the launcher died with the game
//!    last time, that record is the only evidence it happened.
//! 3. **Regenerate the emulator configuration.** Every launch, from scratch, so
//!    a change made inside Dolphin never persists and the launcher's settings
//!    stay the single source of truth.
//! 4. **Ask the runtime what controllers exist, then write the pad file.**
//!    Asked at launch rather than remembered: a pad can be swapped between
//!    sessions, and a remembered device string that no longer matches binds
//!    nothing without saying so.
//! 5. **Start the game and wait**, then classify and record how it ended.

use std::path::{Path, PathBuf};

use crate::controller::{ControllerProfile, GcPadConfig};
use crate::devices::{DeviceScanner, Enumeration};
use crate::ini::IniDocument;
use crate::platform::{DataDir, Error, Platform, Result};
use crate::runtime::{
    ConfigGenerator, GameBuild, GraphicsPreset, LaunchConfig, Launcher, SessionOutcome,
};
use crate::supervisor::{SessionEnd, Startup, Supervisor};

/// What the caller asks for.
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub runtime_exe: PathBuf,
    pub game_path: PathBuf,
    pub graphics: GraphicsPreset,
    pub fullscreen: bool,
    pub build: GameBuild,
    /// Boot without any mod applied, this once. The selection in
    /// activation.json is left as it is. The recovery page's answer to a
    /// crash that a mod might have caused.
    pub mods_disabled: bool,
    /// The Dolphin user directory to boot against.
    ///
    /// `None` means "find the one that already exists", which is almost always
    /// what a player wants and is what this field exists to fix.
    ///
    /// The launcher used to create a fresh empty directory here and hand it to
    /// Dolphin with `-u`. Measured on a real install, that directory held 21
    /// files while the player's own held 35,015: no memory cards, no
    /// `GameSettings`, and none of the 27,025 files of their texture pack. The
    /// game booted to factory defaults with no save data visible.
    pub user_dir: Option<PathBuf>,
}

/// Why a launch could not even be attempted.
///
/// Separate from an error, because none of these are faults: they are states a
/// player can be in, and each one has a different thing to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The runtime is not where the caller said it was.
    NoRuntime { path: PathBuf },
    /// The game image is not where the caller said it was.
    NoGame { path: PathBuf },
    /// Another launcher holds the lock.
    AlreadyRunning,
}

impl Refusal {
    /// A sentence a player can act on.
    pub fn message(&self) -> String {
        match self {
            Refusal::NoRuntime { path } => format!(
                "No Reborn Dolphin at {}. Set the runtime in Settings.",
                path.display()
            ),
            Refusal::NoGame { path } => format!(
                "No game image at {}. Set the game in Settings.",
                path.display()
            ),
            Refusal::AlreadyRunning => {
                "Another launcher is already running. Close it first.".to_string()
            }
        }
    }
}

/// What happened.
#[derive(Debug, Clone)]
pub enum Launched {
    /// The game ran and stopped.
    Finished {
        end: SessionEnd,
        outcome: SessionOutcome,
        minutes: f64,
        /// Which pad went to which port, so the caller can report it.
        pads: Vec<(u8, String)>,
        /// True when the pad list came from the runtime rather than the built in
        /// scan, which is the only authoritative source.
        pads_authoritative: bool,
        /// Which user directory the session booted against.
        user_dir: UserDir,
        /// Whether the game was pointed at the active mods, and what that
        /// took. Never stops a launch; see [`ModsAtLaunch`].
        mods: ModsAtLaunch,
        /// The session record's id, which is what a capture is filed under.
        session_id: String,
        /// What happened to the match results Dolphin wrote during the
        /// session. Never fails a launch that already happened.
        stats: StatsAfterSession,
    },
    /// Nothing was started, and this is why.
    Refused(Refusal),
}

/// What happened to the mods on the way to a launch.
///
/// Its own type rather than a `Result`, because a failure here is REPORTED
/// and the launch goes ahead. The worst case is stale button art or a stale
/// football over a game that runs, and refusing to start the game over that
/// would be the worse outcome. What must not happen is the failure being
/// swallowed, which is why this is carried in the launch result rather than
/// logged and dropped.
#[derive(Debug, Clone)]
pub enum ModsAtLaunch {
    Synced(crate::mods::SyncOutcome),
    Failed(String),
}

impl ModsAtLaunch {
    /// One line for a player, naming what the game is loading.
    pub fn explain(&self) -> String {
        match self {
            ModsAtLaunch::Synced(outcome) => outcome.explain(),
            ModsAtLaunch::Failed(why) => format!("Mods were not applied: {why}"),
        }
    }
}

/// What happened to the match results after the game closed.
///
/// Dolphin writes one JSON capture per finished match into the user
/// directory. Moving those into the launcher's queue is done here, in the
/// one launch sequence, because a capture left in Dolphin's folder is a
/// result nobody will ever submit: the folder is only read by whoever
/// remembers to. Nothing is uploaded here. Submitting cannot be taken back
/// and stays an explicit act.
///
/// Its own type for the same reason as [`ModsAtLaunch`]: a failure here is
/// reported, never turned into a failed launch of a game that already ran.
#[derive(Debug, Clone)]
pub enum StatsAfterSession {
    /// One entry per capture file found, including the ones refused.
    Ingested(Vec<crate::stats::Enqueued>),
    Failed(String),
}

impl StatsAfterSession {
    /// One line for a player. Silent when there was nothing to do, because
    /// most sessions end without a finished match.
    pub fn explain(&self) -> Option<String> {
        use crate::stats::Enqueued;
        match self {
            StatsAfterSession::Failed(why) => {
                Some(format!("Match results were not collected: {why}"))
            }
            StatsAfterSession::Ingested(results) if results.is_empty() => None,
            StatsAfterSession::Ingested(results) => {
                let queued = results
                    .iter()
                    .filter(|r| matches!(r, Enqueued::Queued(_)))
                    .count();
                let held = results
                    .iter()
                    .filter(|r| matches!(r, Enqueued::Quarantined(_)))
                    .count();
                let known = results
                    .iter()
                    .filter(|r| {
                        matches!(
                            r,
                            Enqueued::AlreadyQueued(_) | Enqueued::AlreadySubmitted { .. }
                        )
                    })
                    .count();
                let unread = results
                    .iter()
                    .filter(|r| matches!(r, Enqueued::CouldNotRead { .. }))
                    .count();
                let mut parts = Vec::new();
                if queued > 0 {
                    parts.push(format!("{queued} match result(s) queued for submission"));
                }
                if held > 0 {
                    parts.push(format!("{held} held for review"));
                }
                if known > 0 {
                    parts.push(format!("{known} already known"));
                }
                if unread > 0 {
                    parts.push(format!("{unread} could not be read and were left in place"));
                }
                Some(format!("{}.", parts.join(", ")))
            }
        }
    }
}

/// Everything written for a launch, reported so a caller can show it.
#[derive(Debug, Clone)]
pub struct WrittenConfig {
    pub files: Vec<PathBuf>,
    /// Files already holding the wanted values, so nothing was written.
    pub unchanged: Vec<PathBuf>,
    /// How many individual keys were set. Reported because "3 files written"
    /// used to mean "3 files replaced", which was the bug.
    pub keys_set: usize,
    pub pads: Vec<(u8, String)>,
    pub pads_authoritative: bool,
    /// Present when no controller could be found, so the caller can say so
    /// rather than launching silently into nothing.
    pub no_controller: bool,
    /// Which user directory this launch will boot against, and why.
    pub user_dir: UserDir,
    /// What the recorded controls profile did to the activation.
    ///
    /// `None` when first run never recorded one, which is not a default to fill
    /// in: an unanswered question is different from a player choosing Xbox, and
    /// silently applying a pack nobody picked is how the wrong buttons appear
    /// with nothing to explain them.
    pub controls: Option<crate::mods::ControlsApplied>,
}

impl Default for WrittenConfig {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            unchanged: Vec::new(),
            keys_set: 0,
            pads: Vec::new(),
            pads_authoritative: false,
            no_controller: false,
            // A path is required, and an empty one is obviously not a real
            // directory, so a caller that forgets to set it cannot mistake it
            // for a resolved one.
            user_dir: UserDir::Fresh(PathBuf::new()),
            // Absent, not a profile. Nothing has been read yet, and a default
            // here would be a choice nobody made.
            controls: None,
        }
    }
}

/// Where Dolphin's user directory is.
///
/// Resolution order, and the reasoning:
///
/// 1. Whatever the caller asked for. A player who has said which folder to use
///    is never overridden.
/// 2. An existing install's `User` folder, if one is found. This is the case
///    that was broken: their settings, memory cards, texture pack and
///    savestates all live there, and booting beside it looks identical to the
///    launcher having done nothing.
/// 3. The launcher's own `runtime/user`, created empty.
///
/// Falling through to 3 is reported rather than silent, because a first launch
/// against an empty directory is a legitimate state and an indistinguishable
/// one from the bug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserDir {
    /// The caller named it.
    Chosen(PathBuf),
    /// The `User` folder beside the emulator itself: a portable Dolphin's
    /// own. It outranks every other candidate because it is the folder that
    /// emulator's owner plays from, and booting that emulator against any
    /// other folder is the launch Adam reported as "the wrong user folder".
    Portable(PathBuf),
    /// Found beside an existing install.
    Existing(PathBuf),
    /// Nothing was found, so the launcher will use its own empty directory.
    Fresh(PathBuf),
}

impl UserDir {
    pub fn path(&self) -> &Path {
        match self {
            UserDir::Chosen(p) | UserDir::Portable(p) | UserDir::Existing(p) | UserDir::Fresh(p) => p,
        }
    }

    /// Whether this directory already holds the player's own content.
    pub fn is_populated(&self) -> bool {
        !matches!(self, UserDir::Fresh(_))
    }

    /// A sentence naming which directory is in use and why, so a launch that
    /// boots to defaults says so instead of looking broken.
    pub fn explain(&self) -> String {
        match self {
            UserDir::Chosen(p) => format!("Using the chosen user folder at {}.", p.display()),
            UserDir::Portable(p) => format!(
                "Using the user folder beside the emulator, {}, which is that emulator's own.",
                p.display()
            ),
            UserDir::Existing(p) => {
                format!("Using the existing user folder at {}.", p.display())
            }
            UserDir::Fresh(p) => format!(
                "No existing user folder was found, so a new empty one at {} \
                 will be used. Settings, memory cards and texture packs will \
                 not be present until one is set in Settings.",
                p.display()
            ),
        }
    }
}

/// Find the user directory to boot against.
///
/// Ranked by evidence of real content, not by which candidate came first.
///
/// The first version of this checked for a `Config` directory and took the
/// first hit. That picked the launcher's OWN `runtime/User`, because the very
/// bug being fixed had already created a `Config` folder there. A test for
/// "does this look like a Dolphin user directory" was satisfied by the
/// wreckage of the thing it was meant to detect.
///
/// So the test is now for content only a real install has: a memory card, a
/// populated texture load path, or per-game settings. A directory holding
/// nothing but the handful of INIs the launcher itself writes scores zero and
/// loses to a genuine one.
pub fn resolve_user_dir(platform: &Platform, chosen: Option<&Path>) -> Result<UserDir> {
    resolve_user_dir_for(platform, chosen, None)
}

/// Find the user directory to boot THIS emulator against.
///
/// A chosen folder wins. Then the emulator's own: a `User` folder beside the
/// executable, which a portable Dolphin (one with `portable.txt` beside it)
/// always has and which holds that emulator's saves, settings and textures.
/// Only then the global candidates, ranked by evidence. The bundled runtime
/// ships no `User` folder, so for it this falls through to the 2.1 install's,
/// which is where an upgrading player's saves are.
///
/// Observed before this existed: a chosen portable Dolphin was booted against
/// the 2.1 install's folder, so its own settings, saves and texture pack were
/// all absent from the game that came up.
pub fn resolve_user_dir_for(
    platform: &Platform,
    chosen: Option<&Path>,
    runtime_exe: Option<&Path>,
) -> Result<UserDir> {
    if let Some(c) = chosen {
        return Ok(UserDir::Chosen(c.to_path_buf()));
    }

    if let Some(dir) = runtime_exe.and_then(Path::parent) {
        let beside = dir.join("User");
        let portable = dir.join("portable.txt").is_file();
        if beside.is_dir() && (portable || user_dir_score(&beside) > 0) {
            return Ok(UserDir::Portable(beside));
        }
    }

    let mut best: Option<(u32, PathBuf)> = None;
    for user in candidate_user_dirs(platform)? {
        let score = user_dir_score(&user);
        if score == 0 {
            continue;
        }
        if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
            best = Some((score, user));
        }
    }

    if let Some((_, path)) = best {
        return Ok(UserDir::Existing(path));
    }

    Ok(UserDir::Fresh(
        platform
            .paths
            .data_root()?
            .join(DataDir::Runtime.relative())
            .join("user"),
    ))
}

/// How much real player content a directory holds.
///
/// Zero means "not a populated user directory", whatever else is in it.
fn user_dir_score(user: &Path) -> u32 {
    if !user.is_dir() {
        return 0;
    }
    let mut score = 0;

    // A memory card is the strongest signal and the most important thing to
    // get right: booting without one shows the player no save data at all.
    if let Ok(entries) = std::fs::read_dir(user.join("GC")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("memorycard") && name.ends_with(".raw") {
                score += 100;
                break;
            }
        }
    }

    // A populated texture load path. Counted shallowly on purpose: a real one
    // holds tens of thousands of files and walking it to decide a launch would
    // cost seconds.
    if has_any_entry(&user.join("Load")) {
        score += 40;
    }

    // Per-game settings, which carry the patches a modded game depends on.
    if has_any_entry(&user.join("GameSettings")) {
        score += 30;
    }

    if has_any_entry(&user.join("StateSaves")) {
        score += 10;
    }

    score
}

fn has_any_entry(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false)
}

/// User directories worth scoring.
fn candidate_user_dirs(platform: &Platform) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();

    // The shipped product's own location, named through the environment rather
    // than hardcoded to one machine's profile.
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let base = PathBuf::from(&appdata);
        out.push(base.join("NFL Street Reborn").join("User"));
        // Stock Dolphin, in case that is where a player kept things.
        out.push(base.join("Dolphin Emulator"));
    }

    // A runtime unpacked beside the launcher's own data, checked last so its
    // own scaffolding can never outrank a real install.
    out.push(
        platform
            .paths
            .data_root()?
            .join(DataDir::Runtime.relative())
            .join("User"),
    );

    Ok(out)
}

/// The launch sequence.
#[derive(Debug)]
pub struct Session;

impl Session {
    /// Write every configuration file a launch needs, and report what was
    /// written. Split out from [`Session::run`] so a caller can offer a dry run
    /// without duplicating the order.
    pub fn prepare(platform: &Platform, req: &LaunchRequest) -> Result<WrittenConfig> {
        crate::runtime::ensure_runtime_idle(&req.runtime_exe)?;
        let prefs=crate::settings::Settings::load(&crate::settings::settings_path(platform)?)?;
        if let Some(text)=prefs.graphics_override.as_ref().filter(|o|o.label=="Previous settings").and_then(|o|o.legacy_reshade.as_deref()) {
            crate::reshade::apply_imported(&req.runtime_exe,text)?;
        }else{crate::reshade::apply(&req.runtime_exe, if prefs.graphics_override.as_ref().is_some_and(|o|o.label=="Previous settings"){GraphicsPreset::Performance}else{req.graphics})?;}
        let user = resolve_user_dir_for(platform, req.user_dir.as_deref(), Some(&req.runtime_exe))?;
        let user_dir = user.path().to_path_buf();
        let config_dir = user_dir.join("Config");
        std::fs::create_dir_all(&config_dir).map_err(|source| Error::Io {
            path: config_dir.clone(),
            source,
        })?;

        let config = LaunchConfig {
            build: req.build,
            graphics: req.graphics,
            fullscreen: req.fullscreen,
            game_path: req.game_path.clone(),
            user_dir: user_dir.clone(),
            match_context: None,
        };

        let mut written = WrittenConfig {
            user_dir: user.clone(),
            ..Default::default()
        };

        // The button art, reconciled on every launch beside the configuration.
        //
        // Here rather than in first run because that is what makes changing it
        // later work: a new profile is recorded and the next launch applies it,
        // with no first run to walk again and nothing to re-apply by hand.
        //
        // A failure here does not stop the launch. The worst case is the wrong
        // buttons drawn over a game that runs, and refusing to start over
        // button art would be a worse outcome than the art being wrong.
        written.controls = reconcile_controls(platform).unwrap_or(None);

        // Keys, not files. Writing whole files here deleted ten sections of a
        // real player's Dolphin.ini, including the paths to their own game
        // images. See crate::ini for the measurements.
        let preferences = crate::settings::Settings::load(&crate::settings::settings_path(platform)?)?;
        for (name, mut overlay) in ConfigGenerator::overlays(&config) {
            if name == "GFX.ini" {
                let options = preferences.graphics_override.as_ref();
                // Retain legacy visual choices, while the compatibility fixes below still win.
                if options.is_some_and(|o|o.label=="Previous settings") {
                    if let Some(section)=overlay.get_mut("Settings") { section.remove("MaxAnisotropy"); }
                }
                if let Some(previous)=options.filter(|o|o.label=="Previous settings").and_then(|o|o.legacy_gfx.as_deref()) {
                    let mut retained=crate::ini::Overlay::new();let mut section=String::new();
                    for line in previous.lines(){let line=line.trim();if line.starts_with('[')&&line.ends_with(']'){section=line[1..line.len()-1].into();}else if !line.starts_with([';','#']){if let Some((key,value))=line.split_once('='){retained.entry(section.clone()).or_default().insert(key.trim().into(),value.trim().into());}}}
                    for (section,keys) in overlay{retained.entry(section).or_default().extend(keys);}overlay=retained;
                }
                let section=overlay.entry("Settings".into()).or_default();
                section.insert("InternalResolution".into(),"3".into());
                section.insert("MSAA".into(),"1".into());
                section.insert("SSAA".into(),"False".into());
            }
            let path = config_dir.join(&name);

            let existing = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(source) => {
                    return Err(Error::Io {
                        path: path.clone(),
                        source,
                    })
                }
            };

            // A record of what the file looked like before the launcher ever
            // touched it, written once and never read back automatically.
            // Automatic restore from a stale copy is its own failure mode.
            if !existing.is_empty() {
                let original = path.with_extension("ini.launcher-original");
                if !original.exists() {
                    std::fs::write(&original, &existing).map_err(|source| Error::Io {
                        path: original.clone(),
                        source,
                    })?;
                }
            }

            let mut doc = IniDocument::parse(&existing);
            let report = doc.merge(&overlay);

            // Nothing to change means nothing to write. A launcher that
            // rewrites a config on every start looks like it is corrupting it.
            if report.is_noop() && !existing.is_empty() {
                written.unchanged.push(path);
                continue;
            }

            std::fs::write(&path, doc.render()).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            written.keys_set += report.updated.len() + report.added.len();
            written.files.push(path);
        }

        // Per-game overrides take precedence over Dolphin.ini, including old
        // installations. Keep required emulated memory consistent in both places.
        let mut game_ids=vec!["GN7E69",req.build.game_id()];game_ids.sort();game_ids.dedup();
        for game_id in game_ids {
        let game_settings=user_dir.join("GameSettings").join(format!("{game_id}.ini"));
        std::fs::create_dir_all(game_settings.parent().unwrap())?;
        let existing=std::fs::read_to_string(&game_settings).unwrap_or_default();
        let mut doc=crate::ini::IniDocument::parse(&existing);
        let report=doc.merge(&ConfigGenerator::required_game_settings());
        if !report.is_noop(){
            if !existing.is_empty() && !game_settings.with_extension("ini.launcher-original").exists(){std::fs::write(game_settings.with_extension("ini.launcher-original"),&existing)?;}
            std::fs::write(&game_settings,doc.render())?;written.keys_set+=report.updated.len()+report.added.len();written.files.push(game_settings);
        }else{written.unchanged.push(game_settings);}

        }
        // Ask the runtime that is about to be launched. The built in scan sees
        // XInput only, which on a real machine misses an entire backend.
        let scratch = platform.paths.data_root()?.join(DataDir::Cache.relative());
        let (scan, authoritative) =
            match crate::devices::scan_via_runtime(platform, &req.runtime_exe, &scratch) {
                Ok(s) => (s, true),
                Err(_) => (DeviceScanner::scan(), false),
            };
        written.pads_authoritative = authoritative;

        crate::controller_profiles::adopt_migrated(platform, &scan)?;
        let saved_profiles = crate::controller_profiles::load(&crate::controller_profiles::path(platform)?)?;
        let preferences = crate::settings::Settings::load(&crate::settings::settings_path(platform)?)?;
        match build_gcpad_for_slots(&scan, &saved_profiles, preferences.primary_controller.as_deref(), preferences.controller_slots.as_ref()) {
            Some((ini, assignment)) => {
                let path = config_dir.join("GCPadNew.ini");
                // The pad file IS wholly owned by the launcher: it is
                // regenerated from the live device list every launch by
                // design, because a remembered device string that no longer
                // matches binds nothing without saying so. Still recorded
                // once before the first overwrite.
                if let Ok(prev) = std::fs::read_to_string(&path) {
                    let original = path.with_extension("ini.launcher-original");
                    if !original.exists() && !prev.is_empty() {
                        let _ = std::fs::write(&original, &prev);
                    }
                }
                std::fs::write(&path, ini).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
                written.files.push(path);
                written.pads = assignment;
                let dolphin_file=config_dir.join("Dolphin.ini");
                let dolphin_text = match std::fs::read_to_string(&dolphin_file) {
                    Ok(text) => text,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(error) => return Err(error.into()),
                };
                let mut dolphin_doc=crate::ini::IniDocument::parse(&dolphin_text);
                let overlay=controller_port_overlay(&written.pads);
                if !dolphin_doc.merge(&overlay).is_noop(){
                    std::fs::write(&dolphin_file,dolphin_doc.render())?;
                    if !written.files.contains(&dolphin_file) { written.files.push(dolphin_file); }
                }
                written.no_controller = written.pads.is_empty();

            }
            None => written.no_controller = true,
        }

        Ok(written)
    }

    /// Start the game and block until it stops.
    ///
    /// Blocking is deliberate: something has to hold the lock and the session
    /// record for as long as the game runs, and that something is whoever
    /// called this. A window calls it off the interface thread.
    pub fn run(platform: &Platform, req: &LaunchRequest) -> Result<Launched> {
        if !req.runtime_exe.exists() {
            return Ok(Launched::Refused(Refusal::NoRuntime {
                path: req.runtime_exe.clone(),
            }));
        }
        if !req.game_path.exists() {
            return Ok(Launched::Refused(Refusal::NoGame {
                path: req.game_path.clone(),
            }));
        }

        // The lock, before anything is written or started.
        let Startup::Owner(supervisor) = Supervisor::acquire(platform)? else {
            return Ok(Launched::Refused(Refusal::AlreadyRunning));
        };
        let _ = supervisor.recover_unfinished()?;

        let written = Self::prepare(platform, req)?;

        // The mods, made real. prepare reconciled the activation list; this
        // builds the tree that list describes and points the emulator at it.
        // Before this call existed every profile switch wrote the list
        // correctly and the game never saw it.
        let mods = sync_mods_for_launch(platform, &supervisor, req.build, written.user_dir.path(), req.mods_disabled);

        let config = LaunchConfig {
            build: req.build,
            graphics: req.graphics,
            fullscreen: req.fullscreen,
            game_path: req.game_path.clone(),
            // The same directory prepare just merged into, never a fresh one.
            user_dir: written.user_dir.path().to_path_buf(),
            match_context: None,
        };

        let record = supervisor.begin(req.build)?;
        let handle = Launcher::launch(platform, &req.runtime_exe, &config)?;
        let exit = platform.processes.wait(handle)?;
        let finished = supervisor.finish(record, &exit)?;

        // The match results, collected while the lock is still held and the
        // session id is still in hand. Filed under this session so a capture
        // can always be traced to the launch that produced it.
        let stats = {
            let capture_dir = crate::stats::dolphin_capture_dir(written.user_dir.path());
            match crate::stats::StatsQueue::open(platform)
                .and_then(|queue| queue.ingest_from(&capture_dir, Some(&finished.id)))
            {
                Ok(results) => StatsAfterSession::Ingested(results),
                Err(e) => StatsAfterSession::Failed(e.to_string()),
            }
        };

        let end = finished
            .end
            .clone()
            .unwrap_or(SessionEnd::Exited { code: None });
        Ok(Launched::Finished {
            outcome: end.outcome(),
            end,
            minutes: finished.duration_ms().unwrap_or(0) as f64 / 60_000.0,
            pads: written.pads,
            pads_authoritative: written.pads_authoritative,
            user_dir: written.user_dir,
            mods,
            session_id: finished.id.clone(),
            stats,
        })
    }
}

/// Point this build at what `activation.json` says is on.
///
/// Every error is caught and carried, never propagated: see [`ModsAtLaunch`].
fn sync_mods_for_launch(
    platform: &Platform,
    lock: &Supervisor,
    build: GameBuild,
    user_dir: &Path,
    disabled: bool,
) -> ModsAtLaunch {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let attempt = (|| -> Result<crate::mods::SyncOutcome> {
        let game = crate::mods::GameId::parse(build.game_id())?;
        let mut req = crate::mods::SyncRequest::in_data_root(platform, game, user_dir)?;
        let settings_file = crate::settings::settings_path(platform)?;
        req.link_into_user_dir = crate::settings::manage_texture_folder(&settings_file)?;
        if disabled {
            req.active_override = Some(Vec::new());
        }
        crate::mods::sync_active(platform, lock, &req, now_ms)
    })();
    match attempt {
        Ok(outcome) => ModsAtLaunch::Synced(outcome),
        Err(e) => ModsAtLaunch::Failed(e.to_string()),
    }
}

/// Build `GCPadNew.ini` from whatever the runtime says is plugged in.
///
/// Returns the file contents and the port assignment, or `None` when there is
/// nothing to bind.
pub fn build_gcpad(scan: &Enumeration, ports: u8) -> Option<(String, Vec<(u8, String)>)> {
    build_gcpad_with_profiles(scan, ports, &[])
}

pub fn build_gcpad_with_profiles(scan: &Enumeration, ports: u8, saved: &[ControllerProfile]) -> Option<(String, Vec<(u8, String)>)> {
    build_gcpad_for_player(scan, ports, saved, None)
}

/// Explicit player slots stay fixed even when a controller disconnects.
pub fn build_gcpad_for_slots(scan: &Enumeration, saved: &[ControllerProfile], primary: Option<&str>, slots: Option<&[Option<String>;4]>) -> Option<(String, Vec<(u8,String)>)> {
    let Some(slots)=slots else { return build_gcpad_for_player(scan,4,saved,primary); };
    let pads=scan.playable_pads();let mut used=std::collections::HashSet::new();
    let mut profiles=Vec::new();let mut assignment=Vec::new();
    for (index,device) in slots.iter().enumerate(){
        let Some(device)=device else {continue};
        if !used.insert(device){continue;}
        let Some(pad)=pads.iter().find(|p|p.device_string()==*device) else {continue};
        let mut profile=saved.iter().find(|p|p.identity.profile_key()==pad.identity().profile_key()).cloned().unwrap_or_else(||ControllerProfile::new(pad.identity(),index as u8+1));
        profile.port=index as u8+1;profiles.push(profile);assignment.push((index as u8+1,device.clone()));
    }
    let mut ini=GcPadConfig::generate(&profiles,|id|pads.iter().find(|p|p.identity().profile_key()==id.profile_key()).map(|p|p.device_string()).unwrap_or_default());
    for port in 1..=4 {if !assignment.iter().any(|(p,_)|*p==port){ini.push_str(&format!("[GCPad{port}]\nDevice = \n\n"));}}
    Some((ini,assignment))
}

pub fn controller_port_overlay(assignment:&[(u8,String)])->crate::ini::Overlay {
    let mut overlay=crate::ini::Overlay::new();
    for index in 0..4 {overlay.entry("Core".into()).or_default().insert(format!("SIDevice{index}"),if assignment.iter().any(|(port,_)|*port==index+1){"6"}else{"0"}.into());}
    overlay
}

pub fn build_gcpad_for_player(scan: &Enumeration, ports: u8, saved: &[ControllerProfile], primary: Option<&str>) -> Option<(String, Vec<(u8, String)>)> {
    let mut pads = scan.playable_pads();
    pads.sort_by_key(|pad| Some(pad.device_string().as_str()) != primary);
    if pads.is_empty() {
        return None;
    }

    let profiles: Vec<ControllerProfile> = pads
        .iter()
        .take(ports as usize)
        .enumerate()
        .map(|(i, d)| {
            let mut profile = saved.iter().find(|p| p.identity.profile_key()==d.identity().profile_key()).cloned()
                .unwrap_or_else(|| ControllerProfile::new(d.identity(), (i + 1) as u8));
            profile.port=(i+1) as u8;
            profile
        })
        .collect();

    let by_key: std::collections::BTreeMap<String, String> = pads
        .iter()
        .map(|d| (d.identity().profile_key(), d.device_string()))
        .collect();

    let ini = GcPadConfig::generate(&profiles, |id| {
        by_key
            .get(&id.profile_key())
            .cloned()
            // Never invent one. A made up Device line parses fine and binds
            // nothing, which looks exactly like a broken controller.
            .unwrap_or_else(|| "UNKNOWN-DEVICE".to_string())
    });

    let assignment = pads
        .iter()
        .take(ports as usize)
        .enumerate()
        .map(|(i, d)| ((i + 1) as u8, d.device_string()))
        .collect();
    Some((ini, assignment))
}
// ---------------------------------------------------------------------------
// Finding the game image
// ---------------------------------------------------------------------------

/// Why a particular image was picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameImageSource {
    /// The emulator's own settings name it as the default. The strongest
    /// evidence there is: it is what that emulator boots when nothing else is
    /// said, recorded by the person who set it up.
    RecordedDefault { ini: PathBuf },
    /// `NSR.iso` by name, the name the product ships under.
    NamedNsr { folder: PathBuf },
    /// The only image anywhere the launcher looked.
    OnlyOne { folder: PathBuf },
}

/// The outcome of looking for the image to boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameSearch {
    Found { path: PathBuf, source: GameImageSource },
    /// More than one, none of them decisive. NOT picked: the launch that took
    /// the alphabetically first of eleven files booted ATL.iso on a machine
    /// whose emulator had NSR.iso recorded as its default.
    Several { candidates: Vec<PathBuf>, looked: Vec<PathBuf> },
    None { looked: Vec<PathBuf> },
}

impl GameSearch {
    pub fn path(&self) -> Option<&Path> {
        match self {
            GameSearch::Found { path, .. } => Some(path),
            _ => None,
        }
    }

    /// A sentence for the player: what will boot and why, or why nothing will.
    pub fn explain(&self) -> String {
        fn list(paths: &[PathBuf]) -> String {
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("; ")
        }
        match self {
            GameSearch::Found { path, source } => match source {
                GameImageSource::RecordedDefault { .. } => format!(
                    "Using {}, the image the emulator's own settings name as its default.",
                    path.display()
                ),
                GameImageSource::NamedNsr { folder } => {
                    format!("Using {}, found by name in {}.", path.display(), folder.display())
                }
                GameImageSource::OnlyOne { folder } => {
                    format!("Using {}, the only game image in {}.", path.display(), folder.display())
                }
            },
            GameSearch::Several { candidates, .. } => format!(
                "{} game images were found and none is NSR.iso, so none was picked: {}. \
                 Set the one to play on the Settings page.",
                candidates.len(),
                candidates
                    .iter()
                    .filter_map(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            GameSearch::None { looked } => format!(
                "No game image was found. Looked in: {}. Set its path on the Settings \
                 page, or point the launcher at your existing NFL Street Reborn install.",
                if looked.is_empty() { "nowhere that exists".to_string() } else { list(looked) }
            ),
        }
    }
}

/// Find the image to boot, without guessing.
///
/// In order: the default the emulator's own `Dolphin.ini` records, if that
/// file still exists; `NSR.iso` by name in the launcher's own `Games`, the
/// emulator's `Games`, and the existing install's `Games`; the only image in
/// those folders if there is exactly one. Anything else is a refusal that
/// names the candidates, because a wrong image that boots looks exactly like
/// the right one until the menu comes up.
pub fn find_game_image(
    runtime_exe: &Path,
    user_dir: &Path,
    legacy_root: Option<&Path>,
    launcher_dir: Option<&Path>,
) -> GameSearch {
    let ini = user_dir.join("Config").join("Dolphin.ini");
    if let Ok(text) = std::fs::read_to_string(&ini) {
        if let Some(value) = IniDocument::parse(&text).get("Core", "DefaultISO") {
            let recorded = PathBuf::from(value.trim());
            if !value.trim().is_empty() && recorded.is_file() {
                return GameSearch::Found {
                    path: recorded,
                    source: GameImageSource::RecordedDefault { ini },
                };
            }
        }
    }

    let mut folders: Vec<PathBuf> = Vec::new();
    for base in [launcher_dir, runtime_exe.parent(), legacy_root].into_iter().flatten() {
        let games = base.join("Games");
        if games.is_dir() && !folders.contains(&games) {
            folders.push(games);
        }
    }

    let mut all: Vec<(PathBuf, PathBuf)> = Vec::new();
    for folder in &folders {
        let Ok(entries) = std::fs::read_dir(folder) else { continue };
        let mut here: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_game_image(p))
            .collect();
        here.sort();
        for image in &here {
            let named = image
                .file_name()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case("nsr.iso"))
                .unwrap_or(false);
            if named {
                return GameSearch::Found {
                    path: image.clone(),
                    source: GameImageSource::NamedNsr { folder: folder.clone() },
                };
            }
        }
        for image in here {
            all.push((folder.clone(), image));
        }
    }

    match all.len() {
        0 => GameSearch::None { looked: folders },
        1 => {
            let (folder, path) = all.remove(0);
            GameSearch::Found { path, source: GameImageSource::OnlyOne { folder } }
        }
        _ => GameSearch::Several {
            candidates: all.into_iter().map(|(_, p)| p).collect(),
            looked: folders,
        },
    }
}

// ---------------------------------------------------------------------------
// Finding the emulator
// ---------------------------------------------------------------------------

/// Read the recorded controls profile and make the activation match it.
///
/// Returns `None` when first run recorded no profile, which is the honest
/// answer for a machine that has not been set up: there is nothing to apply,
/// and picking a default here would apply art nobody chose.
///
/// Reads the onboarding journal directly rather than taking a profile as an
/// argument, for the same reason every other launch value is read rather than
/// passed: the journal is where the answer was recorded, and an argument could
/// carry a profile the player never selected.
fn reconcile_controls(
    platform: &Platform,
) -> Result<Option<crate::mods::ControlsApplied>> {
    use crate::onboarding::{Environment, Onboarding};

    // Settings first, first run as the seed. The journal latches and stays
    // closed, so a preference read only from there could never be changed
    // without re-walking the wizard; see crate::settings for why they are
    // separate files.
    let flow = Onboarding::open(platform, Environment::nothing_found())?;
    let settings_file = crate::settings::settings_path(platform)?;
    let Some(profile) = crate::settings::effective_controls_profile(
        &settings_file,
        flow.summary().controls_profile,
    )?
    else {
        return Ok(None);
    };

    // Only the packages the profile could want. Reading the whole library to
    // apply one pack would make every launch wait on a full library scan.
    let root = crate::mods::packages_root(platform)?;
    let mut installed = Vec::new();
    if let Some(id) = profile.package_id() {
        let dir = root.join(id);
        if let Ok(versions) = std::fs::read_dir(&dir) {
            for version in versions.flatten() {
                if let Ok(package) = crate::mods::ModPackage::load(&version.path()) {
                    installed.push(package);
                }
            }
        }
    }

    let activation_file = crate::mods::activation_path(platform)?;
    crate::mods::apply_controls_profile(&activation_file, profile, &installed).map(Some)
}

/// A path typed or pasted by a person, cleaned up.
///
/// Windows Explorer's "Copy as path" wraps the path in double quotes, and
/// the quotes come along when it is pasted into a box. The result is a path
/// that looks completely correct in every message and matches nothing on
/// disk, so the launcher refuses while printing the right path back:
///
///   No Reborn Dolphin at "C:\\...\\Dolphin.exe". Set the runtime in Settings.
///
/// That is a real refusal from a real session, and the emulator was there.
/// The only clue was the quotes in the message, which read as punctuation.
///
/// Strips ONE matching pair, because a second pair would be part of the name,
/// and trims whitespace, because a pasted path often carries a trailing space
/// or newline. An unbalanced quote is left alone: which half to drop would be
/// a guess, and guessing turns a typo into a different path.
pub fn user_path(text: &str) -> PathBuf {
    let trimmed = text.trim();
    let stripped = strip_pair(trimmed, '"');
    // Single quotes too: pasting out of a shell transcript brings those.
    PathBuf::from(strip_pair(stripped, '\'').trim())
}

/// One matching pair of `quote` from both ends, or the text unchanged.
fn strip_pair(text: &str, quote: char) -> &str {
    let mut chars = text.chars();
    match (chars.next(), chars.next_back()) {
        (Some(first), Some(last)) if first == quote && last == quote => {
            &text[first.len_utf8()..text.len() - last.len_utf8()]
        }
        _ => text,
    }
}

/// Where an emulator came from, and whether it is the one that works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runtime {
    /// The copy that shipped with the launcher, beside its own executable.
    ///
    /// The normal answer on Windows and the only supported one: the launcher is
    /// built against a fork, and a player is not asked which emulator to use
    /// because there is only one right answer.
    Bundled(PathBuf),
    /// The emulator from an NFL Street Reborn 2.1 install.
    ///
    /// Used when the launcher is running without a bundled copy beside it,
    /// which is the case during development and for anyone upgrading before the
    /// new runtime has been laid down. It IS a Reborn fork, so it works.
    FromLegacyInstall(PathBuf),
    /// A path someone chose.
    ///
    /// Permitted on macOS and Linux, where no build is shipped, and it will be
    /// missing the fork's features: this launcher is built against a Dolphin
    /// that reports controllers, writes match results and honours the Reborn
    /// configuration. A stock Dolphin runs the game and does none of that.
    ///
    /// Carried as its own variant rather than folded into the others so nothing
    /// can report a stock emulator as though it were supported.
    Chosen(PathBuf),
}

impl Runtime {
    pub fn path(&self) -> &Path {
        match self {
            Runtime::Bundled(p) | Runtime::FromLegacyInstall(p) | Runtime::Chosen(p) => p,
        }
    }

    /// Whether this is a build the launcher can rely on.
    ///
    /// `Some(true)` for the bundled copy and the one from a 2.1 install, both
    /// of which are known Reborn forks. `None` for [`Runtime::Chosen`]: a path
    /// cannot say whether the build behind it is a Reborn fork or a stock
    /// Dolphin, and a stock Dolphin boots the game so nothing visibly breaks
    /// at launch; what breaks is every feature that reads back from the
    /// emulator, silently. Calling a player's own fork unsupported would cry
    /// wolf, so the honest answer is that it is unknown, and the warning says
    /// what is lost IF it is stock rather than asserting that it is.
    pub fn is_supported(&self) -> Option<bool> {
        match self {
            Runtime::Bundled(_) | Runtime::FromLegacyInstall(_) => Some(true),
            Runtime::Chosen(_) => None,
        }
    }

    /// One line for a player, naming the consequence rather than the category.
    pub fn explain(&self) -> String {
        match self {
            Runtime::Bundled(p) => format!("Using the emulator that came with the launcher, {}.", p.display()),
            Runtime::FromLegacyInstall(p) => format!(
                "Using the emulator from your existing NFL Street Reborn install, {}. \
                 It is a Reborn build, so everything works.",
                p.display()
            ),
            Runtime::Chosen(p) => format!(
                "Using an emulator you chose, {}. Whether it is a Reborn build \
                 cannot be told from here. If it is a stock Dolphin it will run \
                 the game but without controller reporting, match results, or \
                 the Reborn configuration. Those features will be missing rather \
                 than broken, and nothing will say so while you play.",
                p.display()
            ),
        }
    }
}

/// Anything that looks like a Dolphin executable in a directory.
///
/// Matched by name rather than by a fixed filename, because the Reborn build
/// ships as `NFL Street Reborn Dolphin.exe` in the 2.1 install and as
/// `Dolphin.exe` in the launcher's own payload, and hardcoding either one makes
/// the other invisible.
fn dolphin_in(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            name.contains("dolphin")
                && (name.ends_with(".exe") || !name.contains('.'))
        })
        .collect();
    // Sorted so two candidates do not resolve differently between runs.
    found.sort();
    found.into_iter().next()
}

/// Find the emulator, without asking anyone.
///
/// On Windows the player is never asked: the launcher is built against a fork
/// and there is one right answer. `chosen` exists for macOS and Linux, where no
/// build is shipped, and for development.
///
/// The order is deliberate. A bundled copy wins because it is the one this
/// launcher was built against. A 2.1 install comes next because it is also a
/// Reborn build, which makes it correct rather than merely available, and it is
/// what every upgrading player already has. Only then is there nothing to use.
///
/// Returns the places it looked when it finds nothing, so the refusal can name
/// them instead of saying the install is incomplete and leaving it there.
pub fn resolve_runtime(
    platform: &Platform,
    chosen: Option<&Path>,
    legacy_root: Option<&Path>,
) -> std::result::Result<Runtime, Vec<PathBuf>> {
    let mut looked = Vec::new();

    // An explicit choice wins, so a developer or a Linux player is not
    // overruled. It is still reported as chosen, never as supported.
    if let Some(path) = chosen {
        if path.is_file() {
            return Ok(Runtime::Chosen(path.to_path_buf()));
        }
        looked.push(path.to_path_buf());
    }

    // Beside the launcher's own executable, which is where the installer puts
    // it. current_exe can fail, and a failure here is just one place not
    // searched rather than a reason to stop.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [dir.join("runtime"), dir.to_path_buf()] {
                looked.push(candidate.clone());
                if let Some(found) = dolphin_in(&candidate) {
                    return Ok(Runtime::Bundled(found));
                }
            }
        }
    }

    // The 2.1 install. A Reborn build, so this is a correct answer and not a
    // fallback that merely happens to run.
    if let Some(root) = legacy_root {
        looked.push(root.to_path_buf());
        if let Some(found) = dolphin_in(root) {
            return Ok(Runtime::FromLegacyInstall(found));
        }
    }

    let _ = platform;
    Err(looked)
}

/// The refusal, naming every directory that was searched.
///
/// Written here so the CLI and the window say the same thing, and so the
/// message lists places rather than asserting the install is incomplete: on a
/// development build nothing is bundled and that is expected, not broken.
pub fn no_runtime_message(looked: &[PathBuf]) -> String {
    let mut out = String::from("No Reborn emulator was found. Looked in:");
    for path in looked {
        out.push_str(&format!("\n  {}", path.display()));
    }
    if looked.is_empty() {
        out.push_str(" nowhere, because no search path could be worked out.");
    }
    out.push_str(
        "\n\nOn Windows this ships beside the launcher, so a fresh install \
         should always find it. Point at an existing NFL Street Reborn install \
         to use the emulator already there.",
    );
    out
}


/// Where the game image is expected to be, if the caller has not said.
pub fn default_game_path(platform: &Platform) -> Result<PathBuf> {
    Ok(platform
        .paths
        .data_root()?
        .join(DataDir::LibraryOffline.relative())
        .join("game.iso"))
}

/// Whether a path looks like a GameCube image this launcher can boot.
pub fn is_game_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref(),
        Some("iso") | Some("rvz") | Some("gcm") | Some("ciso")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(runtime: PathBuf, game: PathBuf) -> LaunchRequest {
        LaunchRequest {
            runtime_exe: runtime,
            game_path: game,
            graphics: GraphicsPreset::Balanced,
            fullscreen: true,
            build: GameBuild::Offline,
            mods_disabled: false,
            // Tests must never pick up whatever real install happens
            // to be on the machine running them.
            user_dir: Some(PathBuf::from("test-user-dir")),
        }
    }

    /// A missing runtime is a state, not a crash, and the message has to say
    /// what to do about it.
    #[test]
    fn a_missing_runtime_is_refused_with_a_usable_message() {
        let p = Platform::detect();
        let r = req(
            PathBuf::from("does-not-exist.exe"),
            PathBuf::from("does-not-exist.iso"),
        );
        match Session::run(&p, &r).expect("refusal is not an error") {
            Launched::Refused(Refusal::NoRuntime { .. }) => {}
            other => panic!("expected a runtime refusal, got {other:?}"),
        }
        let msg = Refusal::NoRuntime {
            path: PathBuf::from("x"),
        }
        .message();
        assert!(msg.contains("Settings"), "no way forward given: {msg}");
    }

    /// The runtime is checked before the game, because a missing runtime makes
    /// the game path unverifiable anyway.
    #[test]
    fn a_present_runtime_with_no_game_is_refused_for_the_game() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Dolphin.exe");
        std::fs::write(&exe, b"not really an exe").unwrap();
        let p = Platform::detect();
        let r = req(exe, dir.path().join("absent.iso"));
        match Session::run(&p, &r).expect("refusal is not an error") {
            Launched::Refused(Refusal::NoGame { .. }) => {}
            other => panic!("expected a game refusal, got {other:?}"),
        }
    }

    #[test]
    fn every_refusal_says_something_actionable() {
        for r in [
            Refusal::NoRuntime {
                path: PathBuf::from("a"),
            },
            Refusal::NoGame {
                path: PathBuf::from("b"),
            },
            Refusal::AlreadyRunning,
        ] {
            let m = r.message();
            assert!(m.len() > 20, "too terse to act on: {m}");
            assert!(m.ends_with('.'), "not a sentence: {m}");
        }
    }

    #[test]
    fn game_images_are_recognised_by_extension() {
        assert!(is_game_image(Path::new("a/b/NSR.iso")));
        assert!(is_game_image(Path::new("a/b/NSR.RVZ")));
        assert!(is_game_image(Path::new("a/b/NSR.gcm")));
        assert!(!is_game_image(Path::new("a/b/NSR.txt")));
        assert!(!is_game_image(Path::new("a/b/NSR")));
    }

    /// Nothing to bind must produce no file rather than an empty one: an empty
    /// GCPadNew.ini is indistinguishable from a broken pad.
    #[test]
    fn no_controller_produces_no_pad_file() {
        let empty = Enumeration {
            devices: Vec::new(),
            not_searched: Vec::new(),
        };
        assert!(build_gcpad(&empty, 4).is_none());
    }
    /// The detection bug, pinned. A directory holding only the INIs the
    /// launcher itself writes must never be mistaken for a real install: that
    /// is exactly how the first fix picked its own wreckage.
    #[test]
    fn a_config_folder_alone_does_not_look_like_a_real_install() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("launcher-runtime-user");
        std::fs::create_dir_all(fake.join("Config")).unwrap();
        std::fs::write(fake.join("Config").join("Dolphin.ini"), "[Display]\n").unwrap();
        assert_eq!(user_dir_score(&fake), 0, "a Config folder is not evidence");
    }

    /// A memory card is the signal that matters most: booting without one
    /// shows the player no save data at all.
    #[test]
    fn a_memory_card_outranks_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join("GC")).unwrap();
        std::fs::write(real.join("GC").join("MemoryCardA.USA.raw"), [0u8; 16]).unwrap();

        let lesser = dir.path().join("lesser");
        std::fs::create_dir_all(lesser.join("GameSettings")).unwrap();
        std::fs::write(lesser.join("GameSettings").join("GN7E69.ini"), "x").unwrap();
        std::fs::create_dir_all(lesser.join("StateSaves")).unwrap();
        std::fs::write(lesser.join("StateSaves").join("a.sav"), "x").unwrap();

        assert!(
            user_dir_score(&real) > user_dir_score(&lesser),
            "a memory card must win: {} vs {}",
            user_dir_score(&real),
            user_dir_score(&lesser)
        );
    }

    #[test]
    fn an_absent_directory_scores_zero() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(user_dir_score(&dir.path().join("nope")), 0);
    }

    /// An empty Load directory is not a texture pack.
    #[test]
    fn empty_subdirectories_are_not_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let u = dir.path().join("u");
        for d in ["Load", "GameSettings", "StateSaves", "GC"] {
            std::fs::create_dir_all(u.join(d)).unwrap();
        }
        assert_eq!(user_dir_score(&u), 0);
    }

    /// A chosen directory is never second-guessed, even if it scores nothing.
    #[test]
    fn a_chosen_directory_is_used_as_given() {
        let p = Platform::detect();
        let dir = tempfile::tempdir().unwrap();
        let chosen = dir.path().join("whatever");
        let r = resolve_user_dir(&p, Some(&chosen)).unwrap();
        assert_eq!(r, UserDir::Chosen(chosen.clone()));
        assert_eq!(r.path(), chosen.as_path());
    }

    /// Falling back to an empty directory is a legitimate state and an
    /// indistinguishable one from the bug, so it has to announce itself.
    #[test]
    fn a_fresh_directory_says_what_will_be_missing() {
        let note = UserDir::Fresh(PathBuf::from("x")).explain();
        assert!(note.contains("memory cards"), "{note}");
        assert!(note.contains("Settings"), "{note}");
        assert!(!UserDir::Fresh(PathBuf::from("x")).is_populated());
        assert!(UserDir::Existing(PathBuf::from("x")).is_populated());
    }
    #[test]
    fn a_bundled_emulator_wins_and_is_the_supported_one() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::write(runtime.join("Dolphin.exe"), b"").unwrap();

        // dolphin_in is what the resolver uses for every candidate directory,
        // so testing it directly tests the part that can be wrong. The full
        // resolve_runtime reads current_exe, which a test cannot move.
        let found = dolphin_in(&runtime).expect("a bundled Dolphin must be found");
        assert_eq!(found.file_name().unwrap(), "Dolphin.exe");
        assert_eq!(Runtime::Bundled(found).is_supported(), Some(true));
    }

    #[test]
    fn the_reborn_build_is_found_by_name_not_by_a_fixed_filename() {
        // The 2.1 install ships it as "NFL Street Reborn Dolphin.exe" and the
        // launcher payload as "Dolphin.exe". Hardcoding either makes the other
        // invisible, which is exactly the bug that left a player being asked for
        // a path the launcher was standing next to.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("NFL Street Reborn Dolphin.exe"), b"").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"").unwrap();

        let found = dolphin_in(dir.path()).expect("the Reborn build must be found");
        assert_eq!(
            found.file_name().unwrap(),
            "NFL Street Reborn Dolphin.exe"
        );
    }

    #[test]
    fn a_directory_with_no_emulator_finds_nothing_rather_than_guessing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"").unwrap();
        std::fs::write(dir.path().join("Games.exe"), b"").unwrap();
        assert_eq!(
            dolphin_in(dir.path()),
            None,
            "an unrelated exe must not be launched as the emulator"
        );
    }

    #[test]
    fn a_chosen_emulator_is_unknown_not_unsupported() {
        // The whole reason Chosen is its own variant. Adam: a stock build will
        // be missing the features for sure. But a path cannot say whether the
        // build is stock, and Adam's own Reborn builds live at chosen paths
        // during development, so asserting NOT a Reborn build cries wolf.
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("dolphin");
        std::fs::write(&exe, b"").unwrap();

        let platform = Platform::detect();
        let runtime = resolve_runtime(&platform, Some(&exe), None).unwrap();
        assert_eq!(runtime, Runtime::Chosen(exe.clone()));
        assert_eq!(
            runtime.is_supported(),
            None,
            "a chosen build is UNKNOWN, not unsupported: nothing here can tell a Reborn fork from a stock Dolphin by looking at a path, and calling a player's own build unsupported would cry wolf"
        );

        // And the explanation has to name what is lost, not the category.
        let said = runtime.explain();
        assert!(
            said.contains("cannot be told"),
            "the warning must not claim to know: {said}"
        );
        for expected in ["controller reporting", "match results"] {
            assert!(
                said.contains(expected),
                "the warning must say what will be missing: {said}"
            );
        }
    }

    #[test]
    fn a_failed_search_names_every_place_it_looked() {
        // The refusal this replaces said the install was incomplete and stopped,
        // which on a development build is wrong and on a real one is useless.
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let missing = dir.path().join("nowhere").join("dolphin.exe");
        let empty_legacy = dir.path().join("legacy");
        std::fs::create_dir_all(&empty_legacy).unwrap();

        let looked = resolve_runtime(&platform, Some(&missing), Some(&empty_legacy))
            .expect_err("nothing should have been found");
        assert!(
            looked.contains(&missing),
            "the path that was asked for must be listed: {looked:?}"
        );
        assert!(
            looked.contains(&empty_legacy),
            "and the install that was searched: {looked:?}"
        );

        let message = no_runtime_message(&looked);
        assert!(message.contains("nowhere"), "{message}");

        // The prose only. The path list is indented by two spaces on purpose,
        // so the usual doubled-space check has to skip those lines or it fires
        // on the formatting it was never meant to catch.
        for line in message.lines().filter(|l| !l.starts_with("  ")) {
            assert!(
                !line.contains("  "),
                "a doubled space means a lost line continuation: {line:?}"
            );
        }
    }

    #[test]
    fn a_path_pasted_with_windows_copy_as_path_still_resolves() {
        // The real failure. Explorer's Copy as path wraps the path in quotes,
        // the quotes are pasted with it, and the result matches nothing while
        // looking perfect in every message the launcher prints.
        let clean = PathBuf::from("C:\\Games\\Dolphin.exe");
        assert_eq!(user_path("C:\\Games\\Dolphin.exe"), clean, "an unquoted path is untouched");
        assert_eq!(
            user_path("\"C:\\Games\\Dolphin.exe\""),
            clean,
            "the quotes Copy as path adds must come off"
        );
        assert_eq!(
            user_path("   \"C:\\Games\\Dolphin.exe\"  \n"),
            clean,
            "with the whitespace a paste carries"
        );
        assert_eq!(
            user_path("'C:\\Games\\Dolphin.exe'"),
            clean,
            "and out of a shell transcript"
        );
    }

    #[test]
    fn an_unbalanced_quote_is_left_alone() {
        // Half stripping would turn a typo into a different path, and which
        // half to drop is a guess. Left as it is, so the refusal names what
        // was actually given.
        let given = "\"C:\\Games\\Dolphin.exe";
        assert_eq!(user_path(given), PathBuf::from(given));
    }

    #[test]
    fn only_one_pair_comes_off() {
        // A second pair is part of the name. A launcher that kept stripping
        // would refuse a legal path.
        assert_eq!(
            user_path("\"\"C:\\Games\\Dolphin.exe\"\""),
            PathBuf::from("\"C:\\Games\\Dolphin.exe\"")
        );
    }

    // -----------------------------------------------------------------------
    // The user folder follows the emulator; the image is never guessed
    // -----------------------------------------------------------------------

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn a_portable_emulator_boots_against_the_user_folder_beside_it() {
        // Adam: "the right user folder was NOT used". The chosen Dolphin had
        // portable.txt and its own User folder beside it, and the launch went
        // to the 2.1 install's folder instead.
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Dolphin.exe");
        touch(&exe);
        touch(&dir.path().join("portable.txt"));
        std::fs::create_dir_all(dir.path().join("User")).unwrap();

        let p = Platform::detect();
        let got = resolve_user_dir_for(&p, None, Some(&exe)).unwrap();
        assert_eq!(got, UserDir::Portable(dir.path().join("User")));
        assert!(got.is_populated());
        assert!(got.explain().contains("beside the emulator"), "{}", got.explain());

        // A chosen folder still wins over the portable one.
        let chosen = dir.path().join("elsewhere");
        let got = resolve_user_dir_for(&p, Some(&chosen), Some(&exe)).unwrap();
        assert_eq!(got, UserDir::Chosen(chosen));
    }

    #[test]
    fn a_user_folder_beside_the_emulator_counts_when_it_holds_a_memory_card_even_without_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Dolphin.exe");
        touch(&exe);
        touch(&dir.path().join("User").join("GC").join("MemoryCardA.USA.raw"));
        let p = Platform::detect();
        assert_eq!(
            resolve_user_dir_for(&p, None, Some(&exe)).unwrap(),
            UserDir::Portable(dir.path().join("User"))
        );
    }

    #[test]
    fn an_empty_folder_beside_a_non_portable_emulator_is_not_taken_for_its_own() {
        // The bundled runtime ships no User folder at all, and a stray empty
        // one must not capture the launch away from the player's real saves.
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Dolphin.exe");
        touch(&exe);
        std::fs::create_dir_all(dir.path().join("User")).unwrap();
        let p = Platform::detect();
        let got = resolve_user_dir_for(&p, None, Some(&exe)).unwrap();
        assert!(!matches!(got, UserDir::Portable(_)), "{got:?}");
    }

    #[test]
    fn the_image_the_emulator_records_as_its_default_wins() {
        // Both Dolphin.ini files on Adam's machine say DefaultISO = ...\NSR.iso.
        // The launcher booted ATL.iso, the alphabetically first of eleven.
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("rt").join("Dolphin.exe");
        touch(&exe);
        let user = dir.path().join("rt").join("User");
        let default = dir.path().join("somewhere").join("Reborn.iso");
        touch(&default);
        let ini = user.join("Config").join("Dolphin.ini");
        std::fs::create_dir_all(ini.parent().unwrap()).unwrap();
        std::fs::write(&ini, format!("[Core]\nDefaultISO = {}\n", default.display())).unwrap();
        let legacy = dir.path().join("legacy");
        touch(&legacy.join("Games").join("ATL.iso"));
        touch(&legacy.join("Games").join("NSR.iso"));

        let got = find_game_image(&exe, &user, Some(&legacy), None);
        assert_eq!(got.path(), Some(default.as_path()), "{got:?}");
        assert!(got.explain().contains("own settings"), "{}", got.explain());
    }

    #[test]
    fn nsr_by_name_beats_alphabetical_order_and_a_stale_recorded_default_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("rt").join("Dolphin.exe");
        touch(&exe);
        let user = dir.path().join("rt").join("User");
        let ini = user.join("Config").join("Dolphin.ini");
        std::fs::create_dir_all(ini.parent().unwrap()).unwrap();
        std::fs::write(&ini, "[Core]\nDefaultISO = C:\\gone\\missing.iso\n").unwrap();
        let legacy = dir.path().join("legacy");
        touch(&legacy.join("Games").join("ATL.iso"));
        touch(&legacy.join("Games").join("nsr.ISO"));
        touch(&legacy.join("Games").join("ZZZ.iso"));

        let got = find_game_image(&exe, &user, Some(&legacy), None);
        assert_eq!(got.path(), Some(legacy.join("Games").join("nsr.ISO").as_path()), "{got:?}");
        assert!(matches!(got, GameSearch::Found { source: GameImageSource::NamedNsr { .. }, .. }));
    }

    #[test]
    fn several_images_and_no_nsr_is_a_refusal_that_names_them() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("rt").join("Dolphin.exe");
        touch(&exe);
        let user = dir.path().join("rt").join("User");
        let legacy = dir.path().join("legacy");
        touch(&legacy.join("Games").join("ATL.iso"));
        touch(&legacy.join("Games").join("FUMBLES.iso"));
        // Not images: a backup and a sidecar must not count.
        touch(&legacy.join("Games").join("ATL.iso.bak_pre_flare8"));
        touch(&legacy.join("Games").join("ATL.png"));

        let got = find_game_image(&exe, &user, Some(&legacy), None);
        let GameSearch::Several { candidates, .. } = &got else {
            panic!("two images and no NSR must not be picked: {got:?}");
        };
        assert_eq!(candidates.len(), 2);
        let said = got.explain();
        assert!(said.contains("ATL.iso") && said.contains("FUMBLES.iso"), "{said}");
        assert!(said.contains("Settings"), "{said}");
    }

    #[test]
    fn the_only_image_anywhere_is_used_and_the_launcher_folder_is_looked_in_first() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("rt").join("Dolphin.exe");
        touch(&exe);
        let user = dir.path().join("rt").join("User");
        let launcher = dir.path().join("launcher");
        touch(&launcher.join("Games").join("Street.rvz"));

        let got = find_game_image(&exe, &user, None, Some(&launcher));
        assert_eq!(got.path(), Some(launcher.join("Games").join("Street.rvz").as_path()));
        assert!(matches!(got, GameSearch::Found { source: GameImageSource::OnlyOne { .. }, .. }));

        let none = find_game_image(&exe, &user, None, None);
        assert!(matches!(none, GameSearch::None { .. }));
        assert!(none.explain().contains("No game image was found"), "{}", none.explain());
    }
}
