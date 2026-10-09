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
use crate::platform::{DataDir, Error, ExitKind, Platform, Result};
use crate::runtime::{
    ConfigGenerator, GameBuild, GraphicsPreset, LaunchConfig, Launcher, SessionOutcome,
};
use crate::supervisor::{SessionEnd, SessionRecord, Startup, Supervisor};

/// Whether this launch is safe to write disc-level or per-game changes for:
/// true for anything except the image netplay boots.
///
/// ⛔⛔ THE ONLY RELIABLE TEST. `netplay_open` builds its `LaunchRequest` with
/// `build: GameBuild::Offline` and swaps only `game_path`, and the online
/// image carries the SAME disc id (`GN7E69`), so both `req.build` and
/// `req.build.game_id()` say "offline" for a netplay launch. Anything that
/// must not reach netplay has to compare the image path.
///
/// Getting this wrong is not cosmetic: a Dolphin `[OnFrame]` patch rewrites
/// emulated memory every frame and Dolphin does not synchronise those, so a
/// machine carrying one beside a machine that is not diverges on the first
/// frame. Netplay is lockstep; that is a certain desync, reproduced by Adam
/// on 2026-09-18.
///
/// Asked this way round on purpose. "Is it THE offline image?" would answer
/// no for a player whose game path is not the standard booted one, and they
/// would silently lose the GameCube no-swap patch offline, which is the very
/// complaint 3.0.10 shipped to fix. Excluding one known path is the narrow
/// claim; the rest of the world is ordinary offline play.
pub(crate) fn is_netplay_image(data_root: &std::path::Path, game_path: &std::path::Path) -> bool {
    let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let online = crate::online_image::online_dir(data_root).join(crate::online_image::IMAGE_FILE);
    canon(game_path) == canon(&online)
}

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
    /// The player said this is a league match. Recorded in the session before Dolphin
    /// starts; the backend forwards the game to the league site only when it is true.
    pub league_match: bool,
    /// Start the game even though player one's pad has dead essential bindings.
    /// False for Play, which then answers [`Refusal::ControllerNeedsSetup`]
    /// and lets the player choose. True for every caller with no way to ask
    /// (netplay, the desktop shortcut, the command line), which keeps
    /// exactly the behaviour it had.
    pub play_with_unset_controller: bool,
    /// What the launcher knows about this image, which decides what happens
    /// to Reborn's texture pack. Only `Session::run` reads it;
    /// `Session::prepare` composes nothing, so a prepare-only caller has
    /// nothing to claim and passes `Unknown`.
    pub textures: crate::game_identity::ImageStanding,
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
    /// Player one's pad cannot enter or move through the game. A pad Windows has
    /// no names for has to be set up once by pressing its buttons, and the
    /// launcher knows that BEFORE the game starts. It used to write it to a log
    /// nobody reads and start the game anyway. The caller offers the setup,
    /// and may launch again with `play_with_unset_controller`.
    ControllerNeedsSetup(BlockedPad),
}

/// Player one's pad, and the controls on it that do nothing yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedPad {
    pub device: String,
    pub controls: Vec<crate::controller::GcInput>,
}

/// Whether player one's pad stops this launch.
///
/// `pads` is the port assignment the launch just made, `dead` the controls
/// that cannot be pressed, per device. Only PLAYER ONE's pad decides: they are
/// the one at the press start screen, and a second player's unfinished pad
/// must never stop the first from playing. No pad at all is a keyboard player,
/// or a pad that turns up later and is picked up by the runtime; neither is
/// this launch's business to refuse.
pub fn player_one_blocked(
    pads: &[(u8, String)],
    dead: &std::collections::BTreeMap<String, Vec<crate::controller::GcInput>>,
) -> Option<BlockedPad> {
    let (_, device) = pads.iter().find(|(port, _)| *port == 1)?;
    let controls = dead.get(device)?;
    crate::controller::blocks_play(controls).then(|| BlockedPad {
        device: device.clone(),
        controls: controls.clone(),
    })
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
            Refusal::ControllerNeedsSetup(pad) => {
                // Name the essential controls that still need a binding.
                let names: Vec<&str> = pad
                    .controls
                    .iter()
                    .filter(|c| crate::controller::blocks_play(&[**c]))
                    .map(|c| c.label())
                    .collect();
                format!(
                    "Your controller needs a one-time setup before it can be used in the game: {} would do nothing yet. Open Controllers and choose Set up this controller. It takes about a minute, and you only do it once.",
                    names.join(" and ")
                )
            }
        }
    }
}

/// What happened.
// Not Clone: StatsAfterSession is not, and nothing in the workspace clones a
// Launched value. A caller that needs to keep what happened should hold the
// explained string, not the value itself.
#[derive(Debug)]
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
        /// The match results this session produced, already moved into the
        /// queue and NOT yet sent.
        ///
        /// # Why the caller does the sending
        ///
        /// Collecting needs the single-instance lock; sending does not, and
        /// sending is one request per queued game with the client's full
        /// timeout each, so a backlog against a slow or unreachable backend is
        /// minutes. `run` held the lock across both, and the caller holds a
        /// launch guard across the whole of `run`, so for those minutes,
        /// with the emulator already exited, pressing Play answered "A game
        /// session is already active", the Online page said to close the
        /// emulator first, opening the emulator from Settings was refused and
        /// the command line retry was refused. All four were false.
        ///
        /// So `run` returns here with the lock given back, the caller gives
        /// the guard back, and then calls [`SessionResults::send`].
        ///
        /// Nothing is at risk in between: the captures are already in the
        /// queue on disk, and an unsent queue is picked up by the next send.
        results: SessionResults,
        /// What the memory cards said around the game: the full gear sync
        /// before it opened them and the harvest of in-game edits after it
        /// closed. Never a failed launch; see [`crate::cards::CardsAtSession`].
        cards: crate::cards::CardsAtSession,
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
#[derive(Debug)]
pub enum StatsAfterSession {
    /// One entry per capture file found, including the ones refused, and what sending did.
    /// `sent` is None when nobody is signed in, so the entries wait.
    Ingested {
        enqueued: Vec<crate::stats::Enqueued>,
        /// Set when collecting or sending hit a problem.
        ///
        /// Carried rather than returned, and never allowed to stop the other
        /// half. Collection running before sending in one chain meant a single
        /// undeletable file stopped everything being sent, on that session and
        /// on every session after it, and the queue holds games from earlier
        /// sessions that are still perfectly deliverable.
        ///
        /// A named failure and not a string: see [`StatsProblem`].
        problem: Option<StatsProblem>,
        sent: Option<crate::stats::SubmitReport>,
    },
    /// The queue itself could not be opened, so there was nowhere to collect
    /// into and nothing to send from.
    ///
    /// It carries nothing. It used to carry `e.to_string()`, and
    /// [`crate::Error::Io`] displays as `io error at {path}: {source}`, so a
    /// full disk while writing the queue put a path out of the player's
    /// roaming profile and `os error 112` into a value the window renders.
    Failed,
}

/// Which half of the results step had a problem.
///
/// A named failure rather than a message, because the alternative was an
/// `Option<String>` filled in with `e.to_string()` at the two places it is
/// set. Every error type in this workspace formats itself for a developer --
/// paths, error numbers, library wording -- and none of that may reach a
/// player. Making the type unable to hold a string is a stronger guarantee
/// than remembering not to put one in it, and the detail is not lost: it goes
/// to `tracing::warn!` where the failure happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsProblem {
    /// A capture folder could not be read, or a file could not be moved.
    Collecting,
    /// The results could not be sent this time.
    Sending,
}

impl StatsProblem {
    /// A sentence for a player.
    pub fn message(self) -> &'static str {
        match self {
            StatsProblem::Collecting => {
                "Some results could not be collected this time, so they are being kept where \
                 they are. Nothing was lost, and the next game will pick them up."
            }
            StatsProblem::Sending => {
                "Some results could not be sent this time, so they are being kept. Nothing was \
                 lost, and they will be sent again."
            }
        }
    }
}

/// Shown when the queue itself could not be opened: there was nowhere to
/// collect into and nothing to send from. Nothing about the failure is in it,
/// because nothing about the failure is a thing a player can act on.
const RESULTS_NOT_COLLECTED: &str = "Match results could not be collected this time. Nothing was \
     lost: they are still on disk and the next game will pick them up.";

impl StatsAfterSession {
    /// One line for a player. Silent when there was nothing to do.
    pub fn explain(&self) -> Option<String> {
        use crate::stats::Enqueued;
        match self {
            StatsAfterSession::Failed => Some(RESULTS_NOT_COLLECTED.to_string()),
            StatsAfterSession::Ingested { enqueued, problem, sent } => {
                let found = enqueued.iter().filter(|e| matches!(e, Enqueued::Queued(_) | Enqueued::Quarantined(_))).count();
                let mut parts: Vec<String> = Vec::new();
                if found > 0 {
                    parts.push(format!("{found} result{} recorded", if found == 1 { "" } else { "s" }));
                }
                match sent {
                    // Only when there is nothing else to explain the silence.
                    None if found > 0 && problem.is_none() => parts.push("waiting for sign-in".to_string()),
                    None => {}
                    Some(r) => {
                        let n = r.accepted.len() + r.recorded.len();
                        if n > 0 { parts.push(format!("{n} result{} sent", if n == 1 { "" } else { "s" })); }
                        if !r.held.is_empty() { parts.push(format!("{} held for review", r.held.len())); }
                        if !r.rejected.is_empty() { parts.push(format!("{} refused by the server", r.rejected.len())); }
                        if !r.deferred.is_empty() || !r.stalled.is_empty() { parts.push(format!("{} could not be delivered yet", r.deferred.len() + r.stalled.len())); }
                        if r.waiting_for_sign_in { parts.push("waiting for sign-in".to_string()); }
                    }
                }
                // Named, without any error text: the type cannot hold one.
                match problem {
                    Some(StatsProblem::Collecting) => parts.push("some could not be collected this time and are being kept".to_string()),
                    Some(StatsProblem::Sending) => parts.push("some could not be sent this time and are being kept".to_string()),
                    None => {}
                }
                // Labelled rather than bare. "1 result recorded, waiting for
                // sign-in." starts with a numeral and reads as a fragment
                // pasted onto the account card with no idea what it is about.
                if parts.is_empty() { None } else { Some(format!("Match results: {}.", parts.join(", "))) }
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
    /// Connected pads with controls that CANNOT be pressed on them, and how
    /// many: `(device string, dead controls)`. Empty is the normal case. A pad
    /// listed here starts the game with those controls doing nothing, which is
    /// the failure that has always been silent; the caller can finally say so.
    /// See [`crate::controller::ControllerProfile::dead_controls`].
    pub pads_needing_setup: Vec<(String, usize)>,
    /// Set when PLAYER ONE's pad has dead essential bindings. See
    /// [`player_one_blocked`].
    pub player_one_blocked: Option<BlockedPad>,
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
            pads_needing_setup: Vec::new(),
            player_one_blocked: None,
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
        // The bundled runtime ships a `User` template (Config, GameSettings)
        // for seeding new user folders; that must never count as the
        // player's own folder, or a launch with no chosen folder would boot
        // inside the runtime package. Only a memory card (score 100) or an
        // explicit portable.txt makes a folder beside the emulator theirs.
        if beside.is_dir() && (portable || user_dir_score(&beside) >= 100) {
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
        let user = resolve_user_dir_for(platform, req.user_dir.as_deref(), Some(&req.runtime_exe))?;
        let user_dir = user.path().to_path_buf();

        // After the user folder is known, because the ReShade depth filter has
        // to be derived from the resolution the player is actually running at.
        // See reshade::depth_filter_for: with the filter pinned to one
        // resolution, every depth-driven effect vanishes at any other.
        let resolution = crate::reshade::internal_resolution_of(&user_dir);
        if let Some(text)=prefs.graphics_override.as_ref().filter(|o|o.label=="Previous settings").and_then(|o|o.legacy_reshade.as_deref()) {
            crate::reshade::apply_imported(&req.runtime_exe,text,resolution)?;
        }else{crate::reshade::apply(&req.runtime_exe, if prefs.graphics_override.as_ref().is_some_and(|o|o.label=="Previous settings"){GraphicsPreset::Performance}else{req.graphics}, resolution)?;}
        // A folder the setup just created starts from the runtime's baseline,
        // the Dolphin Adam tuned; one a player has run in keeps every edit.
        // Before the path law strips path keys, so a relative path in the
        // baseline goes the way any other would, and before the overlays,
        // so the launcher's required keys still win.
        let seeded = crate::runtime::seed_user_from_template(&req.runtime_exe, &user_dir)?;
        crate::dolphin_preferences::apply_path_law(&user_dir)?;

        // Undo what older builds welded into this install while the launcher
        // had only one verb. One shot per install: it prunes the retired pins
        // out of the per-game layer and puts the player's own values back where
        // there is evidence of what they were. Un-forcing a key in source does
        // nothing on its own, because a merge can add and update but never
        // remove. See crate::settings_law::reconcile_once.
        {
            let mut ids = vec!["GN7E69", req.build.game_id()];
            ids.sort();
            ids.dedup();
            match crate::settings_law::reconcile_once(platform, &user_dir, &ids) {
                Ok(done) if !done.is_empty() => {
                    tracing::info!(
                        "settings law: pruned {} per-game pin(s), restored {}, repaired {}",
                        done.pruned.len(),
                        done.restored.len(),
                        done.repaired.len()
                    );
                    let _ = crate::notices::add(platform, crate::notices::SETTINGS_ARE_YOURS);
                }
                Ok(_) => {}
                // A repair that cannot run must never stop the player launching.
                Err(error) => tracing::warn!("settings law: reconcile skipped: {error}"),
            }
        }

        let config_dir = user_dir.join("Config");
        std::fs::create_dir_all(&config_dir).map_err(|source| Error::Io {
            path: config_dir.clone(),
            source,
        })?;

        let config = LaunchConfig {
            build: req.build,
            graphics: req.graphics,
            fullscreen: crate::dolphin_preferences::fullscreen(platform,req.fullscreen)?,
            game_path: req.game_path.clone(),
            user_dir: user_dir.clone(),
            match_context: None,
        };

        let mut written = WrittenConfig {
            user_dir: user.clone(),
            files: seeded,
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
        let seeded_files = ConfigGenerator::seeded_overlays();
        for (name, mut overlay) in ConfigGenerator::overlays(&config) {
            // The player's own choices from the launcher's Settings page. These
            // are merged rather than seeded because the player asked for them
            // here, in this launcher, and expects pressing Play to apply them.
            for (section, keys) in crate::dolphin_preferences::overlay(&name,&preferences.emulator_preferences) { overlay.entry(section).or_default().extend(keys); }

            // Everything below is applied with merge_absent: a default, never
            // an override. A key already in the file keeps its value.
            let mut seeded = seeded_files.get(&name).cloned().unwrap_or_default();

            // "Previous settings" carries a migrating player's whole old
            // GFX.ini forward. It seeds a fresh folder and never overrides what
            // is already there, so a change they make afterwards is not
            // reverted on every launch for the rest of the install's life.
            if name == "GFX.ini" {
                if let Some(previous)=preferences.graphics_override.as_ref().filter(|o|o.label=="Previous settings").and_then(|o|o.legacy_gfx.as_deref()) {
                    let mut section=String::new();
                    for line in previous.lines(){
                        let line=line.trim();
                        if line.starts_with('[')&&line.ends_with(']'){section=line[1..line.len()-1].into();}
                        else if !line.starts_with([';','#']){
                            if let Some((key,value))=line.split_once('='){
                                seeded.entry(section.clone()).or_default().entry(key.trim().into()).or_insert_with(||value.trim().into());
                            }
                        }
                    }
                }
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
            // Two verbs, in this order. `merge` for what the launcher decides
            // and what the player set in the launcher; `merge_absent` for the
            // tuned defaults, which fill a gap and never overwrite a choice.
            // There is no third step restoring the player's value afterwards,
            // because nothing above took it away. See crate::settings_law.
            let mut report = doc.merge(&overlay);
            let seeded_report = doc.merge_absent(&seeded);
            report.added.extend(seeded_report.added);
            report.sections_created.extend(seeded_report.sections_created);
            // A backend this platform's Dolphin cannot run, carried in by a
            // template built on Windows. Always empty on Windows.
            if name == "Dolphin.ini" {
                let repair = crate::dolphin_preferences::unusable_here(&doc);
                report.updated.extend(doc.merge(&repair).updated);
            }

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

        // Pre-boot config check. A 0-FPS freeze on 2026-09-11 was caused by the
        // HD texture prefetch being on (12 GB RAM, the "running slow" report),
        // and by a run being dual core. Log the three settings that decide both
        // so a bad state is visible in the launcher log the moment it happens,
        // not weeks later from a probe. `CacheHiresTextures` back on is a
        // warning; the core mode is informational (single core is the user's
        // per-machine choice and is not forced here).
        let prefetch = doc.get("Video_Settings", "CacheHiresTextures").unwrap_or("<unset>");
        let cpu_thread = doc.get("Core", "CPUThread").unwrap_or("<default:True>");
        let sync_gpu = doc.get("Core", "SyncGPU").unwrap_or("<unset>");
        if prefetch != "False" {
            tracing::warn!(
                "{game_id}: CacheHiresTextures = {prefetch} (expected False); the HD pack will prefetch to ~12 GB of RAM and the game will run slow"
            );
        }
        tracing::info!(
            "{game_id} pre-boot: CacheHiresTextures={prefetch} CPUThread={cpu_thread} SyncGPU={sync_gpu}"
        );

        }

        // Is this launch the offline image, or netplay?
        //
        // ⛔⛔ `req.build` CANNOT ANSWER THAT. `netplay_open` builds its
        // LaunchRequest with `build: GameBuild::Offline` and only swaps
        // `game_path` to the online image, and that image carries the SAME
        // disc id (`GN7E69`) as the offline one, so `req.build.game_id()` is
        // "GN7E69" for netplay too. Every "offline only" rule written against
        // the build or the id therefore includes netplay by accident. The
        // image path is the only thing that tells them apart.
        // Default to "this is ordinary offline play". Only a launch we can
        // positively identify as netplay's image is excluded.
        let on_booted_offline = !platform
            .paths
            .data_root()
            .ok()
            .is_some_and(|root| is_netplay_image(&root, &req.game_path));

        // The GameCube profile plays the game the GameCube way: the disc's
        // in-match X/B swap is switched off for that player through Dolphin's
        // per-game patch list, and switched back on for every other profile.
        //
        // ⛔⛔ NETPLAY MUST NEVER CARRY IT. Dolphin does not synchronise
        // [OnFrame] patches, so one machine running the cave beside one that
        // is not diverges on the first B or X press: netplay is lockstep, and
        // that is a guaranteed desync, not a risk. Before 2026-09-18 this was
        // guarded by the game id, which netplay shares, so the patch went in
        // anyway. The state is rewritten per launch, so an offline launch puts
        // it back.
        {
            let wanted = on_booted_offline
                && crate::gamecube_controls::wanted_for(
                    written.controls.as_ref().map(|c| c.profile),
                );
            let mut ids = vec!["GN7E69", req.build.game_id()];
            ids.sort();
            ids.dedup();
            let applied = crate::gamecube_controls::apply(&user_dir, &ids, wanted)?;
            for path in applied.files_written {
                tracing::info!(
                    "gamecube controls: {} {}",
                    if applied.carried { "no X/B swap written to" } else { "disc behaviour restored in" },
                    path.display()
                );
                written.keys_set += 1;
                written.files.push(path);
            }
        }

        // The same profile, the same rule, one layer further in: the disc's
        // in-match Gamebreaker prompt draws X because game B is physical X
        // for a swapped pad. A GameCube player is not swapped, so their
        // prompt has to say B to match the button they actually press. The
        // glyph lives in a disc file, not in a code word, so the offline
        // image carries its own copy of that file. Offline only: netplay
        // boots a different, non-customizable image where they ARE swapped
        // and the shipped X is already right. See crate::gamecube_glyph.
        //
        // ⛔ Gated on the image path, for the reason spelled out above: a
        // build check would rewrite the very disc netplay must keep identical
        // between players.
        // ⛔ And only for a build the player may customize: the league image is shared byte for byte between members
        // in netplay (GameBuild::League is not customizable), so it is never rewritten, offline either.
        if on_booted_offline && req.build.is_customizable() {
            let wanted =
                crate::gamecube_controls::wanted_for(written.controls.as_ref().map(|c| c.profile));
            match crate::gamecube_glyph::apply(&req.game_path, wanted) {
                Ok(crate::gamecube_glyph::Outcome::WroteVariant) => {
                    tracing::info!("gamecube glyph: Gamebreaker prompt set to B in {}", req.game_path.display());
                }
                Ok(crate::gamecube_glyph::Outcome::WroteShipped) => {
                    tracing::info!("gamecube glyph: Gamebreaker prompt restored to X in {}", req.game_path.display());
                }
                Ok(crate::gamecube_glyph::Outcome::AlreadyRight) => {}
                Ok(crate::gamecube_glyph::Outcome::Unrecognised) => {
                    tracing::warn!(
                        "gamecube glyph: {} in the booted image matches neither copy this launcher carries, so the Gamebreaker prompt was left alone; a game update has changed the file and the launcher needs a new copy",
                        crate::gamecube_glyph::FILE_NAME
                    );
                }
                Ok(crate::gamecube_glyph::Outcome::NotFound) => {
                    tracing::warn!(
                        "gamecube glyph: no {} of the expected length in {}",
                        crate::gamecube_glyph::FILE_NAME,
                        req.game_path.display()
                    );
                }
                Err(e) => tracing::warn!("gamecube glyph: {e}"),
            }
        }
        // Ask the runtime that is about to be launched. The built in scan sees
        // XInput only, which on a real machine misses an entire backend.
        let scratch = platform.paths.data_root()?.join(DataDir::Cache.relative());
        // Prefer the dump, which answers what the pads are called AND what
        // their inputs are called, for the cost of the same single process.
        // Fall back in order, because a pad list with no input list is still
        // far better than no pad list.
        let (scan, authoritative, device_inputs) =
            match crate::devices::dump_via_runtime(platform, &req.runtime_exe, &scratch) {
                Ok((scan, inputs)) => (scan, true, inputs),
                Err(_) => match crate::devices::scan_via_runtime(platform, &req.runtime_exe, &scratch) {
                    Ok(scan) => (scan, true, Vec::new()),
                    Err(_) => (DeviceScanner::scan(), false, Vec::new()),
                },
            };
        written.pads_authoritative = authoritative;

        crate::controller_profiles::adopt_migrated(platform, &scan)?;
        let mut saved_profiles = crate::controller_profiles::load(&crate::controller_profiles::path(platform)?)?;

        // Bind every pad by the names IT reports.
        //
        // This is the fix for a PlayStation 5 pad being unusable. The shipping
        // default is XInput vocabulary (`Button A`, `Shoulder R`, `Start`,
        // `Pad N`). A PS5 pad on Windows never reaches XInput; it arrives on
        // WGInput reporting `Cross`, `Bumper R`, `Menu`, `Switch 0 N`. Every
        // button and the whole d-pad therefore resolved to nothing, in silence,
        // which is why the workaround going around was DS4Windows: it
        // manufactures a virtual Xbox pad, and an Xbox pad was the only thing
        // this could bind.
        //
        // Seeding a profile for every connected pad also means the builders
        // below never fall back to their own `ControllerProfile::new`, which
        // still carries the XInput default.
        let mut profiles_changed = false;
        {
            for pad in scan.playable_pads() {
                // Ask the runtime first. Fall back to a recorded device only
                // when it could not be asked, which is any runtime older than
                // --dump-device-inputs. See `devices::known_inputs`.
                let available = match device_inputs
                    .iter()
                    .find(|d| d.device == pad.device_string())
                {
                    Some(reported) => reported.all_names(),
                    // The runtime is too old to list controls. Do NOT try to
                    // guess the pad from its name: Windows names a DualSense
                    // from its HID product string, which differs between
                    // machines, and a pad whose name we failed to recognise
                    // got dead bindings, which is the original bug again.
                    //
                    // A profile seeded here already carries the universal
                    // mapping, which binds every vocabulary at once and needs
                    // no identification at all. Leave it alone.
                    None => {
                        tracing::info!(
                            "{}: the runtime cannot list controls, so the pad keeps the universal mapping",
                            pad.device_string()
                        );
                        Vec::new()
                    }
                };
                let key = pad.identity().profile_key();
                match saved_profiles
                    .iter_mut()
                    .find(|p| p.identity.profile_key() == key)
                {
                    Some(existing) => {
                        let before = existing.clone();
                        // Widen first, repair second. A saved profile can
                        // still carry the XInput-only default: written by a
                        // build before 3.0.6, or adopted out of the old pad
                        // file by the first 3.0.6 launch as twenty
                        // "overrides". On a PlayStation pad that is dead, and
                        // when the runtime cannot list the pad's controls
                        // `repair_for` has no evidence to act on, so it never
                        // reached those players. The universal binding needs
                        // no evidence; the repair narrows it back to exact
                        // names whenever it can.
                        let widened = existing.widen_stale_defaults();
                        if !widened.is_empty() {
                            tracing::info!(
                                "{}: {} binding(s) were the launcher's own older vocabulary and were widened to bind every pad: {:?}",
                                pad.device_string(),
                                widened.len(),
                                widened
                            );
                        }
                        let repaired = existing.repair_for(&available);
                        if !repaired.is_empty() {
                            tracing::info!(
                                "{}: repaired {} binding(s) that named controls this pad does not have: {:?}",
                                pad.device_string(),
                                repaired.len(),
                                repaired
                            );
                        }
                        if *existing != before {
                            profiles_changed = true;
                        }
                    }
                    None => {
                        let seeded = crate::controller::ControllerProfile::from_device(
                            pad.identity(),
                            1,
                            &available,
                        );
                        let dead = seeded.mapping.unresolvable(&available);
                        if dead.is_empty() {
                            tracing::info!(
                                "{}: mapped from the pad's own input names",
                                pad.device_string()
                            );
                        } else {
                            // Numeric-only pads (DirectInput, SDL) report
                            // `Button 7`, which means nothing without being
                            // measured. Never guess: the remap wizard asks.
                            tracing::warn!(
                                "{}: {} control(s) could not be mapped from names alone and need the remap wizard: {:?}",
                                pad.device_string(),
                                dead.len(),
                                dead
                            );
                        }
                        saved_profiles.push(seeded);
                    }
                }
            }
        }
        // Adopt whatever the player changed in Dolphin's own Controller
        // Settings before regenerating. The launcher keeps owning the Device
        // line and the port layout, because a remembered device string that no
        // longer matches binds nothing and Dolphin says nothing about it. It
        // stops owning the bindings, the tuning and the calibration.
        //
        // The sidecar holds exactly the bytes the launcher wrote last time, so
        // "what did somebody else change" is answerable without guessing.
        let pad_path = config_dir.join("GCPadNew.ini");
        let sidecar = pad_path.with_extension("ini.launcher-wrote");
        let mut saved_profiles = saved_profiles;
        if let Ok(live) = std::fs::read_to_string(&pad_path) {
            let ours = std::fs::read_to_string(&sidecar).unwrap_or_default();
            // Which device each saved pad is THIS session. A section of the
            // file is only ever adopted into the pad its Device line names.
            let connected: std::collections::BTreeMap<String, String> = scan
                .playable_pads()
                .iter()
                .map(|pad| (pad.identity().profile_key(), pad.device_string()))
                .collect();
            let adopted = crate::controller::adopt_external_edits(&live, &ours, &mut saved_profiles, |identity| {
                connected.get(&identity.profile_key()).cloned()
            });
            if !adopted.is_empty() {
                tracing::info!("controller: adopted {} change(s) made in Dolphin: {:?}", adopted.len(), adopted);
                profiles_changed = true;
            }
        }
        // Assess the final bindings, including edits just adopted from Dolphin.
        // Say which pads will start the game with controls that do nothing.
        //
        // The warning above never fired for the pads that need it: it asks
        // `Mapping::unresolvable`, which skips every binding containing `|`,
        // and since 3.0.6 every default binding is an OR. A pad Windows has no
        // gamepad mapping for (a USB GameCube-style pad, a generic HID pad)
        // reports `Button 3` and `Axis 1-`, nothing in the default binding
        // exists on it, and this log said nothing. At WARN, because that is
        // the level a player's launcher.log keeps.
        let mut dead_by_device: std::collections::BTreeMap<String, Vec<crate::controller::GcInput>> = Default::default();
        for pad in scan.playable_pads() {
            let Some(reported) = device_inputs.iter().find(|d| d.device == pad.device_string()) else { continue };
            let key = pad.identity().profile_key();
            let Some(profile) = saved_profiles.iter().find(|p| p.identity.profile_key() == key) else { continue };
            let dead = profile.dead_controls(&reported.all_names());
            if !dead.is_empty() {
                tracing::warn!(
                    "controller: {} has {} control(s) that cannot be pressed on it and will do nothing in the game until it is set up under Controllers: {:?}",
                    pad.device_string(),
                    dead.len(),
                    dead
                );
                written.pads_needing_setup.push((pad.device_string(), dead.len()));
                dead_by_device.insert(pad.device_string(), dead);
            }
        }
        // Persist what changed: an adoption, a widening, or a repair. A
        // widened or repaired profile left unsaved would show its dead
        // bindings on the Controllers page as the player's own choices.
        if profiles_changed {
            if let Ok(file) = crate::controller_profiles::path(platform) {
                for profile in &saved_profiles {
                    let _ = crate::controller_profiles::save(&file, profile.clone());
                }
            }
        }
        let saved_profiles = saved_profiles;
        let preferences = crate::settings::Settings::load(&crate::settings::settings_path(platform)?)?;
        match build_gcpad_for_slots(&scan, &saved_profiles, preferences.primary_controller.as_deref(), preferences.controller_slots.as_ref()) {
            Some((ini, assignment)) => {
                let path = pad_path;
                // Recorded once before the launcher's first overwrite, for
                // evidence. Never restored from automatically.
                if let Ok(prev) = std::fs::read_to_string(&path) {
                    let original = path.with_extension("ini.launcher-original");
                    if !original.exists() && !prev.is_empty() {
                        let _ = std::fs::write(&original, &prev);
                    }
                }
                std::fs::write(&path, &ini).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
                // What we just wrote, so the next launch can tell our lines
                // from theirs.
                let _ = std::fs::write(&sidecar, &ini);
                written.files.push(path);
                written.pads = assignment;
                written.no_controller = written.pads.is_empty();
                // Decided here, once the ports are known: only player one's pad counts.
                written.player_one_blocked = player_one_blocked(&written.pads, &dead_by_device);
            }
            None => {
                written.no_controller = true;
                // No pad was SEEN, which is not the same as no pad. The scan
                // can miss one, and a Bluetooth pad is often switched on after
                // Play. The runtime binds player one to whichever pad turns
                // up, so what is on disk has to be pressable by it. Never a
                // reason not to start the game.
                match leave_standing_pad_file(&pad_path) {
                    Ok(true) => {
                        tracing::warn!(
                            "controller: no pad was seen at launch, so {} now binds every pad's names and the game takes whichever pad is connected",
                            pad_path.display()
                        );
                        written.files.push(pad_path.clone());
                    }
                    Ok(false) => {}
                    Err(e) => tracing::warn!("controller: {} could not be prepared for a pad that turns up later: {e}", pad_path.display()),
                }
            }
        }

        // Dolphin.ini: the controller ports for the pads bound above, and a
        // memory card when none is configured. Written only when something
        // changes, and whether or not a pad was found: a keyboard player
        // still needs somewhere to save.
        let dolphin_file=config_dir.join("Dolphin.ini");
        let dolphin_text = match std::fs::read_to_string(&dolphin_file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        let mut dolphin_doc=crate::ini::IniDocument::parse(&dolphin_text);
        let user_dir = config_dir.parent().map(Path::to_path_buf).unwrap_or_else(|| config_dir.clone());
        let card = memory_card_overlay(&dolphin_doc, &user_dir);
        let mut changed = false;
        let ports = if written.no_controller {
            // A pad that turns up after the scan reaches the game only if
            // player one's port is on. Same rule as always: a port is switched
            // on when it is off or unset, and a value somebody chose (a
            // GameCube adapter, say) is left exactly as it is.
            controller_port_overlay(&dolphin_doc, &player_one_port())
        } else {
            controller_port_overlay(&dolphin_doc, &written.pads)
        };
        changed |= !dolphin_doc.merge(&ports).is_noop();
        changed |= !dolphin_doc.merge(&card).is_noop();
        if changed {
            std::fs::write(&dolphin_file,dolphin_doc.render())?;
            if !written.files.contains(&dolphin_file) { written.files.push(dolphin_file); }
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

        // The lock, before anything is written or started, and waited for
        // rather than refused outright. The launcher takes this same lock for
        // its own background work; telling a player to close a launcher that
        // does not exist, and not starting their game, is the worse answer.
        // See [`Supervisor::acquire_waiting`].
        let Startup::Owner(supervisor) = Supervisor::acquire_waiting(platform)? else {
            return Ok(Launched::Refused(Refusal::AlreadyRunning));
        };
        let _ = supervisor.recover_unfinished()?;

        let written = Self::prepare(platform, req)?;

        // Never start the game on a pad that cannot press Start.
        //
        // Before the session record opens and before the emulator starts, so a
        // refusal leaves nothing behind; the lock is given back by returning.
        // What prepare wrote stays written, which is what the setup needs.
        if !req.play_with_unset_controller {
            if let Some(blocked) = written.player_one_blocked.clone() {
                tracing::warn!(
                    "controller: not starting the game, because {} cannot press {:?}; the player is offered the setup",
                    blocked.device,
                    blocked.controls
                );
                return Ok(Launched::Refused(Refusal::ControllerNeedsSetup(blocked)));
            }
        }

        // The mods, made real. prepare reconciled the activation list; this
        // builds the tree that list describes and points the emulator at it.
        // Before this call existed every profile switch wrote the list
        // correctly and the game never saw it.
        // The league disc is never judged by the retail verdict: see `standing_for_build`.
        let data_root = platform.paths.data_root().ok();
        let textures = standing_for_build(req.build, &req.game_path, data_root.as_deref(), req.textures);
        let mods = sync_mods_for_launch(platform, &supervisor, req.build, written.user_dir.path(), req.mods_disabled, textures);

        // The memory cards, brought up to date with the installed roster
        // before the game opens them, under the lock and with the game not
        // yet started. Free when nothing changed since the last full run.
        // Never a reason not to start the game: a failure is a sentence.
        let cards_before = crate::cards::before_session(platform, written.user_dir.path());

        let config = LaunchConfig {
            build: req.build,
            graphics: req.graphics,
            fullscreen: crate::dolphin_preferences::fullscreen(platform,req.fullscreen)?,
            game_path: req.game_path.clone(),
            // The same directory prepare just merged into, never a fresh one.
            user_dir: written.user_dir.path().to_path_buf(),
            match_context: None,
        };

        let record = supervisor.begin_with_intent(
            req.build,
            Some(crate::supervisor::SessionIntent { league_match: req.league_match, match_id: None }),
        )?;
        let handle = Launcher::launch(platform, &req.runtime_exe, &config)?;
        let exit = platform.processes.wait(handle)?;
        let finished = supervisor.finish(record, &exit)?;

        // The match results, collected while the lock is still held and the
        // session id is still in hand. Collecting is the half that needs the
        // lock: the queue and the session log are shared state, and a second
        // launcher moving captures out of the same folder is the case the lock
        // exists for.
        let results = collect(platform, &supervisor, &finished.id, written.user_dir.path());

        // The member's in-game gear edits, harvested off the memory cards
        // while the lock is still held and the game is known to be closed.
        // Nothing is written to a card here; the next full sync does that.
        let cards_after = crate::cards::after_session(platform, written.user_dir.path());

        // The lock, back, before this returns and before anything reaches the
        // network. The caller still holds a launch guard across this whole
        // call, so it gives that back too and then calls
        // `SessionResults::send`. Held across the send, the two of them made
        // four screens say something that was not true for as long as a
        // backlog took to deliver. Nothing is dropped by returning early: the
        // captures are already in the queue on disk.
        drop(supervisor);

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
            results,
            cards: crate::cards::CardsAtSession { before: cards_before, after: Some(cards_after) },
        })
    }
}

/// The results of a finished session, collected and not yet sent.
///
/// # Why collecting and sending are two calls
///
/// Both have to happen and only one of them needs the lock. Collecting reads
/// the emulator's folders into the queue and reads the session log, which are
/// shared state and are exactly what the single-instance lock is for. Sending
/// is one network round trip per queued game, each with the client's full
/// timeout, so a backlog against a slow or unreachable backend is minutes.
///
/// Holding the lock and the launch guard across the send made four things the
/// product says untrue for those minutes, with the emulator already exited:
/// pressing Play answered "A game session is already active", the Online page
/// answered "close the emulator first", opening the emulator from Settings was
/// refused, and the command line retry was refused. So a caller holding them
/// gives them back between these two calls.
///
/// Every path does this, the launch included. [`Session::run`] returns this
/// value with the lock already given back, and its caller gives the launch
/// guard back before calling [`send`](Self::send). There is deliberately no
/// helper that does both in one call: one existed, the launch used it, and
/// that is how the primary path kept doing this after the two secondary paths
/// were fixed.
///
/// Nothing is at risk between the two calls. The captures are in the queue on
/// disk by the end of [`collect`], and an unsent queue is picked up whole by
/// the next send.
#[derive(Debug)]
pub struct SessionResults {
    /// `None` when the queue would not open, which is the one failure that
    /// really ends it: nowhere to collect into and nothing to send from.
    queue: Option<crate::stats::StatsQueue>,
    /// Read while the lock was still held, because it is the supervisor's own
    /// log, and carried so the send needs nothing from the supervisor.
    history: Vec<SessionRecord>,
    enqueued: Vec<crate::stats::Enqueued>,
    problem: Option<StatsProblem>,
}

impl SessionResults {
    /// Explain local collection before a background upload has answered.
    /// Do not claim delivery or a sign-in failure before contacting the service.
    pub fn background_note(&self) -> Option<String> {
        if self.queue.is_none() { return Some(RESULTS_NOT_COLLECTED.to_string()); }
        if let Some(problem) = self.problem { return Some(problem.message().to_string()); }
        let recorded = self.enqueued.iter().any(|entry| matches!(entry,
            crate::stats::Enqueued::Queued(_) | crate::stats::Enqueued::Quarantined(_)));
        recorded.then(|| "Match results are saved. View their sync status on Stats.".to_string())
    }

    /// Send what was collected, and everything else already queued.
    ///
    /// Needs no lock and no guard. Never returns an error: by the time this
    /// runs the game has been played.
    pub fn send(self, platform: &Platform) -> StatsAfterSession {
        let SessionResults {
            queue,
            history,
            enqueued,
            mut problem,
        } = self;
        let Some(queue) = queue else {
            return StatsAfterSession::Failed;
        };

        let sent = match crate::backend::auto_submit(platform, &queue, &history) {
            Ok(crate::backend::SendAttempt::Sent(report)) => Some(report),
            Ok(crate::backend::SendAttempt::NotSignedIn) => None,
            Err(e) => {
                tracing::warn!("results could not be sent: {e}");
                problem.get_or_insert(StatsProblem::Sending);
                None
            }
        };

        StatsAfterSession::Ingested {
            enqueued,
            problem,
            sent,
        }
    }
}

/// Collect the match results a session produced, without sending them.
///
/// Call it with the lock still held: the queue and the session log are shared
/// state, and a second launcher collecting the same folder at the same time is
/// the case the lock exists for. Then give the lock back and call
/// [`SessionResults::send`].
pub fn collect(
    platform: &Platform,
    supervisor: &Supervisor,
    session_id: &str,
    user_dir: &Path,
) -> SessionResults {
    let queue = match crate::stats::StatsQueue::open(platform) {
        Ok(queue) => queue,
        // The detail goes to the log; the value a caller shows carries none of
        // it.
        Err(e) => {
            tracing::warn!("the results queue could not be opened: {e}");
            return SessionResults {
                queue: None,
                history: Vec::new(),
                enqueued: Vec::new(),
                problem: None,
            };
        }
    };

    // Collecting and sending are two jobs, not one chain. Collecting used to
    // come first with a `?` on it, so a folder it could not finish stopped the
    // send as well, and the queue holds games from earlier sessions that were
    // still perfectly deliverable.
    let (enqueued, mut problem) = match queue.ingest_all(user_dir, Some(session_id)) {
        Ok(found) => (found, None),
        Err(e) => {
            tracing::warn!("results could not be collected from {}: {e}", user_dir.display());
            (Vec::new(), Some(StatsProblem::Collecting))
        }
    };

    // The last thing that needs the lock.
    let history = match supervisor.history() {
        Ok(history) => history,
        Err(e) => {
            tracing::warn!("the session log could not be read: {e}");
            problem.get_or_insert(StatsProblem::Sending);
            Vec::new()
        }
    };

    SessionResults {
        queue: Some(queue),
        history,
        enqueued,
        problem,
    }
}

/// Close a session whose emulator has stopped, then [`collect`].
///
/// The send is deliberately not part of this. The caller spawned the emulator
/// itself and is holding the launch guard and the single-instance lock, and a
/// send is minutes against a slow backend, so it gives both back and then
/// calls [`SessionResults::send`].
///
/// For a caller that spawned the emulator itself and waited for it on its own
/// thread, rather than going through [`Session::run`]. The session id comes
/// from the record that was written *before* the emulator started, which is
/// the whole point: where a result goes is decided at launch and written down,
/// never worked out from the file afterwards.
///
/// `exit` is `None` when the wait itself failed. The record is then left open
/// on purpose, so the next launch recovers it as
/// [`SessionEnd::Interrupted`](crate::supervisor::SessionEnd::Interrupted),
/// which is the truth. Guessing a clean exit here would put a fact on the
/// envelope that nobody observed.
///
/// The collection runs either way. The game was played whether or not the
/// launcher managed to write down how it ended.
pub fn finish_and_collect(
    platform: &Platform,
    supervisor: &Supervisor,
    record: SessionRecord,
    exit: Option<&ExitKind>,
    user_dir: &Path,
) -> SessionResults {
    let session_id = record.id.clone();
    if let Some(exit) = exit {
        // Reported through the session log on the next launch if it fails.
        // Nothing here may stop the collection below.
        let _ = supervisor.finish(record, exit);
    }
    collect(platform, supervisor, &session_id, user_dir)
}

/// What to do with the emulator's texture folder.
///
/// Public because a launch is not the only thing that composes into it: the
/// Mods page, the pack toggle and the controls-profile apply all write the
/// same folder a launch boots against, and a second copy of this rule for
/// them is exactly how the two would drift apart.
///
/// Three answers, not two, and the order of the two refusals is the whole
/// point of the function:
///
/// 1. A player who turned the launcher's management off owns that folder.
///    That wins outright, over everything below it. Reborn composes nothing
///    and removes nothing, which is what a texture author needs and what the
///    setting has always promised.
/// 2. Otherwise, an image Reborn did not verify as its own gets
///    [`TexturePolicy::Withhold`]: the composed tree is taken away rather
///    than merely not rebuilt. Not rebuilding is what a boolean could
///    express, and it is not enough, because the pack is already sitting in
///    the user folder for anybody who has ever booted successfully and
///    Dolphin loads it by game ID for whatever `GN7E69` disc turns up. That
///    is exactly the "old names with new faces" report this law comes from.
/// 3. Otherwise, compose normally.
pub fn texture_policy_for_launch(
    standing: crate::game_identity::ImageStanding,
    player_wants_managed: bool,
) -> crate::mods::TexturePolicy {
    use crate::game_identity::ImageStanding;
    use crate::mods::TexturePolicy;
    if !player_wants_managed {
        return TexturePolicy::LeaveAlone;
    }
    match standing {
        ImageStanding::Reborn => TexturePolicy::Manage,
        ImageStanding::NotReborn => TexturePolicy::Withhold,
        // Withholding DELETES the pack, so it is reserved for an image the
        // launcher actually read and actually ruled out. Not knowing is not a
        // ruling: a missing DolphinTool or a disc held open for a moment must
        // not cost the player the texture pack they already have. Leaving it
        // alone keeps whatever is composed exactly as it is.
        ImageStanding::Unknown => TexturePolicy::LeaveAlone,
    }
}

/// The standing the texture sync acts on for this build.
///
/// Every build but the league passes the caller's standing straight through: the retail law
/// ([`texture_policy_for_launch`], `game_identity::may_play`) is unchanged.
///
/// ⛔ The league disc is not a retail disc. The retail verdict, asked about it, answers "not Reborn", and NotReborn
/// withholds, which DELETES the composed tree. So a league launch never takes the caller's standing: the installed
/// league image (`library/league/game/league.iso`, with a `league.json` that names [`LEAGUE_GAME_ID`]) is the
/// league's own and composes, and anything else is left alone. It is never judged NotReborn, so a league launch
/// never selects Withhold. The sync itself is keyed by the build's game ID, so only `Load/Textures/NSRS04` is ever
/// in play, never `GN7E69`.
///
/// [`LEAGUE_GAME_ID`]: crate::league_edition::LEAGUE_GAME_ID
pub fn standing_for_build(
    build: GameBuild,
    game_path: &Path,
    data_root: Option<&Path>,
    caller: crate::game_identity::ImageStanding,
) -> crate::game_identity::ImageStanding {
    use crate::game_identity::ImageStanding;
    if build != GameBuild::League {
        return caller;
    }
    let installed = data_root.is_some_and(|root| {
        let expected = crate::league_edition::league_image_path(root);
        let same = game_path == expected
            || matches!(
                (std::fs::canonicalize(game_path), std::fs::canonicalize(&expected)),
                (Ok(a), Ok(b)) if a == b
            );
        same && crate::league_edition::read_league_info(root).is_ok()
    });
    if installed {
        ImageStanding::Reborn
    } else {
        ImageStanding::Unknown
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
    textures: crate::game_identity::ImageStanding,
) -> ModsAtLaunch {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let attempt = (|| -> Result<crate::mods::SyncOutcome> {
        let game = crate::mods::GameId::parse(build.game_id())?;
        let mut req = crate::mods::SyncRequest::in_data_root(platform, game, user_dir)?;
        let settings_file = crate::settings::settings_path(platform)?;
        req.texture_policy =
            texture_policy_for_launch(textures, crate::settings::manage_texture_folder(&settings_file)?);
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

/// The port assignment of a launch that saw no pad: player one, and no claim
/// about which device, because none was seen.
pub fn player_one_port() -> Vec<(u8, String)> {
    vec![(1, String::new())]
}

/// What a launch that saw NO pad leaves in `GCPadNew.ini`. True when the file
/// was written.
///
/// "Saw no pad" is a statement about the scan, not about the desk. See
/// [`crate::controller::standing_pad_file`] for the measurements and for what
/// is never touched. Kept apart from [`Session::prepare`] so the bytes that
/// land on disk can be checked without a runtime to scan with.
///
/// The sidecar is deliberately left alone. It records what the launcher wrote
/// for a pad it could see, and everything widened here is the launcher's own
/// vocabulary, which adoption already refuses to take for a player's choice.
pub fn leave_standing_pad_file(pad_path: &Path) -> std::io::Result<bool> {
    let live = match std::fs::read_to_string(pad_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let Some(standing) = crate::controller::standing_pad_file(&live) else {
        return Ok(false);
    };
    if let Some(parent) = pad_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(pad_path, standing)?;
    Ok(true)
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
    // No pad bound at all means there is nothing to say, and saying nothing
    // means writing nothing. An empty GCPadNew.ini is indistinguishable from a
    // broken pad, which is the invariant this crate already states for the
    // other builder; it was never enforced on the path production uses.
    if assignment.is_empty() {
        return None;
    }
    let ini=GcPadConfig::generate(&profiles,|id|pads.iter().find(|p|p.identity().profile_key()==id.profile_key()).map(|p|p.device_string()).unwrap_or_default());
    // A port with no pad this session gets NO section at all.
    //
    // This used to write `[GCPadN]\nDevice = ` for every unfilled port, which
    // meant one launch with a pad asleep or reconnecting over Bluetooth wiped
    // that player's bindings for good. An omitted section leaves whatever
    // Dolphin already has; an empty Device line destroys it.
    Some((ini,assignment))
}

/// The memory card the game saves to. A fresh user folder seeded from the
/// runtime template used to name no card at all (SlotA = 255), so the game
/// could never save and the card gear sync had no card to bring up to date
/// (Adam, 2026-09-12: "it didn't change the memory card"). Slot A becomes a
/// raw memory card with NO path: Dolphin then uses its default,
/// `<user>/GC/MemoryCardA.<region>.raw`, which is the path law
/// (`dolphin_preferences::apply_path_law`). A card path the file names is the
/// law's business: one inside a Dolphin user folder is removed for the
/// default, one elsewhere is a player's own choice and stays. A working slot
/// (a raw card, a folder of GCIs) is left as it is.
pub fn memory_card_overlay(doc: &crate::ini::IniDocument, _user_dir: &Path) -> crate::ini::Overlay {
    let mut overlay = crate::ini::Overlay::new();
    let slot = doc.get("Core", "SlotA").map(str::trim).unwrap_or("");
    if !(slot.is_empty() || slot == "255" || slot == "0") {
        return overlay;
    }
    overlay.entry("Core".into()).or_default().insert("SlotA".into(), "1".into());
    overlay
}

/// Turn ON the ports the launcher just bound a pad to. Never turn one off.
///
/// This used to write `SIDevice<n> = 0` for every port the launcher had not
/// filled, on every launch. A pad asleep, reconnecting over Bluetooth, or
/// simply unplugged for one session was enough to switch its port off, and it
/// also destroyed a port a player had deliberately set to something the
/// launcher has no concept of: a GameCube adapter (12), a GC keyboard (7).
///
/// Writing a port on is recoverable and visible. Writing one off is neither,
/// so the launcher does not do it: `doc` is consulted and a port that already
/// holds a value the launcher did not put there is left exactly as it is.
pub fn controller_port_overlay(doc:&crate::ini::IniDocument,assignment:&[(u8,String)])->crate::ini::Overlay {
    let mut overlay=crate::ini::Overlay::new();
    for index in 0..4u8 {
        if !assignment.iter().any(|(port,_)|*port==index+1) {
            continue;
        }
        let key=format!("SIDevice{index}");
        // Already a standard controller, or already something the player chose.
        // Either way there is nothing to say.
        match doc.get("Core",&key).map(str::trim) {
            None | Some("") | Some("0") => {
                overlay.entry("Core".into()).or_default().insert(key,"6".into());
            }
            Some(_) => {}
        }
    }
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
///   No Reborn Dolphin at "C:\\...\\NFL Street Reborn Dolphin.exe". Set the runtime in Settings.
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
/// Matched by name rather than by a fixed filename, because the fork is being
/// renamed from `Dolphin.exe` to `NFL Street Reborn Dolphin.exe`, and a tree
/// mid-update can hold both: an old copy left behind and the new one just
/// unpacked beside it. When both are present the new name wins, because it is
/// the one this launcher was built against; alphabetical order alone would
/// pick the old `dolphin.exe`.
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
            // Linux's build ships its disc tool beside the emulator as
            // `dolphin-tool`; it is not the emulator.
            name.contains("dolphin")
                && !name.contains("tool")
                && (name.ends_with(".exe") || !name.contains('.'))
        })
        .collect();
    // The Reborn name wins over a stock name when both are present, which happens on a
    // tree mid-update. Alphabetical order alone would pick "dolphin.exe".
    found.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        (if name.trim_end_matches(".exe") == "nfl street reborn dolphin" { 0 } else { 1 }, name)
    });
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

    // The managed runtime, `<data root>/runtime`, is where the content pipeline
    // installs every runtime update, and it has to be searched FIRST.
    //
    // It was searched last, which is to say never. The installer lays a copy
    // down beside the launcher at `<data root>/app/runtime`; `content::seed`
    // mirrors that into `<data root>/runtime` by hardlink on first run, so the
    // two are briefly the same bytes on disk; and from then on every update
    // writes the managed copy through `atomic_replace`, which is a rename. A
    // rename gives the destination a NEW inode and quietly severs the link. The
    // trees drift from that moment, and this function kept answering with the
    // installer's copy.
    //
    // The consequence was total: no runtime component update has reached the
    // emulator that actually runs since the install that created it. Measured
    // on the dev box 2026-09-14 - the running exe was 18,508,800 bytes while
    // the component had installed 18,516,992, and runtime 1.2.2's
    // `Sys/GameSettings` fix, the one that frees the video backend, had reached
    // nobody. Players were still reporting the backend stuck on Direct3D 11
    // with the fix live on the bucket.
    let managed = platform.data_dir(crate::platform::DataDir::Runtime).ok();
    let data_root = managed.as_ref().and_then(|m| m.parent()).map(|p| p.to_path_buf());

    // An explicit choice wins, so a developer or a Linux player is not
    // overruled. It is still reported as chosen, never as supported.
    //
    // But only when it points somewhere the launcher does not manage. An
    // ordinary install's saved `runtime_exe` names the installer's copy inside
    // the data root, and honouring THAT is precisely how updates were ignored:
    // it is not a choice anybody made, it is a path the launcher wrote down
    // for itself.
    if let Some(path) = chosen {
        let ours = data_root.as_ref().is_some_and(|root| path.starts_with(root));
        if path.is_file() {
            if !ours {
                return Ok(Runtime::Chosen(path.to_path_buf()));
            }
        } else {
            looked.push(path.to_path_buf());
        }
    }

    if let Some(dir) = &managed {
        looked.push(dir.clone());
        if let Some(found) = dolphin_in(dir) {
            return Ok(Runtime::Bundled(found));
        }
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
    use crate::player_sentence::assert_player_safe;

    /// Reborn's art only ever paints a disc Reborn verified. A player who chose
    /// to play a modified NFL Street 2 anyway must see it as what it is, not
    /// wearing Reborn's faces over someone else's roster.
    ///
    /// The unverified answer is `Withhold`, not `LeaveAlone`. `LeaveAlone`
    /// composes nothing and leaves the folder exactly as it is, and for
    /// everyone who has ever booted successfully "exactly as it is" means
    /// Reborn's whole pack still sitting there under `GN7E69` for Dolphin to
    /// load. Asserting the variant, rather than a boolean that used to carry
    /// both meanings, is what keeps the two from collapsing back together.
    #[test]
    fn an_image_ruled_out_never_gets_rebornes_texture_pack() {
        use crate::mods::TexturePolicy;
        use crate::game_identity::ImageStanding;
        assert_eq!(
            texture_policy_for_launch(ImageStanding::NotReborn, true),
            TexturePolicy::Withhold,
            "an image ruled out must have the pack actively taken away, not merely not rebuilt"
        );
        assert_eq!(
            texture_policy_for_launch(ImageStanding::Reborn, true),
            TexturePolicy::Manage,
            "a verified image must still get the pack"
        );
    }

    /// The other direction of being wrong, and the one the law itself caused.
    ///
    /// While this took a `bool`, "I could not read the image" and "this image
    /// is not ours" were the same value, so a missing DolphinTool or a disc
    /// held open for a moment removed the player's whole composed pack. That
    /// is where "missing textures" came from. Withholding is a ruling and
    /// needs an answer; not knowing must cost the player nothing.
    #[test]
    fn an_image_that_could_not_be_read_costs_the_player_nothing() {
        use crate::mods::TexturePolicy;
        use crate::game_identity::ImageStanding;
        assert_eq!(
            texture_policy_for_launch(ImageStanding::Unknown, true),
            TexturePolicy::LeaveAlone,
            "a question that was never answered took the player's texture pack away"
        );
        assert_ne!(
            texture_policy_for_launch(ImageStanding::Unknown, true),
            texture_policy_for_launch(ImageStanding::NotReborn, true),
            "not knowing and ruling out have collapsed back into one answer"
        );
    }

    /// The author's folder wins over the withholding rule.
    ///
    /// Somebody who turned management off asserted that the contents of
    /// `Load/Textures/<game>` are theirs. Withholding removes a tree, and
    /// removing out of a folder the player has claimed is a worse failure
    /// than the one it would prevent: the composed marker is not proof the
    /// tree is free of their own edits.
    #[test]
    fn the_authors_folder_is_never_taken_away_even_on_an_unverified_image() {
        use crate::mods::TexturePolicy;
        use crate::game_identity::ImageStanding;
        for standing in [ImageStanding::NotReborn, ImageStanding::Reborn, ImageStanding::Unknown] {
            assert_eq!(
                texture_policy_for_launch(standing, false),
                TexturePolicy::LeaveAlone,
                "an author's folder must never be touched, whatever the disc is: {standing:?}"
            );
        }
    }

    /// Review Focus 1. Two members boot the same league bytes in lockstep: an offline league boot must never write a
    /// customization into the league image.
    #[test]
    fn the_league_image_is_never_customized() {
        let src = include_str!("session.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        assert!(
            production.contains("if on_booted_offline && req.build.is_customizable() {"),
            "the Gamebreaker glyph is written into the booted image; it must be gated on a customizable build"
        );
        assert!(!crate::runtime::GameBuild::League.is_customizable());
    }

    fn installed_league(root: &std::path::Path) -> PathBuf {
        let iso = crate::league_edition::league_image_path(root);
        std::fs::create_dir_all(iso.parent().unwrap()).unwrap();
        std::fs::write(&iso, b"NSRS04").unwrap();
        std::fs::write(
            crate::league_edition::league_info_path(root),
            r#"{"schema":"nsr.league-game.v1","game_id":"NSRS04","iso":"game/league.iso","textures":"textures/NSRS04"}"#,
        )
        .unwrap();
        iso
    }

    /// The retail verdict judges retail discs. Asked about the league disc it says "not Reborn", and NotReborn
    /// withholds, which DELETES a composed tree. A league launch must never select Withhold, whatever the caller's
    /// retail standing says: the installed league image composes, anything else is left alone.
    #[test]
    fn a_league_launch_never_withholds_its_textures() {
        use crate::game_identity::ImageStanding;
        use crate::mods::TexturePolicy;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let installed = installed_league(root);
        let elsewhere = root.join("somewhere").join("league.iso");
        for caller in [ImageStanding::NotReborn, ImageStanding::Reborn, ImageStanding::Unknown] {
            for (path, data_root) in [(&installed, Some(root)), (&elsewhere, Some(root)), (&installed, None)] {
                let standing = standing_for_build(GameBuild::League, path, data_root, caller);
                assert_ne!(standing, ImageStanding::NotReborn, "{caller:?} {}", path.display());
                assert_ne!(
                    texture_policy_for_launch(standing, true),
                    TexturePolicy::Withhold,
                    "a league launch withheld for {caller:?} at {}",
                    path.display()
                );
            }
            assert_eq!(
                standing_for_build(GameBuild::League, &installed, Some(root), caller),
                ImageStanding::Reborn,
                "the installed league image is the league's own"
            );
        }
        // An installed image whose league.json names another build is not vouched for, and still not deleted.
        std::fs::write(
            crate::league_edition::league_info_path(root),
            r#"{"schema":"nsr.league-game.v1","game_id":"NSRS05","iso":"game/league.iso","textures":"textures/NSRS05"}"#,
        )
        .unwrap();
        assert_eq!(
            standing_for_build(GameBuild::League, &installed, Some(root), ImageStanding::NotReborn),
            ImageStanding::Unknown
        );
    }

    /// The retail standing logic is unchanged: every other build passes the caller's standing straight through.
    #[test]
    fn retail_standing_passes_through_unchanged() {
        use crate::game_identity::ImageStanding;
        let dir = tempfile::tempdir().unwrap();
        let installed = installed_league(dir.path());
        for build in [GameBuild::Offline, GameBuild::Online] {
            for caller in [ImageStanding::NotReborn, ImageStanding::Reborn, ImageStanding::Unknown] {
                assert_eq!(standing_for_build(build, &installed, Some(dir.path()), caller), caller);
            }
        }
    }

    /// The league tree is the only one a league launch composes: the sync is keyed by the build's game ID, so
    /// `Load/Textures/GN7E69` is never touched, and the run path routes the standing through the league rule.
    #[test]
    fn a_league_launch_syncs_nsrs04_and_routes_its_standing() {
        assert_eq!(GameBuild::League.game_id(), "NSRS04");
        assert_ne!(GameBuild::League.game_id(), "GN7E69");
        let src = include_str!("session.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        assert!(production.contains("let game = crate::mods::GameId::parse(build.game_id())?;"));
        assert!(
            production.contains("standing_for_build(req.build, &req.game_path, data_root.as_deref(), req.textures)"),
            "the launch must pass the league rule's standing to the texture sync, not the caller's retail one"
        );
    }

    #[test]
    fn background_sync_reports_collection_without_claiming_delivery() {
        let root = tempfile::tempdir().unwrap();
        let queue = crate::stats::StatsQueue::open_at(&root.path().join("queue"), crate::supervisor::SystemClock).unwrap();
        let entry = queue.enqueue(b"{}", None, Some("local-session"), Some("offline")).unwrap();
        let mut results = SessionResults { queue: Some(queue), history: vec![], enqueued: vec![entry], problem: None };
        let note = results.background_note().unwrap();
        assert!(note.contains("saved"));
        assert!(!note.contains("sent") && !note.contains("sign-in"), "No network request has answered yet");
        assert_player_safe(&note);
        assert_eq!(results.queue.as_ref().unwrap().entries().unwrap().len(), 1, "The background handoff keeps queued data");
        results.problem = Some(StatsProblem::Collecting);
        assert_eq!(results.background_note().as_deref(), Some(StatsProblem::Collecting.message()));
        results.queue = None;
        assert_eq!(results.background_note().as_deref(), Some(RESULTS_NOT_COLLECTED));
    }

    /// A fresh user folder seeded from the runtime template has no memory
    /// card at all (SlotA = 255); the launch turns slot A into a raw card
    /// with no path, so Dolphin uses its default inside the user folder.
    /// A working card choice is never touched.
    #[test]
    fn a_launch_gives_a_cardless_user_folder_a_memory_card_and_leaves_a_working_one_alone() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path();
        let none = crate::ini::IniDocument::parse("[Core]\nSlotA = 255\nSlotB = 255\n");
        let overlay = memory_card_overlay(&none, user);
        assert_eq!(overlay["Core"]["SlotA"], "1");
        assert!(!overlay["Core"].contains_key("MemcardAPath"), "no path: Dolphin's default inside the user folder is the path law");
        assert!(!overlay["Core"].contains_key("SlotB"), "slot B stays as it was");
        // no [Core] section at all: the same card
        assert_eq!(memory_card_overlay(&crate::ini::IniDocument::parse("[Display]\n"), user)["Core"]["SlotA"], "1");
        // a folder of GCIs, or a raw card: nothing to change
        assert!(memory_card_overlay(&crate::ini::IniDocument::parse("[Core]\nSlotA = 8\n"), user).is_empty());
        assert!(memory_card_overlay(&crate::ini::IniDocument::parse("[Core]\nSlotA = 1\nMemcardAPath = C:/cards/A.raw\n"), user).is_empty());
        // slot none with a card path named: the slot is turned on; the path is the path law's business
        let doc = crate::ini::IniDocument::parse("[Core]\nSlotA = 255\nMemcardAPath = D:/cards/A.raw\n");
        let on = memory_card_overlay(&doc, user);
        assert_eq!(on["Core"]["SlotA"], "1");
        assert!(!on["Core"].contains_key("MemcardAPath"));
    }

    /// The launch gives the single-instance lock back before the network work.
    ///
    /// This is the primary path: the Play button, every time. Two secondary
    /// ways of starting the emulator were fixed to collect, give the lock and
    /// the guard back, and only then send. `Session::run` was left calling
    /// `collect_and_send`, which does both under the lock it is still holding.
    ///
    /// A send is one request per queued game with the client's full timeout
    /// each, so a backlog against a slow or unreachable backend is minutes.
    /// For all of those minutes, with the emulator already exited, pressing
    /// Play answered "A game session is already active", the Online page said
    /// to close the emulator first, opening the emulator from Settings was
    /// refused, and `rebornctl stats retry` was refused. Every one of those was
    /// false.
    ///
    /// Collecting stays under the lock and that is not negotiable: the session
    /// id has to be in hand and no second launcher may be moving captures into
    /// the same queue. Only the send moves out.
    ///
    /// # How this is bounded so it cannot find itself
    ///
    /// The slice runs from `pub fn run(` to the doc comment that introduces
    /// [`SessionResults`], both of which are production text roughly eight
    /// hundred lines above this module. `find` returns the first match, so the
    /// copies of those markers inside this test are never reached, and the
    /// searched text ends long before them. The stop marker matters: run one
    /// item further and the slice swallows `SessionResults::send`, whose whole
    /// job is the network call this test forbids.
    #[test]
    fn the_launch_gives_the_lock_back_before_the_network_work() {
        let src = include_str!("session.rs");
        let from = src
            .find("    pub fn run(platform: &Platform, req: &LaunchRequest)")
            .expect("Session::run is not in this file any more");
        let rest = &src[from..];
        let body = &rest[..rest
            .find("\n/// The results of a finished session")
            .expect("the SessionResults doc comment no longer follows Session::run")];

        assert!(
            !body.contains("collect_and_send"),
            "Session::run still collects and sends in one call, with the single-instance lock \
             held across the network work, so Play says a game session is active and the Online \
             page says to close an emulator that has already exited"
        );

        let collect = body
            .find("collect(platform, &supervisor")
            .expect("Session::run no longer collects what the session produced under the lock");
        let release = body.find("drop(supervisor)").unwrap_or_else(|| {
            panic!(
                "Session::run never gives the single-instance lock back before it returns, so \
                 the caller cannot send outside it"
            )
        });
        assert!(
            collect < release,
            "Session::run gives the lock back before the results are safely collected"
        );

        // And the send really is the caller's job now: nothing in the body
        // reaches the backend.
        for reaches_the_network in ["auto_submit", ".send(platform)", ".send(&p)"] {
            assert!(
                !body.contains(reaches_the_network),
                "Session::run still does the network work itself, under the lock and under the \
                 caller's launch guard"
            );
        }
    }

    /// The stats step has nowhere to put an error string.
    ///
    /// `collect_and_send` used to write `e.to_string()` into three places: the
    /// `Failed` payload, and twice into `problem`. [`crate::Error::Io`] displays
    /// as `io error at {path}: {source}`, so a full disk while writing the queue
    /// put a path out of the player's roaming profile and `os error 112` into a
    /// value the window renders. `explain()` happened to summarize those two
    /// fields rather than repeat them, which is the only reason nobody had seen
    /// it on screen; any caller that showed the payload, and the command line
    /// tool is one line away from being one, would have.
    ///
    /// So the fix is the type and not the wording. `StatsAfterSession` now has
    /// nowhere to put a string at all: the failure is named
    /// ([`StatsProblem`]), the sentence is a constant, and the detail goes to
    /// `tracing::warn!` where it is produced. This test holds both halves,
    /// because "it happens not to be rendered today" is not a guarantee.
    ///
    /// `collect_and_send` itself is gone: the launch used it and that is how
    /// the primary path kept sending under the lock. Its two halves,
    /// `SessionResults::send` and `collect`, are what is scanned now, which is
    /// strictly more code than the one-line wrapper ever held.
    #[test]
    fn a_stats_result_has_nowhere_to_put_an_error_string() {
        let src = include_str!("session.rs");
        // Both halves of what `collect_and_send` used to be, each bounded by
        // the production text that follows it. All of it is hundreds of lines
        // above this module, and `find` returns the first match, so this
        // test's own text is never inside what is searched.
        for (name, start, stop) in [
            ("the send", "impl SessionResults {", "\n/// Collect the match results"),
            ("the collection", "pub fn collect(", "\n/// Close a session"),
        ] {
            let from = src
                .find(start)
                .unwrap_or_else(|| panic!("{name} is not in this file any more"));
            let rest = &src[from..];
            let body = &rest[..rest
                .find(stop)
                .unwrap_or_else(|| panic!("{name} no longer ends where it did"))];
            assert!(
                !body.contains("e.to_string()"),
                "{name} puts an error string on a value a player sees"
            );
        }

        // Every sentence the type can produce, mechanically checked.
        assert_player_safe(RESULTS_NOT_COLLECTED);
        for problem in [StatsProblem::Collecting, StatsProblem::Sending] {
            assert_player_safe(problem.message());
        }
        assert_player_safe(&StatsAfterSession::Failed.explain().expect("a failure says something"));

        let mut report = crate::stats::SubmitReport::default();
        report.accepted.push(crate::stats::CaptureKey::of_bytes(b"a"));
        report.held.push(crate::stats::CaptureKey::of_bytes(b"b"));
        report.rejected.push(crate::stats::CaptureKey::of_bytes(b"c"));
        report.stalled.push(crate::stats::CaptureKey::of_bytes(b"d"));
        report.waiting_for_sign_in = true;
        for problem in [None, Some(StatsProblem::Collecting), Some(StatsProblem::Sending)] {
            for sent in [None, Some(crate::stats::SubmitReport::default()), Some(report.clone())] {
                let s = StatsAfterSession::Ingested { enqueued: Vec::new(), problem, sent };
                if let Some(line) = s.explain() {
                    assert_player_safe(&line);
                }
            }
        }
    }

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
            league_match: false,
            play_with_unset_controller: true,
            textures: crate::game_identity::ImageStanding::Reborn,
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

    /// The launch path, not just the text function. A launch that sees no pad
    /// used to leave the seeded Xbox-only file in charge, and a PlayStation
    /// pad the scan had missed was dead for the session. What is on disk
    /// afterwards must be pressable by that pad.
    #[test]
    fn a_launch_that_sees_no_pad_leaves_player_one_bindable_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let pad = dir.path().join("GCPadNew.ini");
        std::fs::write(&pad, "[GCPad1]\nDevice = XInput/0/Gamepad\nButtons/A = `Button A`\nButtons/Start = Start\n").unwrap();

        assert!(leave_standing_pad_file(&pad).unwrap(), "the seeded file is rewritten");
        let after = std::fs::read_to_string(&pad).unwrap();
        assert!(after.contains("Buttons/A = `Button A`|`Cross`"), "{after}");
        assert!(after.contains("Buttons/Start = `Start`|`Menu`"), "{after}");

        assert!(!leave_standing_pad_file(&pad).unwrap(), "and then there is nothing left to say");
        assert_eq!(std::fs::read_to_string(&pad).unwrap(), after, "so nothing is written");
    }

    /// A keyboard player has no pad on every launch of their life. Their file
    /// is theirs, byte for byte.
    #[test]
    fn a_launch_that_sees_no_pad_never_touches_a_keyboard_players_file() {
        let dir = tempfile::tempdir().unwrap();
        let pad = dir.path().join("GCPadNew.ini");
        let theirs = "[GCPad1]\r\nDevice = DInput/0/Keyboard Mouse\r\nButtons/A = X\r\nMain Stick/Up = UP\r\n";
        std::fs::write(&pad, theirs).unwrap();
        assert!(!leave_standing_pad_file(&pad).unwrap());
        assert_eq!(std::fs::read_to_string(&pad).unwrap(), theirs);
    }

    /// A pad that turns up after the scan only reaches the game if player
    /// one's port is on. Off or unset is switched on; a value somebody chose
    /// is not the launcher's to change.
    #[test]
    fn a_launch_that_sees_no_pad_still_switches_player_one_on() {
        let off = crate::ini::IniDocument::parse("[Core]\nSIDevice0 = 0\n");
        let overlay = controller_port_overlay(&off, &player_one_port());
        assert_eq!(overlay.get("Core").and_then(|c| c.get("SIDevice0")).map(String::as_str), Some("6"));

        let adapter = crate::ini::IniDocument::parse("[Core]\nSIDevice0 = 12\n");
        assert!(controller_port_overlay(&adapter, &player_one_port()).is_empty(), "a GameCube adapter stays");
    }

    /// No pad file at all is the same player with even less: they still get a
    /// player one.
    #[test]
    fn a_launch_that_sees_no_pad_and_no_pad_file_writes_one() {
        let dir = tempfile::tempdir().unwrap();
        let pad = dir.path().join("GCPadNew.ini");
        assert!(leave_standing_pad_file(&pad).unwrap());
        assert!(std::fs::read_to_string(&pad).unwrap().starts_with("[GCPad1]\n"));
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
    fn the_new_exe_name_wins_when_both_are_present() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Dolphin.exe"), b"").unwrap();
        std::fs::write(dir.path().join("NFL Street Reborn Dolphin.exe"), b"").unwrap();
        assert_eq!(dolphin_in(dir.path()).unwrap().file_name().unwrap(), "NFL Street Reborn Dolphin.exe");
    }

    /// The Linux build is `dolphin-emu`, with `dolphin-tool` beside it.
    /// Alphabetical order happens to pick the emulator; nothing should rest on
    /// that, so the disc tool is never a candidate at all.
    #[test]
    fn a_linux_runtime_is_found_and_its_disc_tool_is_not_the_emulator() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("dolphin-tool"), b"").unwrap();
        assert!(dolphin_in(dir.path()).is_none(), "the disc tool alone is no emulator");
        std::fs::write(dir.path().join("dolphin-emu"), b"").unwrap();
        assert_eq!(dolphin_in(dir.path()).unwrap().file_name().unwrap(), "dolphin-emu");
        std::fs::write(dir.path().join("NFL Street Reborn Dolphin"), b"").unwrap();
        assert_eq!(dolphin_in(dir.path()).unwrap().file_name().unwrap(), "NFL Street Reborn Dolphin");
    }

    #[test]
    fn explain_counts_sent_held_recorded_and_waiting() {
        let mut r = crate::stats::SubmitReport::default();
        r.accepted.push(crate::stats::CaptureKey::of_bytes(b"a"));
        r.held.push(crate::stats::CaptureKey::of_bytes(b"b"));
        let s = StatsAfterSession::Ingested { enqueued: vec![], problem: None, sent: Some(r) };
        let text = s.explain().unwrap();
        assert!(text.contains("1 result sent"), "{text}");
        assert!(text.contains("1 held for review"), "{text}");
        let mut w = crate::stats::SubmitReport::default();
        w.waiting_for_sign_in = true;
        let s = StatsAfterSession::Ingested { enqueued: vec![], problem: None, sent: Some(w) };
        assert!(s.explain().unwrap().contains("waiting for sign-in"));
    }

    /// Collecting and sending are two jobs. A problem in one is reported and
    /// never stops the other, and neither is described to a player in the
    /// words of an error: no path, no error number, no library.
    ///
    /// Both values used to be built from `e.to_string()`. They now cannot be:
    /// `Failed` carries nothing and `problem` is a [`StatsProblem`]. What this
    /// test still has to prove is the other half, that a problem in one job
    /// never hides the other job's result.
    #[test]
    fn a_collection_problem_is_reported_without_showing_a_path_or_an_error_code() {
        let s = StatsAfterSession::Failed;
        let text = s.explain().expect("a failure has to say something");
        assert!(!text.contains("os error"), "{text}");
        assert!(!text.contains(r"C:\"), "{text}");
        assert!(text.contains("Nothing was lost"), "{text}");

        let mut r = crate::stats::SubmitReport::default();
        r.accepted.push(crate::stats::CaptureKey::of_bytes(b"a"));
        let s = StatsAfterSession::Ingested {
            enqueued: vec![],
            problem: Some(StatsProblem::Collecting),
            sent: Some(r),
        };
        let text = s.explain().expect("a send that happened has to say so");
        assert!(
            text.contains("1 result sent"),
            "the send must still have run and still be reported: {text}"
        );
        assert!(text.contains("being kept"), "the problem must be named: {text}");
        assert!(!text.contains("os error"), "{text}");
        assert!(!text.contains(r"C:\"), "{text}");
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

    /// The bug that made every runtime update invisible to the emulator.
    ///
    /// An ordinary install saves `runtime_exe` pointing at the INSTALLER's copy,
    /// `<data root>/app/runtime`. The content pipeline installs updates into the
    /// MANAGED copy, `<data root>/runtime`. The two start as the same bytes
    /// (content::seed hardlinks the mirror on first run) and drift the moment an
    /// update lands, because `atomic_replace` renames a staged file over the
    /// destination and a rename makes a new inode.
    ///
    /// So the saved path must not win. It is not a choice a person made; it is a
    /// path the launcher wrote down for itself, and honouring it meant runtime
    /// 1.2.2's `Sys/GameSettings` fix reached nobody while sitting live on the
    /// bucket.
    ///
    /// Machine-dependent by design: there is no way to fake a data root through
    /// `Platform::detect()`, so this proves itself against the real install when
    /// there is one and stands aside when there is not.
    #[test]
    fn a_saved_path_inside_the_data_root_never_beats_the_managed_runtime() {
        let platform = Platform::detect();
        let Ok(managed) = platform.data_dir(crate::platform::DataDir::Runtime) else { return };
        let Some(found) = dolphin_in(&managed) else { return };
        let Some(data_root) = managed.parent() else { return };

        let installer_copy = data_root
            .join("app")
            .join("runtime")
            .join(found.file_name().expect("the emulator has a file name"));
        if !installer_copy.is_file() {
            return;
        }

        let runtime = resolve_runtime(&platform, Some(&installer_copy), None)
            .expect("the managed runtime is right there");
        assert_eq!(
            runtime,
            Runtime::Bundled(found),
            "the saved installer path won over the managed copy, which is the tree every update lands in",
        );
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

        // On a machine that HAS a Reborn install, the managed runtime at
        // <data root>/runtime is a real answer and the search rightly succeeds.
        // That became true on 2026-09-14, when the managed copy started being
        // searched at all; before it, this function could not see the only tree
        // updates ever reach. Nothing to prove about the failure listing here.
        if resolve_runtime(&platform, Some(&missing), Some(&empty_legacy)).is_ok() {
            return;
        }
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

    /// The bug this exists to stop coming back.
    ///
    /// `netplay_open` sets `build: GameBuild::Offline` and swaps only
    /// `game_path`, and the online image shares the offline disc id, so
    /// gating on either of those silently includes netplay. Before
    /// 2026-09-18 `gamecube_controls` was gated on the game id and wrote its
    /// [OnFrame] no-swap patch on a netplay launch, which desynced a
    /// GameCube-profile player against everyone else on the first frame.
    /// Adam reproduced that.
    #[test]
    fn the_image_netplay_boots_is_recognised_and_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let online = crate::online_image::online_dir(root).join(crate::online_image::IMAGE_FILE);
        std::fs::create_dir_all(online.parent().unwrap()).unwrap();
        std::fs::write(&online, b"online").unwrap();
        assert!(is_netplay_image(root, &online), "netplay's image must be recognised");
    }

    /// The other half, and the reason the question is asked this way round:
    /// ordinary offline play must still get its per-game writes, including a
    /// game path that is not the standard booted image. Requiring the booted
    /// path would take the GameCube no-swap patch away from those players.
    #[test]
    fn ordinary_offline_play_is_never_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let booted = crate::game_image::booted_path(root);
        std::fs::create_dir_all(booted.parent().unwrap()).unwrap();
        std::fs::write(&booted, b"offline").unwrap();
        assert!(!is_netplay_image(root, &booted), "the booted image is not netplay");

        let elsewhere = root.join("Games").join("NFL Street 2 FIELDS 12-19.iso");
        std::fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
        std::fs::write(&elsewhere, b"a player's own path").unwrap();
        assert!(!is_netplay_image(root, &elsewhere), "a non-standard offline path is not netplay");

        let missing = root.join("not-on-disk.iso");
        assert!(!is_netplay_image(root, &missing), "a path that does not exist is not netplay");
    }
}
