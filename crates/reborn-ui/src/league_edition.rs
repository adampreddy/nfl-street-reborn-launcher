//! The League edition's commands (Plan 3b; contracts file "Plan 3b → Plan 3a") and the two helpers the netplay
//! path shares with them. Every rejection is a fixed sentence; nothing here ever shows a status, URL or link.
//!
//! Membership is the cached League answer (3a's `league_home` cache, `league.json` beside `identity.json`): every
//! league launch, room, friendly and install is refused with [`NOT_A_MEMBER`] when it says not a member. The files
//! stay on disk; a member again plays again. The server is the second gate (rooms and downloads are members-only).
//!
//! None of these commands changes the page: the League page opens the room page itself (contracts, rulings).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reborn_core::content::pipeline::{CheckRequest, ComponentState, ContentCheck, Depth, Updater};
use reborn_core::inventory::ComponentId;
use reborn_core::league_edition::{self as le, LeagueError, LeagueInfoError, LeagueSource, RoomOpen};
use reborn_core::origin::Origin;
use reborn_core::platform::Platform;

pub(crate) const NOT_A_MEMBER: &str = "League files are for league members. Sign in with the Discord account your team is linked to.";
pub(crate) const NOT_INSTALLED: &str = "Install the league files from the League tab first.";
pub(crate) const WRONG_BUILD: &str = "These league files are for a different league build. Update the launcher, then update the league files.";
pub(crate) const DAMAGED: &str = "The league files look damaged. Update league files on the League tab to repair them.";

const SIGN_IN_FIRST: &str = "Sign in with Discord on the Play page first.";
const BUSY: &str = "The league server is busy. Try again in a minute.";
const UNAVAILABLE: &str = "The league server couldn't be reached. Try again in a minute.";
const NOT_PUBLISHED: &str = "League files aren't available yet. Check back after the commissioners publish them.";
const INSTALL_FAILED: &str = "The league files didn't finish downloading. Press Install again to resume where it stopped.";
const CLOSE_THE_GAME: &str = "Close the game, then press Install again to finish. Nothing downloaded is lost.";
const NO_SPACE_LEFT: &str = "The drive the game is installed on is full. Free up some space, then press Install again.";
const ALREADY_INSTALLING: &str = "The league files are already being installed.";
pub(crate) const NO_GAME_NOW: &str = "There's no league game for you this week.";
pub(crate) const ALREADY_FINAL: &str = "This week's series is already final.";
pub(crate) const OPPONENT_UNLINKED: &str = "Your opponent hasn't linked Discord on the league site yet.";
pub(crate) const ROOM_FAILED: &str = "The league game couldn't be opened. Try again.";

/// Room sentences the matchmaking server already words for players (backend `src/mm/rooms.js` `ROOM_ERRORS`), the
/// only refusal text ever shown as the server sent it. Field complaints ("ruleset must be ...") are not on the list.
const PASS_THROUGH: [&str; 9] = [
    "No such lobby.",
    "That lobby is no longer open.",
    "That lobby is full.",
    "You are already in a lobby.",
    "You are already in a match.",
    "Your opponent is already in a match.",
    "Someone in your lobby is already in a match.",
    "Could not open a lobby just now. Try again.",
    "This league game's series is already final.",
];

/// The two members-only components the League edition installs.
const COMPONENTS: [ComponentId; 2] = [ComponentId::LeagueGame, ComponentId::LeagueContent];

/// The backend signs each download link for 6 h. Links older than this are asked for again after any failure, not
/// only after a refusal the blob store marked as an expired link.
const LINK_MAX_AGE: Duration = Duration::from_secs(5 * 3600 + 30 * 60);

/// One install at a time: a second press while the first downloads is refused, never run alongside it.
static INSTALLING: AtomicBool = AtomicBool::new(false);

/// `{ kind, message }` like every League command, plus `required` for a version refusal that names one (contracts,
/// rulings). No `required` means "update" with nothing to compare.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LeagueCommandError {
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<String>,
}

fn err(kind: &'static str, message: &str) -> LeagueCommandError {
    LeagueCommandError { kind, message: message.to_string(), required: None }
}

fn update_required(required: String) -> LeagueCommandError {
    LeagueCommandError {
        kind: "league_update_required",
        message: reborn_core::matchmaking::LEAGUE_UPDATE_SENTENCE.to_string(),
        required: Some(required).filter(|r| !r.trim().is_empty()),
    }
}

/// A refusal of [`ready_league_image`] (always one of this file's sentences) as a command error.
fn image_error(message: String) -> LeagueCommandError {
    let kind = match message.as_str() {
        NOT_A_MEMBER => "not_member",
        NOT_INSTALLED | WRONG_BUILD | DAMAGED => "not_installed",
        _ => "unavailable",
    };
    LeagueCommandError { kind, message, required: None }
}

/// The installed league build's version (its content pointer), or `None` when it is not installed.
pub(crate) fn installed_league_version() -> Option<String> {
    let p = Platform::detect();
    let svc = reborn_core::content::ContentService::for_platform(&p).ok()?;
    svc.installed(reborn_core::inventory::ComponentId::LeagueGame.as_str()).ok().flatten().map(|ptr| ptr.current.version)
}

/// Whether the cached League answer says this player is a member. A member whose league server is down keeps their
/// cache (`league_home::resolve`), so an outage never locks them out; signing out removes it.
pub(crate) fn is_member(p: &Platform) -> bool {
    reborn_core::league_home::cache_path(p).map(|path| is_member_at(&path)).unwrap_or(false)
}

fn is_member_at(cache: &Path) -> bool {
    reborn_core::league_home::load_cache(cache).member
}

/// The league disc, ready to boot: a member, the build installed, `league.json` naming this launcher's game ID, the
/// image there. Every league boot (offline, room, friendly) passes here.
pub(crate) fn ready_league_image(p: &Platform) -> Result<PathBuf, String> {
    let data = p.paths.data_root().map_err(|_| NOT_INSTALLED.to_string())?;
    ready_league_image_in(&data, is_member(p))
}

fn ready_league_image_in(data: &Path, member: bool) -> Result<PathBuf, String> {
    if !member {
        return Err(NOT_A_MEMBER.into());
    }
    match le::read_league_info(data) {
        Ok(_) => {}
        Err(LeagueInfoError::Missing) => return Err(NOT_INSTALLED.into()),
        Err(LeagueInfoError::WrongGameId(_)) => return Err(WRONG_BUILD.into()),
        Err(LeagueInfoError::Unreadable) => return Err(DAMAGED.into()),
    }
    let image = le::league_image_path(data);
    if !image.is_file() {
        return Err(DAMAGED.into());
    }
    Ok(image)
}

pub(crate) fn league_error(e: LeagueError) -> LeagueCommandError {
    match e {
        LeagueError::NotMember => err("not_member", NOT_A_MEMBER),
        LeagueError::RateLimited => err("rate_limited", BUSY),
        LeagueError::SignedOut => err("unavailable", SIGN_IN_FIRST),
        LeagueError::UnknownVersion => err("unavailable", NOT_PUBLISHED),
        LeagueError::Unavailable | LeagueError::Unreachable | LeagueError::BadReply => err("unavailable", UNAVAILABLE),
    }
}

/// A room answer in the contracts' shape. A refusal is a fixed sentence: a known backend code becomes the launcher's
/// own words, a known matchmaking sentence passes, and anything else (a field complaint, a stack trace, a link) is
/// [`ROOM_FAILED`].
pub(crate) fn room_view(open: RoomOpen) -> Result<serde_json::Value, LeagueCommandError> {
    match open {
        RoomOpen::Created(room) => Ok(serde_json::json!({ "action": "created", "room": room })),
        RoomOpen::Join(code) => Ok(serde_json::json!({ "action": "join", "code": code })),
        RoomOpen::InSeries(match_id) => Ok(serde_json::json!({ "action": "in_series", "match_id": match_id })),
        RoomOpen::UpdateRequired(required) => Err(update_required(required)),
        RoomOpen::Refused(why) => Err(match why.as_str() {
            "not_member" => err("not_member", NOT_A_MEMBER),
            "no_game" => err("unavailable", NO_GAME_NOW),
            "already_final" => err("unavailable", ALREADY_FINAL),
            "opponent_unlinked" => err("unavailable", OPPONENT_UNLINKED),
            s if PASS_THROUGH.contains(&s) => err("unavailable", s),
            _ => err("unavailable", ROOM_FAILED),
        }),
    }
}

pub(crate) fn friendly_ruleset(ruleset: &str) -> Result<&'static str, LeagueCommandError> {
    match ruleset.trim() {
        "league" => Ok("league"),
        "raw" => Ok("raw"),
        _ => Err(err("unavailable", ROOM_FAILED)),
    }
}

/// `installed` needs both the pointer and a `league.json` this launcher accepts, so a damaged or foreign install shows
/// Install (which repairs it), never Play.
pub(crate) fn status_view(installed: Option<&str>, latest: Option<&str>, size_bytes: u64, info_ok: bool) -> serde_json::Value {
    let is_installed = installed.is_some() && info_ok;
    serde_json::json!({
        "installed": is_installed,
        "version": installed,
        "latest": latest,
        "update_available": is_installed && latest.is_some_and(|l| Some(l) != installed),
        "size_bytes": size_bytes,
        "game_id": le::LEAGUE_GAME_ID,
    })
}

/// The bearer and the backend origin. Never shown: a missing token is [`SIGN_IN_FIRST`].
fn bearer(p: &Platform) -> Result<(String, String), LeagueCommandError> {
    let token = reborn_core::identity::token(p)
        .ok()
        .flatten()
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| err("unavailable", SIGN_IN_FIRST))?;
    let origin = reborn_core::identity::origin(p).map_err(|_| err("unavailable", UNAVAILABLE))?;
    Ok((token, origin))
}

/// What one download pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attempt {
    Done,
    /// Some files did not arrive whole (their `.part` files are kept).
    Incomplete,
    /// The pass stopped; `expired` when the blob store refused a signed link (`league_edition::link_expired`).
    Failed { expired: bool },
    /// The disk filled up. Pressing Install again would only fill it again.
    NoSpace,
}

/// Whether a pipeline error is the disk filling up.
pub(crate) fn is_storage_full(e: &reborn_core::Error) -> bool {
    match e {
        reborn_core::Error::Io { source, .. } => source.kind() == std::io::ErrorKind::StorageFull,
        reborn_core::Error::PlainIo(io) => io.kind() == std::io::ErrorKind::StorageFull,
        _ => false,
    }
}

/// Free space an install still needs: what is left to download plus what the swap keeps (the plan's peak, minus the
/// bytes an earlier attempt already left), plus the same 1 GB margin a drive move keeps.
pub(crate) fn space_needed(peak: u64, already: u64) -> u64 {
    peak.saturating_sub(already).saturating_add(reborn_core::relocate::MARGIN_BYTES)
}

fn no_space(needed: u64) -> LeagueCommandError {
    let message = format!(
        "There isn't enough free space for the league files. They need {:.1} GB free on the drive the game is installed on. Free up some space, then press Install again.",
        needed as f64 / 1_073_741_824.0
    );
    LeagueCommandError { kind: "no_space", message, required: None }
}

/// `free`: the data root's drive's free bytes, `None` when it could not be read (then the download is not refused).
pub(crate) fn space_check(free: Option<u64>, needed: u64) -> Result<(), LeagueCommandError> {
    match free {
        Some(free) if free < needed => Err(no_space(needed)),
        _ => Ok(()),
    }
}

/// Download, and when a pass fails on an expired link or after the links have aged past [`LINK_MAX_AGE`], ask the
/// backend for fresh links once and download again (which resumes every `.part` file). Bounded: one reissue per
/// Install press, then [`INSTALL_FAILED`], and the next press resumes.
pub(crate) fn fetch_with_fresh_links(
    mut links_age: impl FnMut() -> Duration,
    mut fetch: impl FnMut() -> Attempt,
    mut reissue: impl FnMut() -> Result<(), LeagueCommandError>,
) -> Result<(), LeagueCommandError> {
    let mut reissued = false;
    loop {
        let expired = match fetch() {
            Attempt::Done => return Ok(()),
            // The caller knows the size and words it (`no_space`); this sentence is the fallback.
            Attempt::NoSpace => return Err(err("no_space", NO_SPACE_LEFT)),
            Attempt::Incomplete => false,
            Attempt::Failed { expired } => expired,
        };
        if reissued || !(expired || links_age() >= LINK_MAX_AGE) {
            return Err(err("unavailable", INSTALL_FAILED));
        }
        reissue()?;
        reissued = true;
    }
}

/// Whether a download progress event is worth sending: about 200 per install, and always the last byte.
pub(crate) fn progress_due(before: u64, now: u64, total: u64) -> bool {
    let (before, now) = (before.min(total), now.min(total));
    let step = (total / 200).max(1);
    now > before && (now == total || before / step != now / step)
}

/// Bytes an earlier attempt already left on disk (staged files, `.part` files), each capped at its planned size, so a
/// resumed install's bar starts where the last one stopped.
pub(crate) fn bytes_on_disk<'a>(downloads: &Path, staging: &Path, files: impl Iterator<Item = (&'a str, u64)>) -> u64 {
    let under = |root: &Path, rel: &str| rel.split('/').fold(root.to_path_buf(), |p, seg| p.join(seg));
    let len = |p: &Path| std::fs::metadata(p).ok().filter(|m| m.is_file()).map(|m| m.len());
    files
        .map(|(rel, bytes)| {
            let staged = len(&under(staging, rel));
            let part = || {
                let full = under(downloads, rel);
                let name = format!("{}.part", full.file_name()?.to_string_lossy());
                len(&full.with_file_name(name))
            };
            staged.or_else(part).unwrap_or(0).min(bytes)
        })
        .sum()
}

/// Install state, and the newest version the signed release offers (network; absent offline).
#[tauri::command]
pub async fn league_edition_status() -> Result<serde_json::Value, LeagueCommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        let data = p.paths.data_root().map_err(|_| err("unavailable", UNAVAILABLE))?;
        let installed = installed_league_version();
        let info_ok = le::read_league_info(&data).is_ok();
        let (mut latest, mut size) = (None, 0u64);
        if let (Ok(settings), Ok(verifier)) = (crate::content_updates::load_settings(&p), crate::content_updates::verifier()) {
            let origin = Origin::new(&crate::content_updates::origin_url(&settings));
            let updater = Updater::new(&p, &data, &origin, &verifier);
            if let Ok(check) = updater.check(&CheckRequest { components: &COMPONENTS, depth: Depth::Quick }) {
                if let Some(game) = check.component(ComponentId::LeagueGame) {
                    latest = match &game.state {
                        ComponentState::UpToDate { version, .. } => Some(version.clone()),
                        ComponentState::Available(plan) => Some(plan.version().to_string()),
                        _ => None,
                    };
                }
                size = check.download_bytes();
            }
        }
        Ok(status_view(installed.as_deref(), latest.as_deref(), size, info_ok))
    })
    .await
    .map_err(|_| err("unavailable", UNAVAILABLE))?
}

/// Releases [`INSTALLING`] however the install ends.
struct InstallSlot;

impl InstallSlot {
    fn take() -> Result<Self, LeagueCommandError> {
        INSTALLING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| InstallSlot)
            .map_err(|_| err("unavailable", ALREADY_INSTALLING))
    }
}

impl Drop for InstallSlot {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::SeqCst);
    }
}

/// Download (members-only links), verify, swap in, then compose the league texture tree. Resumes a previous attempt.
/// Emits `league-install-progress` `{phase: "download"|"verify"|"compose", done, total}`; resolves `{version}`.
#[tauri::command]
pub async fn league_edition_install(app: tauri::AppHandle) -> Result<serde_json::Value, LeagueCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        let _slot = InstallSlot::take()?;
        let p = Platform::detect();
        if !is_member(&p) {
            return Err(err("not_member", NOT_A_MEMBER));
        }
        let (token, api) = bearer(&p)?;
        let data = p.paths.data_root().map_err(|_| err("unavailable", UNAVAILABLE))?;
        let settings = crate::content_updates::load_settings(&p).map_err(|_| err("unavailable", UNAVAILABLE))?;
        let verifier = crate::content_updates::verifier().map_err(|_| err("unavailable", UNAVAILABLE))?;
        let origin = Origin::new(&crate::content_updates::origin_url(&settings));
        let say = |phase: &str, done: u64, total: u64| {
            let _ = app.emit("league-install-progress", serde_json::json!({ "phase": phase, "done": done, "total": total }));
        };

        // 1. What the signed release offers, from the public origin. A Verify check hashes the installed tree too, so
        //    Install also repairs a damaged build. It takes a while on a 6 GB build: the bar says so from the start.
        say("verify", 0, 1);
        let check = Updater::new(&p, &data, &origin, &verifier)
            .check(&CheckRequest { components: &COMPONENTS, depth: Depth::Verify })
            .map_err(|e| {
                eprintln!("league: the release could not be checked: {e}");
                err("unavailable", UNAVAILABLE)
            })?;
        match check.component(ComponentId::LeagueGame).map(|c| &c.state) {
            None | Some(ComponentState::NotCarried) | Some(ComponentState::NotUpdatable) => {
                return Err(err("unavailable", NOT_PUBLISHED))
            }
            Some(ComponentState::Problem(_)) | Some(ComponentState::Refused(_)) => {
                eprintln!("league: the league build cannot be installed: {}", check.problems().join(" "));
                return Err(err("unavailable", UNAVAILABLE));
            }
            Some(ComponentState::UpToDate { .. }) | Some(ComponentState::Available(_)) => {}
        }
        if !check.has_updates() {
            return match le::read_league_info(&data) {
                Ok(_) => Ok(serde_json::json!({ "version": installed_league_version().unwrap_or_default() })),
                Err(LeagueInfoError::WrongGameId(_)) => Err(err("not_installed", WRONG_BUILD)),
                Err(_) => Err(err("not_installed", DAMAGED)),
            };
        }

        // 2. One signed link per blob, from the backend (members only), for each component that needs fetching.
        let http = crate::http_client();
        let links = |check: &ContentCheck| -> Result<Vec<le::GatedFile>, LeagueCommandError> {
            let mut all = Vec::new();
            for (cc, plan) in check.available() {
                all.extend(le::request_download(&http, &api, &token, cc.component.as_str(), plan.version()).map_err(league_error)?);
            }
            Ok(all)
        };
        let total = check.download_bytes();
        let layout = reborn_core::content::ContentService::new(&p, &data);
        let already: u64 = check
            .available()
            .map(|(_, plan)| {
                let l = layout.layout();
                bytes_on_disk(
                    &l.downloads_dir(plan.component(), plan.version()),
                    &l.staging_dir(plan.component(), plan.version()),
                    plan.fetches.iter().map(|f| (f.path.as_str(), f.bytes)),
                )
            })
            .sum();
        // Room for what is left to download plus what the swap keeps, before a link is asked for: a full disk would
        // otherwise fail every press of Install the same way.
        let peak: u64 = check.available().map(|(_, plan)| plan.peak_extra_bytes()).sum();
        let needed = space_needed(peak, already);
        space_check(reborn_core::platform::dir_move::volume_facts(&data).map(|f| f.free_bytes), needed)?;
        let done = Arc::new(AtomicU64::new(already.min(total)));
        let tick = {
            let done = done.clone();
            let app = app.clone();
            Arc::new(move |n: u64| {
                let before = done.fetch_add(n, Ordering::Relaxed);
                let now = before.saturating_add(n);
                if progress_due(before, now, total) {
                    let _ = app.emit("league-install-progress", serde_json::json!({ "phase": "download", "done": now.min(total), "total": total }));
                }
            }) as Arc<dyn Fn(u64) + Send + Sync>
        };
        say("download", already.min(total), total);
        let source = RefCell::new(LeagueSource::new(origin.clone(), &links(&check)?).with_progress(tick));
        let issued = Cell::new(Instant::now());

        // 3. Fetch. Links last 6 h: an expired (or old) link asks for fresh links once and resumes the `.part` files.
        fetch_with_fresh_links(
            || issued.get().elapsed(),
            || {
                let src = source.borrow();
                match Updater::new(&p, &data, &*src, &verifier).fetch(&check) {
                    Ok(outcome) if outcome.all_staged() => Attempt::Done,
                    Ok(_) => Attempt::Incomplete,
                    Err(e) => {
                        // The source strips every signature from its errors; this line never carries a link.
                        eprintln!("league: the download stopped: {e}");
                        if is_storage_full(&e) {
                            Attempt::NoSpace
                        } else {
                            Attempt::Failed { expired: le::link_expired(&e) }
                        }
                    }
                }
            },
            || {
                let fresh = links(&check)?;
                source.borrow_mut().refresh(&fresh);
                issued.set(Instant::now());
                Ok(())
            },
        )
        .map_err(|e| if e.kind == "no_space" { no_space(needed) } else { e })?;
        say("download", total, total);

        // 4. Verify and swap in (the pipeline hashes everything before the swap), with the game closed: the league
        //    disc may be the file a running game holds open.
        let _guard = crate::LaunchGuard::acquire().map_err(|m| {
            if crate::session_active() { err("unavailable", CLOSE_THE_GAME) } else { LeagueCommandError { kind: "unavailable", message: m, required: None } }
        })?;
        say("verify", 0, 1);
        let src = source.borrow();
        let promoted = Updater::new(&p, &data, &*src, &verifier)
            .promote(&check, &crate::content_updates::image_hook(&p, &settings))
            .map_err(|e| {
                eprintln!("league: the swap failed: {e}");
                if is_storage_full(&e) { no_space(needed) } else { err("unavailable", INSTALL_FAILED) }
            })?;
        if !promoted.all_applied() {
            eprintln!("league: the swap did not apply: {promoted:?}");
            return Err(err("unavailable", INSTALL_FAILED));
        }
        say("verify", 1, 1);

        // 5. The installed build must be the one this launcher boots; then its textures + the shared mods compose
        //    (only from an installed league build, judged by session's rule for it: `sync_active_judged_for`).
        match le::read_league_info(&data) {
            Ok(_) => {}
            Err(LeagueInfoError::WrongGameId(_)) => return Err(err("not_installed", WRONG_BUILD)),
            Err(_) => return Err(err("not_installed", DAMAGED)),
        }
        say("compose", 0, 1);
        if let Err(e) = crate::sync_active_judged_for(&p, None, None, reborn_core::runtime::GameBuild::League) {
            // Not fatal: every league launch composes again before boot.
            eprintln!("league: compose after install did not finish: {e}");
        }
        say("compose", 1, 1);
        Ok(serde_json::json!({ "version": installed_league_version().unwrap_or_default() }))
    })
    .await
    .map_err(|_| err("unavailable", INSTALL_FAILED))?
}

/// `play_anyway` as `launch` takes it: absent asks first (the controller check runs), exactly like the Play page.
pub(crate) fn play_anyway_arg(play_anyway: Option<bool>) -> Option<bool> {
    Some(play_anyway.unwrap_or(false))
}

/// What `launch` answered, as this command's answer. A controller that needs setup is not a failure: it is
/// `{kind: "controller_setup", message}` with the Play page's own sentence, and the League page offers "Set up
/// controller" and "Play anyway" (contracts, ruling after the Task 8 review).
pub(crate) fn offline_answer(result: crate::LaunchResult) -> Result<serde_json::Value, LeagueCommandError> {
    if result.started {
        Ok(serde_json::Value::Null)
    } else if result.controller_setup.is_some() {
        Err(LeagueCommandError { kind: "controller_setup", message: result.message, required: None })
    } else {
        Err(LeagueCommandError { kind: "unavailable", message: result.message, required: None })
    }
}

/// Boot the league disc, raw, offline. Resolves `null` once the game has run; a refusal is a sentence.
/// `play_anyway: true` is the player's choice to go on after the controller-setup answer (09-19: a PS5/Switch pad
/// that cannot press Start must be offered setup first, never booted to a stuck title screen).
#[tauri::command]
pub async fn league_play_offline(app: tauri::AppHandle, play_anyway: Option<bool>) -> Result<serde_json::Value, LeagueCommandError> {
    // The League page carries no graphics or window choices of its own: the ones the Play page last reported are
    // used (mm_launch_prefs), else Balanced, fullscreen.
    let (runtime, game, preset) = crate::online_mm::launch_prefs();
    let preset = if preset.is_empty() { "balanced".to_string() } else { preset };
    let result = crate::launch(app, runtime, game, preset, true, None, false, Some(false), play_anyway_arg(play_anyway), Some("league".into()))
        .await
        .map_err(image_error)?;
    offline_answer(result)
}

/// A members-only friendly on the league build (`league` seed or `raw`). Resolves `{action: "created", room}`.
#[tauri::command]
pub async fn league_host_friendly(ruleset: String) -> Result<serde_json::Value, LeagueCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let ruleset = friendly_ruleset(&ruleset)?;
        ready_league_image(&p).map_err(image_error)?;
        let (token, api) = bearer(&p)?;
        let version = installed_league_version();
        room_view(le::host_friendly(&crate::http_client(), &api, &token, ruleset, version.as_deref()).map_err(league_error)?)
    })
    .await
    .map_err(|_| err("unavailable", ROOM_FAILED))?
}

/// This week's league game: host it, join the opponent's room, or the series already running.
#[tauri::command]
pub async fn league_room_open() -> Result<serde_json::Value, LeagueCommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        ready_league_image(&p).map_err(image_error)?;
        let (token, api) = bearer(&p)?;
        let version = installed_league_version();
        room_view(le::open_room(&crate::http_client(), &api, &token, version.as_deref()).map_err(league_error)?)
    })
    .await
    .map_err(|_| err("unavailable", ROOM_FAILED))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use reborn_core::league_edition::{LeagueError, RoomOpen};

    #[test]
    fn the_league_disc_boots_only_when_its_build_is_installed_whole() {
        let dir = std::env::temp_dir().join(format!("reborn-ui-league-image-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(ready_league_image_in(&dir, true), Err(NOT_INSTALLED.to_string()));
        let info = le::league_info_path(&dir);
        std::fs::create_dir_all(info.parent().unwrap()).unwrap();
        let good = |id: &str| serde_json::json!({"schema":"nsr.league-game.v1","game_id":id,"iso":"game/league.iso","textures":format!("textures/{id}")}).to_string();
        std::fs::write(&info, good("ZZLG01")).unwrap();
        assert_eq!(ready_league_image_in(&dir, true), Err(WRONG_BUILD.to_string()));
        std::fs::write(&info, "not json").unwrap();
        assert_eq!(ready_league_image_in(&dir, true), Err(DAMAGED.to_string()));
        std::fs::write(&info, good(le::LEAGUE_GAME_ID)).unwrap();
        assert_eq!(ready_league_image_in(&dir, true), Err(DAMAGED.to_string()), "league.json without its disc");
        let image = le::league_image_path(&dir);
        std::fs::create_dir_all(image.parent().unwrap()).unwrap();
        std::fs::write(&image, b"disc").unwrap();
        assert_eq!(ready_league_image_in(&dir, true), Ok(image.clone()));
        assert_eq!(ready_league_image_in(&dir, false), Err(NOT_A_MEMBER.to_string()), "membership lost: refused");
        assert!(image.is_file() && info.is_file(), "a refusal never removes the league files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn membership_is_the_cached_league_answer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("league.json");
        assert!(!is_member_at(&path), "no cache: not a member");
        reborn_core::league_home::save_cache(&path, &reborn_core::league_home::LeagueCache { member: true, home: Some(serde_json::json!({"member":true})), fetched_at_ms: 1 });
        assert!(is_member_at(&path));
        reborn_core::league_home::save_cache(&path, &reborn_core::league_home::LeagueCache { member: false, home: None, fetched_at_ms: 2 });
        assert!(!is_member_at(&path));
    }

    #[test]
    fn room_answers_become_the_contracts_shapes() {
        assert_eq!(room_view(RoomOpen::Created(serde_json::json!({"code":"K"}))).unwrap(), serde_json::json!({"action":"created","room":{"code":"K"}}));
        assert_eq!(room_view(RoomOpen::Join("K7".into())).unwrap(), serde_json::json!({"action":"join","code":"K7"}));
        assert_eq!(room_view(RoomOpen::InSeries(Some("m".into()))).unwrap(), serde_json::json!({"action":"in_series","match_id":"m"}));
        let up = room_view(RoomOpen::UpdateRequired("2026.10.09".into())).unwrap_err();
        assert_eq!((up.kind, up.required.as_deref()), ("league_update_required", Some("2026.10.09")));
        assert_eq!(up.message, reborn_core::matchmaking::LEAGUE_UPDATE_SENTENCE);
        let msg = |why: &str| room_view(RoomOpen::Refused(why.into())).unwrap_err().message;
        assert_eq!(msg("opponent_unlinked"), OPPONENT_UNLINKED);
        assert_eq!(msg("no_game"), NO_GAME_NOW);
        assert_eq!(msg("already_final"), ALREADY_FINAL);
        assert_eq!(msg("not_member"), NOT_A_MEMBER);
        assert_eq!(room_view(RoomOpen::Refused("not_member".into())).unwrap_err().kind, "not_member");
        assert_eq!(msg("You are already in a lobby."), "You are already in a lobby.");
        assert_eq!(msg("You are already in a match."), "You are already in a match.");
        assert_eq!(msg("Your opponent is already in a match."), "Your opponent is already in a match.");
        assert_eq!(msg("https://x leaked"), ROOM_FAILED, "an unknown reason is never shown");
        assert_eq!(msg("ruleset must be league or raw."), ROOM_FAILED, "a field complaint is not a player sentence");
    }

    #[test]
    fn a_version_refusal_without_a_version_still_says_update() {
        let up = room_view(RoomOpen::UpdateRequired(String::new())).unwrap_err();
        assert_eq!(up.kind, "league_update_required");
        assert_eq!(up.message, reborn_core::matchmaking::LEAGUE_UPDATE_SENTENCE);
        assert_eq!(up.required, None, "no version to compare: the page shows the update sentence");
        let json = serde_json::to_value(&up).unwrap();
        assert!(json.get("required").is_none(), "{json}");
    }

    #[test]
    fn backend_failures_are_sentences() {
        assert_eq!(league_error(LeagueError::NotMember).kind, "not_member");
        assert_eq!(league_error(LeagueError::NotMember).message, NOT_A_MEMBER);
        assert_eq!(league_error(LeagueError::RateLimited).kind, "rate_limited");
        for e in [LeagueError::SignedOut, LeagueError::Unavailable, LeagueError::Unreachable, LeagueError::BadReply, LeagueError::UnknownVersion] {
            let c = league_error(e.clone());
            assert_eq!(c.kind, "unavailable", "{e:?}");
            assert!(!c.message.contains("http") && !c.message.contains("40") && !c.message.contains("50"), "{e:?}: {}", c.message);
        }
    }

    #[test]
    fn status_reads_installed_and_offered_versions() {
        assert_eq!(status_view(Some("2026.10.08"), Some("2026.10.08"), 0, true),
            serde_json::json!({"installed":true,"version":"2026.10.08","latest":"2026.10.08","update_available":false,"size_bytes":0,"game_id":reborn_core::league_edition::LEAGUE_GAME_ID}));
        assert_eq!(status_view(Some("2026.10.08"), Some("2026.10.09"), 700, true)["update_available"], true);
        assert_eq!(status_view(None, Some("2026.10.09"), 6_000_000_000, false)["installed"], false);
        assert_eq!(status_view(Some("2026.10.08"), None, 0, true)["update_available"], false, "offline: no news is not an update");
        assert_eq!(status_view(Some("2026.10.08"), Some("2026.10.08"), 0, false)["installed"], false, "a damaged league.json is not installed");
    }

    #[test]
    fn only_league_or_raw_friendlies() {
        assert!(friendly_ruleset("league").is_ok());
        assert!(friendly_ruleset("raw").is_ok());
        assert_eq!(friendly_ruleset("ranked").unwrap_err().kind, "unavailable");
    }

    fn expired() -> Attempt {
        Attempt::Failed { expired: true }
    }

    #[test]
    fn an_expired_link_asks_for_fresh_links_once_and_resumes() {
        let mut tries = vec![expired(), Attempt::Done];
        let mut refreshed = 0;
        let out = fetch_with_fresh_links(|| std::time::Duration::ZERO, || tries.remove(0), || { refreshed += 1; Ok(()) });
        assert!(out.is_ok());
        assert_eq!(refreshed, 1);
    }

    #[test]
    fn fresh_links_are_asked_for_at_most_once() {
        let mut tries = vec![expired(), expired(), Attempt::Done];
        let mut refreshed = 0;
        let out = fetch_with_fresh_links(|| std::time::Duration::ZERO, || tries.remove(0), || { refreshed += 1; Ok(()) });
        assert_eq!(out.unwrap_err().message, INSTALL_FAILED);
        assert_eq!(refreshed, 1, "bounded: one fresh set of links per Install press");
    }

    #[test]
    fn young_links_are_not_reissued_for_an_ordinary_failure() {
        for first in [Attempt::Incomplete, Attempt::Failed { expired: false }] {
            let mut tries = vec![first, Attempt::Done];
            let mut refreshed = 0;
            let out = fetch_with_fresh_links(|| std::time::Duration::from_secs(60), || tries.remove(0), || { refreshed += 1; Ok(()) });
            assert_eq!(out.unwrap_err().message, INSTALL_FAILED);
            assert_eq!(refreshed, 0);
        }
    }

    #[test]
    fn old_links_are_reissued_for_any_failure() {
        for first in [Attempt::Incomplete, Attempt::Failed { expired: false }] {
            let mut tries = vec![first, Attempt::Done];
            let mut refreshed = 0;
            let out = fetch_with_fresh_links(|| LINK_MAX_AGE, || tries.remove(0), || { refreshed += 1; Ok(()) });
            assert!(out.is_ok());
            assert_eq!(refreshed, 1);
        }
    }

    #[test]
    fn a_refused_reissue_is_the_backends_sentence() {
        let mut tries = vec![expired(), Attempt::Done];
        let out = fetch_with_fresh_links(|| std::time::Duration::ZERO, || tries.remove(0), || Err(league_error(LeagueError::NotMember)));
        assert_eq!(out.unwrap_err().kind, "not_member");
    }

    #[test]
    fn download_progress_is_throttled_but_always_reaches_the_end() {
        let total = 6_000_000_000u64;
        let mut sent = 0;
        let mut last = 0;
        let mut done = 0u64;
        while done < total {
            let before = done;
            done = (done + (1 << 20)).min(total);
            if progress_due(before, done, total) {
                sent += 1;
                last = done;
            }
        }
        assert!(sent <= 201, "{sent} events for 6 GB");
        assert_eq!(last, total, "the last chunk is always reported");
        assert!(progress_due(0, 5, 5));
    }

    #[test]
    fn part_files_already_on_disk_count_as_done() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("dl");
        let staging = dir.path().join("st");
        std::fs::create_dir_all(downloads.join("game")).unwrap();
        std::fs::create_dir_all(staging.join("textures")).unwrap();
        std::fs::write(downloads.join("game").join("league.iso.part"), vec![0u8; 40]).unwrap();
        std::fs::write(staging.join("textures").join("a.png"), vec![0u8; 7]).unwrap();
        let files = [("game/league.iso", 100u64), ("textures/a.png", 7), ("textures/b.png", 9)];
        assert_eq!(bytes_on_disk(&downloads, &staging, files.iter().map(|(p, b)| (*p, *b))), 47);
        std::fs::write(downloads.join("game").join("league.iso.part"), vec![0u8; 400]).unwrap();
        assert_eq!(bytes_on_disk(&downloads, &staging, files.iter().map(|(p, b)| (*p, *b))), 107, "never more than the file");
    }

    #[test]
    fn no_install_sentence_names_a_link_or_a_status() {
        for s in [SIGN_IN_FIRST, BUSY, UNAVAILABLE, NOT_PUBLISHED, INSTALL_FAILED, CLOSE_THE_GAME, NOT_A_MEMBER, ROOM_FAILED] {
            assert!(!s.contains("http") && !s.contains("://") && !s.chars().any(|c| c.is_ascii_digit()), "{s}");
        }
    }

    fn launched(started: bool, setup: bool) -> crate::LaunchResult {
        crate::LaunchResult {
            started,
            user_dir_note: None,
            message: if setup { "Your controller needs a one-time setup before it can be used in the game: Start would do nothing yet. Open Controllers and choose Set up this controller. It takes about a minute, and you only do it once.".into() } else { "Played.".into() },
            minutes: None,
            needs_recovery: false,
            pads: Vec::new(),
            pads_authoritative: false,
            mods_note: None,
            session_id: None,
            stats_note: None,
            cards_note: None,
            controller_setup: setup.then(|| crate::ControllerSetupView { device: "SDL/0/PS5".into(), controls: vec!["Start".into()] }),
        }
    }

    #[test]
    fn offline_play_asks_about_the_controller_first_like_the_play_page() {
        assert_eq!(play_anyway_arg(None), Some(false), "absent asks first");
        assert_eq!(play_anyway_arg(Some(false)), Some(false));
        assert_eq!(play_anyway_arg(Some(true)), Some(true), "the player's own choice to go on");
        let ask = offline_answer(launched(false, true)).unwrap_err();
        assert_eq!(ask.kind, "controller_setup");
        assert!(ask.message.starts_with("Your controller needs a one-time setup"), "the Play page's sentence: {}", ask.message);
        assert_eq!(offline_answer(launched(true, false)).unwrap(), serde_json::Value::Null, "play anyway goes through");
        assert_eq!(offline_answer(launched(false, false)).unwrap_err().kind, "unavailable");
        let body = &include_str!("league_edition.rs")[include_str!("league_edition.rs").find("pub async fn league_play_offline(").unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert!(body.contains("play_anyway: Option<bool>") && body.contains("play_anyway_arg(play_anyway)"), "{body}");
    }

    #[test]
    fn a_drive_without_room_refuses_before_any_download() {
        let gb = 1u64 << 30;
        assert!(space_check(Some(20 * gb), space_needed(6 * gb, 0)).is_ok());
        let full = space_check(Some(6 * gb), space_needed(6 * gb, 0)).unwrap_err();
        assert_eq!(full.kind, "no_space");
        assert!(full.message.contains("7.0 GB"), "the size plus the 1 GB margin: {}", full.message);
        assert!(space_check(None, 6 * gb).is_ok(), "a drive that could not be read is not a refusal");
        assert_eq!(space_needed(6 * gb, 2 * gb), 4 * gb + reborn_core::relocate::MARGIN_BYTES, "bytes already on disk are not needed again");
        assert_eq!(space_needed(gb, 5 * gb), reborn_core::relocate::MARGIN_BYTES);
    }

    #[test]
    fn a_full_disk_while_downloading_is_no_space_never_press_install_again() {
        let full = reborn_core::Error::Io { path: PathBuf::from("x"), source: std::io::Error::from(std::io::ErrorKind::StorageFull) };
        assert!(is_storage_full(&full));
        assert!(is_storage_full(&reborn_core::Error::PlainIo(std::io::Error::from(std::io::ErrorKind::StorageFull))));
        assert!(!is_storage_full(&reborn_core::Error::PlainIo(std::io::Error::from(std::io::ErrorKind::PermissionDenied))));
        let mut tries = vec![Attempt::NoSpace, Attempt::Done];
        let mut refreshed = 0;
        let out = fetch_with_fresh_links(|| LINK_MAX_AGE, || tries.remove(0), || { refreshed += 1; Ok(()) });
        let e = out.unwrap_err();
        assert_eq!(e.kind, "no_space");
        assert_eq!(refreshed, 0, "fresh links never fix a full disk");
    }

    #[test]
    fn the_bar_shows_verify_before_the_first_slow_check() {
        let src = include_str!("league_edition.rs");
        let body = &src[src.find("pub async fn league_edition_install(").unwrap()..];
        let first_say = body.find("say(\"verify\", 0, 1);").expect("verify 0/1 is said");
        let check = body.find("depth: Depth::Verify").unwrap();
        assert!(first_say < check, "the bar is never blank during the first check");
    }

    #[test]
    fn the_five_commands_are_registered() {
        let main = include_str!("main.rs");
        for c in ["league_edition_status", "league_edition_install", "league_play_offline", "league_host_friendly", "league_room_open"] {
            assert!(main.contains(&format!("league_edition::{c},")), "{c} is not in generate_handler!");
        }
    }

    #[test]
    fn offline_league_play_boots_the_league_build() {
        let main = include_str!("main.rs");
        let body = &main[main.find("async fn launch(").unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert!(body.contains("edition: Option<String>"));
        assert!(body.contains("crate::league_edition::ready_league_image(&p)?"));
        assert!(body.contains("GameBuild::League"));
        assert!(body.contains("standing_for_build("), "the league standing is session's, never the retail verdict");
    }

    #[test]
    fn a_league_texture_sync_needs_the_league_build_and_its_own_standing() {
        let main = include_str!("main.rs");
        let body = &main[main.find("pub(crate) fn sync_active_judged_for(").unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert!(body.contains("read_league_info"), "never compose NSRS04 without the league build");
        assert!(body.contains("standing_for_build("), "the league standing is session's, never the retail verdict");
    }

    #[test]
    fn commands_never_navigate() {
        let src = include_str!("league_edition.rs");
        let src = &src[..src.find("#[cfg(test)]").unwrap()];
        for word in ["navigate", "location", "online.html"] {
            assert!(!src.contains(word), "the League page navigates itself: {word}");
        }
    }
}
