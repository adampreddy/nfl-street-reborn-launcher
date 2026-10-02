//! Matchmaking and Discord presence, as the launcher runs them.
//!
//! ONE background thread owns the long poll. It keeps the latest server snapshot in memory for the
//! page to read, and it is the thing that keeps Discord presence current. Everything the page does
//! (queue, lobby, match actions) is a separate short request; it never waits on the poll, and it
//! never has to "nudge" it either - the server bumps the player's cursor and releases the long poll
//! itself, so the next snapshot arrives within a round trip.
//!
//! PRESENCE IS ONE DECISION IN ONE PLACE. What Discord shows is:
//!   matchmaking's view of the player, when it has one (queued, lobby, preparing, in a match),
//!   otherwise whatever page of the launcher is open.
//! Both the poller and the page's navigation call `refresh_presence`, so a page change shows
//! immediately rather than waiting up to a 25-second poll.
//!
//! NOTHING HERE MAY BLOCK PLAY. Discord not running, the servers unreachable, the player signed out:
//! each is logged, reflected in `mm_state`, and otherwise ignored. A failure here must never stop a
//! launch, a queue or a result upload.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use reborn_core::matchmaking::{activity_for, Client, MatchAction, MatchView, MmError, RoomAction, Snapshot};
use reborn_core::relay_driver::{self, Room, Step};
use reborn_core::platform::Platform;
use reborn_core::presence::{payload, Activity, Mode, Presence};
use reborn_core::presence_discord::{now_unix, DiscordPresence};

/// Where the poller stands, for the page to show plainly.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum Link {
    Starting,
    Connected,
    SignedOut,
    Offline,
    /// The server has matchmaking switched off. The page hides the whole section.
    Disabled,
}

struct State {
    link: Link,
    snapshot: Option<Snapshot>,
    last_error: Option<String>,
    /// The launcher page that is open, for presence when matchmaking has nothing to say.
    page: &'static str,
    /// The player's presence switch. Off means Discord shows nothing from us at all.
    presence_on: bool,
    /// Why the last relay step (opening the room, relaying the code, ...) did not land, in the
    /// words the step itself gave. Cleared by the next step that does.
    relay_error: Option<String>,
}

static STATE: Mutex<State> = Mutex::new(State {
    link: Link::Starting,
    snapshot: None,
    last_error: None,
    page: "Home",
    presence_on: true,
    relay_error: None,
});

/// What the relay opens Dolphin with: the runtime, game and graphics preset the page's own Play and
/// private-room paths use (they live in the page's storage, so the page reports them).
static LAUNCH: Mutex<(Option<String>, Option<String>, String)> = Mutex::new((None, None, String::new()));

static PRESENCE: Mutex<Option<Box<dyn Presence + Send>>> = Mutex::new(None);
static POLLER: AtomicBool = AtomicBool::new(false);
/// This launcher's own tally for the match it is streaming: (match id, [seat0, seat1] wins). The live loop writes it,
/// the relay loop reads it for the ranked close.
static LOCAL_WINS: Mutex<Option<(String, [u32; 2])>> = Mutex::new(None);
/// The match whose Dolphin the launcher is closing on purpose because the ranked series is decided. Its exit is not a
/// leave (Task 19 reads this).
pub(crate) static SERIES_CLOSING: Mutex<Option<String>> = Mutex::new(None);
/// The room lost its other player while this PC's Dolphin was still running (for the match id held). Set by the relay
/// loop, read when Dolphin exits (ruling R1).
static PEER_LOST: Mutex<Option<String>> = Mutex::new(None);

/// Take SERIES_CLOSING and PEER_LOST if (and only if) they name `match_id`: (closing, peer lost). A flag naming
/// another match is left for [`forget_other_matches`].
fn spend_exit_flags(match_id: &str) -> (bool, bool) {
    let take = |m: &Mutex<Option<String>>| {
        let mut g = lock(m);
        if g.as_deref() == Some(match_id) { g.take(); true } else { false }
    };
    (take(&SERIES_CLOSING), take(&PEER_LOST))
}

/// A different match is current (a rematch, a new queue match): flags left by an older one are dropped, so a close
/// that never made Dolphin exit cannot silence the next match's real leave.
fn forget_other_matches(current: &str) {
    for m in [&SERIES_CLOSING, &PEER_LOST] {
        let mut g = lock(m);
        if g.as_deref().is_some_and(|id| id != current) { *g = None; }
    }
}

/// The NetPlay Dolphin exited (dolphin_settings' exit thread, on a thread of its own so filing the captures never
/// waits on this request, R-P9). Mid-series, tell the server why (spec §3.4, R13). Nothing is reported while the
/// launcher closes a decided ranked series, nor when it is already decided but the close had not fired yet (I1).
pub fn dolphin_exited(exit: Option<&reborn_core::platform::ExitKind>) {
    let m = lock(&STATE).snapshot.as_ref().and_then(|s| s.matched.clone());
    let Some(m) = m else { return };
    let (closing, peer_lost) = spend_exit_flags(&m.id);
    let running = m.status == "running" && matches!(m.relay_state.as_str(), "running" | "intermission");
    let local = lock(&LOCAL_WINS).as_ref().filter(|(id, _)| id == &m.id).map(|(_, w)| *w).unwrap_or([0, 0]);
    let decided = reborn_core::series::decided_for_exit(
        &m.mode, local, reborn_core::series::wins_needed(&m), m.outcome.as_ref().map(|o| o.kind.as_str()));
    let action = match reborn_core::series::exit_report(exit, running, closing || decided, peer_lost) {
        reborn_core::series::ExitReport::Nothing => {
            if running && (closing || decided) { mm_log(&format!("dolphin exited after the series was decided ({exit:?}): not reported")); }
            return;
        }
        reborn_core::series::ExitReport::Leave(r) => MatchAction::LeaveWith(r),
        reborn_core::series::ExitReport::RoomLost(why) => MatchAction::RoomLost(why.to_string()),
    };
    mm_log(&format!("dolphin exited mid-series ({exit:?}): {action:?}"));
    if let Err(e) = with_client(|c| c.match_action(&m.id, action)) { mm_log(&format!("exit report FAILED: {e}")); }
}

/// One line to `<data root>/logs/matchmaking.log`. The launcher is a windowed app, so stderr goes
/// nowhere; the first real test left no trace of what either launcher saw (2026-09-23). Never
/// fails, never blocks play: a log that cannot be written is skipped.
pub(crate) fn mm_log(line: &str) {
    use std::io::Write;
    let Ok(root) = Platform::detect().paths.data_root() else { return };
    let dir = root.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("matchmaking.log")) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "{secs} {line}");
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A poisoned lock here means a panic mid-update of display state. Recover the data rather than
    // take the whole launcher down over what Discord shows.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Pages that are worth naming in presence. Anything else reads as "In the launcher".
fn page_label(page: &str) -> &'static str {
    match page {
        "mods" | "Mods" => "Mods",
        "controls" | "Controls" => "Controls",
        "settings" | "Settings" => "Settings",
        "stats" | "Stats" => "Stats",
        "online" | "Online" => "Online",
        "music" | "Music" => "Music",
        _ => "Home",
    }
}

/// The single presence decision. Called by the poller and by page navigation.
pub fn refresh_presence() {
    let (activity, on) = {
        let s = lock(&STATE);
        let from_mm = s.snapshot.as_ref().and_then(|snap| activity_for(snap, now_unix()));
        let activity = from_mm.unwrap_or(match s.page {
            "Home" => Activity::Launcher,
            other => Activity::Browsing { page: other },
        });
        (activity, s.presence_on)
    };

    let mut p = lock(&PRESENCE);
    let presence = p.get_or_insert_with(|| Box::new(DiscordPresence::with_default_app()));
    let result = if on { presence.set(&payload(&activity)) } else { presence.clear() };
    // Ordinary to fail: Discord may simply not be running, and that is the normal case for plenty of
    // players. Deliberately not logged - this runs on every poll and would fill the log with a
    // non-event. The transport already reconnects on its own the next time Discord is up.
    let _ = result;
}

/// Start the long-poll thread once. Safe to call repeatedly.
pub fn start(platform: Platform) {
    if POLLER.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("reborn-matchmaking".into())
        .spawn(move || run(platform))
        .ok();
    std::thread::Builder::new()
        .name("reborn-mm-relay".into())
        .spawn(drive)
        .ok();
    // `Platform` is not `Clone`, and `run` owns the one passed in.
    std::thread::Builder::new()
        .name("reborn-live-plays".into())
        .spawn(move || live_plays_loop(Platform::detect()))
        .ok();
}

// ------------------------------------------------------------------------------------------------
// The live feed: every play of a running match, posted as Dolphin writes it. `live_plays` decides; this
// only feeds it the match, the Dolphin folder and the token, once a second.
// ------------------------------------------------------------------------------------------------

/// After the match leaves "running", keep sending what is already written for this long, then stop.
const LIVE_FLUSH_MS: u64 = 120_000;

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// A tick that only skipped plays (a restarted game, a line past the series) logs at most this often.
const LIVE_SKIP_LOG_MS: u64 = 60_000;

fn live_plays_loop(platform: Platform) {
    let state_dir = match platform.data_dir(reborn_core::platform::DataDir::Cache) {
        Ok(d) => d.join("live"),
        Err(e) => {
            mm_log(&format!("live feed: off, no cache folder ({e})"));
            return;
        }
    };
    // Dolphin's play tracker writes to this folder for every game, not only a matched one, so a player
    // who never queues for matchmaking -- and so never runs a `LivePoster`, the only other place this
    // ever happens -- would otherwise never have it swept at all. One pass, here at startup, against
    // whatever folder NetPlay itself would boot against right now (see `netplay_open`); nothing to
    // resolve yet (no runtime found, no settings written) just skips it, same as any other best-effort
    // sweep.
    if let Ok((_, user_dir)) = crate::dolphin_settings::config_user(&platform, None) {
        reborn_core::live_plays::prune_live(&user_dir, &state_dir, None);
    }
    let http = crate::http_client();
    let mut current: Option<(String, reborn_core::live_plays::LivePoster)> = None;
    let mut ended_at: Option<u64> = None;
    // A poster stopped by a 401 reports it again on every tick; say it once, and stop reading the vault.
    let mut halted = false;
    let mut last_skip_log: Option<u64> = None;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = now_ms();
        // One second's work. A panic in it must not end the feed for the rest of the session: it is
        // logged, the poster is dropped, and the next second starts a fresh one from the saved cursor.
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (connected, matched) = {
                let s = lock(&STATE);
                (s.link == Link::Connected && s.snapshot.is_some(), s.snapshot.as_ref().and_then(|x| x.matched.clone()))
            };
            // Not knowing is not "no match". A network blip empties the snapshot; taking that as the
            // match ending would start the flush and drop the poster. Hold everything as it is.
            if !connected {
                return;
            }
            let ctx = matched.as_ref().and_then(reborn_core::live_plays::match_ctx);
            let user_dir = crate::dolphin_settings::netplay_user_dir();
            match (&ctx, &user_dir) {
                (Some(ctx), Some(user)) => {
                    ended_at = None;
                    if current.as_ref().map(|(id, _)| id != &ctx.match_id).unwrap_or(true) {
                        mm_log(&format!("live feed: streaming match {}", ctx.match_id));
                        current = Some((ctx.match_id.clone(), reborn_core::live_plays::LivePoster::start(ctx.clone(), user, &state_dir)));
                        halted = false;
                    }
                    // Safe every tick: the poster latches each session file's base per Dolphin process when it
                    // first lists it, so this only ever reaches files a relaunched Dolphin writes from now on.
                    if let Some((_, poster)) = current.as_mut() {
                        poster.set_games_before(ctx.games_before);
                    }
                }
                _ => {
                    if current.is_none() {
                        return;
                    }
                    let since = *ended_at.get_or_insert(now);
                    let flushed = now.saturating_sub(since) > LIVE_FLUSH_MS
                        || current.as_ref().is_some_and(|(_, p)| p.ready(now) && p.caught_up());
                    if flushed {
                        mm_log("live feed: match over, stopped");
                        current = None;
                        ended_at = None;
                        return;
                    }
                }
            }
            let Some((_, poster)) = current.as_mut() else { return };
            if halted {
                return;
            }
            // Backing off after a failure: nothing will be sent this second. Checked before `caught_up`,
            // which reads and parses the next line: this tick only lists the folder and latches a new
            // file's game base, and writes nothing unless one was latched.
            if !poster.ready(now) {
                poster.tick(&http, "", "", now);
                return;
            }
            // Most seconds of a game have no new play: stop at the cheap size check, before touching the
            // credential vault or the network.
            if poster.caught_up() {
                return;
            }
            let (Ok(Some(token)), Ok(origin)) = (reborn_core::identity::token(&platform), reborn_core::identity::origin(&platform)) else { return };
            let r = poster.tick(&http, &origin, &token, now);
            if let Some((id, poster)) = current.as_ref() {
                *lock(&LOCAL_WINS) = Some((id.clone(), poster.tally().wins()));
            }
            let notable = r.posted + r.dropped > 0 || r.stopped.is_some();
            let skips_due = r.skipped > 0 && last_skip_log.is_none_or(|t| now.saturating_sub(t) >= LIVE_SKIP_LOG_MS);
            if notable || skips_due {
                if !notable {
                    last_skip_log = Some(now);
                }
                mm_log(&format!(
                    "live feed: posted {} dropped {} skipped {}{}{}",
                    r.posted,
                    r.dropped,
                    r.skipped,
                    r.last_drop.map(|d| format!(" (last drop: {d})")).unwrap_or_default(),
                    r.stopped.as_ref().map(|s| format!(" STOPPED ({s})")).unwrap_or_default()
                ));
            }
            halted = r.stopped.is_some();
        }));
        if ran.is_err() {
            mm_log("live feed: a tick panicked; dropped the poster, resuming from its saved cursor");
            current = None;
            ended_at = None;
            halted = false;
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The relay: getting both Dolphins into one room. `relay_driver::plan` decides; this runs it.
// ------------------------------------------------------------------------------------------------

/// Once a second: the server's view of the match (already in memory from the long poll) and this
/// PC's Dolphin room (the fork's status file), planned and run. A tick with no match and no room of
/// ours costs two reads and nothing else.
fn drive() {
    let mut mem = relay_driver::Memory::default();
    let mut last_room: Option<String> = None;
    let mut close: (String, reborn_core::series::RankedClose) = (String::new(), Default::default());
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let (connected, matched) = {
            let s = lock(&STATE);
            (s.link == Link::Connected && s.snapshot.is_some(), s.snapshot.as_ref().and_then(|x| x.matched.clone()))
        };
        // Not knowing is not "no match". A network blip empties the snapshot, and planning on that
        // would close a lobby two players are sitting in.
        if !connected {
            continue;
        }
        let room = Room::from_status(&crate::dolphin_settings::netplay_status().unwrap_or_default());
        // R1: did the other player drop out of a running room before our own Dolphin went away?
        if let Some(m) = matched.as_ref() {
            forget_other_matches(&m.id);
            if room.active && room.phase == "running" && room.players.len() < 2 {
                *lock(&PEER_LOST) = Some(m.id.clone());
            }
        }
        // The room as the fork reports it, logged when it changes. The 09-26 real match could
        // only be read back from the status file by hand; this is that file's history.
        if room.active || last_room.is_some() {
            let seen_room = format!(
                "room: phase={} can_start={} ports={:?} players=[{}]{}",
                room.phase, room.can_start, room.ports,
                room.players.iter().map(|p| format!("{}:{}", p.name, p.ping_ms.map(|m| format!("{m}ms")).unwrap_or_else(|| "-".into()))).collect::<Vec<_>>().join(" "),
                room.error.as_deref().map(|e| format!(" error={e:?}")).unwrap_or_default(),
            );
            if last_room.as_deref() != Some(seen_room.as_str()) {
                mm_log(&seen_room);
                last_room = Some(seen_room);
            }
            if !room.active { last_room = None; }
        }
        // Dolphin's own refusal ("Everyone needs the same game file before starting.", "Choose a
        // fixed graphics resolution...") used to vanish: the fork writes it to the status file
        // and nothing read it. It is the relay error now, on the page and in the log.
        if room.phase == "error" {
            if let Some(e) = room.error.as_deref().filter(|e| !e.is_empty()) {
                let msg = format!("Dolphin: {e}");
                if lock(&STATE).relay_error.as_deref() != Some(msg.as_str()) {
                    mm_log(&format!("room error: {e}"));
                    lock(&STATE).relay_error = Some(msg);
                }
            }
        }
        for step in relay_driver::plan(matched.as_ref(), &room, &mut mem) {
            let result = run_step(matched.as_ref(), &step);
            match result {
                Ok(()) => {
                    mm_log(&format!("relay ok {step:?} (match relay_state {})", matched.as_ref().map(|m| m.relay_state.as_str()).unwrap_or("-")));
                    // Where this room's Dolphin writes its own log (the session turns file
                    // logging on), so a player can be asked for it by path.
                    if matches!(step, Step::OpenRoom { .. }) {
                        if let Some(user) = crate::dolphin_settings::netplay_user_dir() {
                            mm_log(&format!("dolphin log: {}", user.join("Logs").join("dolphin.log").display()));
                        }
                    }
                    mem.record(&step);
                    lock(&STATE).relay_error = None;
                }
                Err(e) => {
                    mm_log(&format!("relay FAILED {step:?}: {e}"));
                    lock(&STATE).relay_error = Some(e);
                    // A room that would not open is not retried every second: the server's deadline
                    // abandons a pre-game match and puts the player back in the queue, which is the
                    // designed recovery, and the reason is on the page meanwhile.
                    if matches!(step, Step::OpenRoom { .. }) {
                        mem.record(&step);
                    }
                    break;
                }
            }
        }
        // The ranked close (series spec §3.2; ruling R11): at 2 wins, wait for the deciding capture (30 s at most),
        // then close the room ~3 s later. Each launcher does this on its own.
        if let Some(m) = matched.as_ref().filter(|m| m.mode == "ranked") {
            if close.0 != m.id { close = (m.id.clone(), Default::default()); }
            let local = lock(&LOCAL_WINS).as_ref().filter(|(id, _)| id == &m.id).map(|(_, w)| *w).unwrap_or([0, 0]);
            let over = reborn_core::series::series_over(local, reborn_core::series::wins_needed(m), m.outcome.as_ref().map(|o| o.kind.as_str()));
            if over && room.active {
                let newest = crate::dolphin_settings::netplay_user_dir().and_then(|u| reborn_core::series::newest_capture_ms(&u));
                let id = m.id.clone();
                let mut stop = || {
                    *lock(&SERIES_CLOSING) = Some(id.clone());
                    mm_log(&format!("series over ({}-{} by this launcher's tally): closing Dolphin", local[0], local[1]));
                    crate::dolphin_settings::netplay_action("leave".into(), None)
                };
                reborn_core::series::run_ranked_close(&mut close.1, now_ms(), over, newest, &mut stop);
            }
        }
    }
}

fn run_step(m: Option<&MatchView>, step: &Step) -> Result<(), String> {
    let id = m.map(|m| m.id.clone()).unwrap_or_default();
    let post = |a: MatchAction| with_client(|c| c.match_action(&id, a)).map(|_| ());
    let (runtime, game, preset) = lock(&LAUNCH).clone();
    match step {
        Step::Ready => post(MatchAction::Ready {
            ready: true,
            // The seed the SERVER named for this match (regular = quick game, pickup), so both
            // peers hash the same file and boot from it.
            preflight: crate::dolphin_settings::mm_preflight(runtime.as_deref(), game.as_deref(), m.and_then(|m| m.seed_kind.as_deref())),
            rtt_ms: None,
        }),
        Step::PostHostLaunched => post(MatchAction::HostLaunched),
        Step::PostHostCode(code) => post(MatchAction::HostCode(code.clone())),
        Step::PostJoinerLaunched => post(MatchAction::JoinerLaunched),
        Step::PostRoomLost(reason) => post(MatchAction::RoomLost(reason.clone())),
        Step::PostObserved { ports, players } => post(MatchAction::Observed {
            ports: ports.to_vec(),
            players: players.iter().map(|(name, ping)| serde_json::json!({ "name": name, "ping_ms": ping })).collect(),
        }),
        Step::OpenRoom { host, nickname, code } => {
            let preset = if preset.is_empty() { "balanced".to_string() } else { preset };
            tauri::async_runtime::block_on(crate::dolphin_settings::netplay_open(
                runtime,
                game,
                preset,
                nickname.clone(),
                if *host { "host" } else { "join" }.to_string(),
                "traversal".to_string(),
                code.clone().unwrap_or_default(),
                2626,
                Some(relay_driver::rng_seed_for(&id)),
                m.and_then(|m| m.seed_kind.clone()),
            ))
            .map(|_| ())
        }
        Step::AssignPorts(ports) => crate::dolphin_settings::netplay_action("ports".into(), Some(ports.to_vec())),
        Step::StartGame => crate::dolphin_settings::netplay_action("start".into(), None),
        Step::LeaveRoom => crate::dolphin_settings::netplay_action("leave".into(), None),
        // The server holds the match ("left-before-start") and frees both seats; the `held`
        // branch of the plan then closes the room.
        Step::GiveUp(_) => post(MatchAction::Leave),
    }
}

/// The page reports what Play would launch with, so the relay opens the same Dolphin.
#[tauri::command]
pub fn mm_launch_prefs(runtime: Option<String>, game: Option<String>, preset: Option<String>) {
    *lock(&LAUNCH) = (runtime, game, preset.unwrap_or_default());
}

fn run(platform: Platform) {
    let mut since: u64 = 0;
    let mut chat: u64 = 0;
    let mut epoch = String::new();
    let mut backoff = Duration::from_secs(2);
    let mut last_seen = String::new();

    loop {
        let token = reborn_core::identity::token(&platform).ok().flatten();
        let Some(token) = token else {
            set_link(Link::SignedOut, None, None);
            refresh_presence();
            std::thread::sleep(Duration::from_secs(5));
            continue;
        };
        let origin = match reborn_core::identity::origin(&platform) {
            Ok(o) => o,
            Err(e) => {
                set_link(Link::Offline, None, Some(e.to_string()));
                std::thread::sleep(backoff);
                continue;
            }
        };

        let http = crate::http_client();
        let client = Client::new(&http, origin, token);

        match client.live(since, chat) {
            Ok(snap) if !snap.changed => {
                // A "nothing changed" answer: the server's 25 s hold ran out, or it asked us to
                // back off. It carries cursors and NO snapshot fields. Taking it as a snapshot made
                // the launcher forget its own match every 25 quiet seconds -- the page fell back to
                // "Find a game", and the relay closed the Dolphin room as "no match" (2026-09-23,
                // Adam's first real test, 00:43:38 in matchmaking.log). Cursors move; state stays.
                backoff = Duration::from_secs(2);
                if !epoch.is_empty() && epoch != snap.epoch {
                    mm_log("poll: server restarted (quiet), re-reading from scratch");
                    since = 0;
                } else {
                    since = snap.version;
                }
                epoch = snap.epoch.clone();
                let wait = if snap.restarting { 250 } else { snap.poll_after_ms.min(10_000) };
                set_link(Link::Connected, None, None);
                if wait > 0 {
                    std::thread::sleep(Duration::from_millis(wait));
                }
            }
            Ok(snap) => {
                backoff = Duration::from_secs(2);
                // A new epoch means the server restarted. The snapshot is complete, so there is
                // nothing to reconcile - just stop trusting cursors minted by the old process.
                if !epoch.is_empty() && epoch != snap.epoch {
                    mm_log("poll: server restarted, re-reading from scratch");
                }
                epoch = snap.epoch.clone();
                since = snap.version;
                chat = snap.chat.latest_seq;
                let wait = if snap.restarting { 250 } else { snap.poll_after_ms.min(10_000) };
                // What the server says about this player, logged when it changes: when a launcher
                // first saw a match is exactly what the 2026-09-23 test could not answer.
                let seen = format!(
                    "ticket={} match={}",
                    snap.ticket.as_ref().map(|t| t.mode.as_str()).unwrap_or("-"),
                    snap.matched.as_ref().map(|m| format!("{}@{}", &m.id[..m.id.len().min(8)], m.relay_state)).unwrap_or_else(|| "-".into()),
                );
                if seen != last_seen {
                    mm_log(&format!("poll: {seen} (version {})", snap.version));
                    last_seen = seen;
                }
                set_link(Link::Connected, Some(snap), None);
                refresh_presence();
                if wait > 0 {
                    std::thread::sleep(Duration::from_millis(wait));
                }
            }
            Err(MmError::Disabled) => {
                // Checked again every minute, so switching matchmaking on needs no launcher update.
                set_link(Link::Disabled, None, None);
                refresh_presence();
                std::thread::sleep(Duration::from_secs(60));
            }
            Err(MmError::SignedOut) => {
                set_link(Link::SignedOut, None, None);
                refresh_presence();
                std::thread::sleep(Duration::from_secs(10));
            }
            Err(e @ MmError::Offline(_)) => {
                set_link(Link::Offline, None, Some(e.to_string()));
                refresh_presence();
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
            Err(e) => {
                mm_log(&format!("poll error: {e:?}"));
                set_link(Link::Connected, None, Some(e.to_string()));
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

fn set_link(link: Link, snap: Option<Snapshot>, err: Option<String>) {
    let mut s = lock(&STATE);
    s.link = link;
    if snap.is_some() || link != Link::Connected {
        s.snapshot = snap;
    }
    s.last_error = err;
}

// ------------------------------------------------------------------------------------------------
// Commands. Every mutating call is a short request; the poller picks up the result.
// ------------------------------------------------------------------------------------------------

fn with_client<T>(f: impl FnOnce(&Client) -> Result<T, MmError>) -> Result<T, String> {
    let p = Platform::detect();
    let token = reborn_core::identity::token(&p)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| MmError::SignedOut.to_string())?;
    let origin = reborn_core::identity::origin(&p).map_err(|e| e.to_string())?;
    let http = crate::http_client();
    let client = Client::new(&http, origin, token);
    f(&client).map_err(|e| e.to_string())
}

fn parse_mode(mode: &str) -> Result<Mode, String> {
    match mode {
        "casual" => Ok(Mode::Casual),
        "ranked" => Ok(Mode::Ranked),
        _ => Err("Choose casual or ranked.".into()),
    }
}

/// The matchmade match the relay is opening Dolphin for right now, if any. The NetPlay session records it in its
/// intent, so every result from that Dolphin names the match (series spec §3.1). None for Play and private rooms.
pub fn current_match_id() -> Option<String> {
    let s = lock(&STATE);
    s.snapshot.as_ref()?.matched.as_ref()
        .filter(|m| !matches!(m.relay_state.as_str(), "complete" | "void" | "abandoned" | "held"))
        .map(|m| m.id.clone())
}

/// Everything the Online page shows, in one read.
#[tauri::command]
pub fn mm_state() -> serde_json::Value {
    start(Platform::detect());
    let s = lock(&STATE);
    serde_json::json!({
        "link": s.link,
        "error": s.last_error,
        "relay_error": s.relay_error,
        "presence_on": s.presence_on,
        "snapshot": s.snapshot.as_ref().map(snapshot_json),
    })
}

/// The snapshot as the page wants it: the fields it draws, not the wire's whole shape.
fn snapshot_json(s: &Snapshot) -> serde_json::Value {
    serde_json::json!({
        "player": s.player.as_ref().map(|p| serde_json::json!({ "name": p.display_name, "region": p.region })),
        "ticket": s.ticket.as_ref().map(|t| serde_json::json!({
            "mode": t.mode, "field": t.field_pick, "waited_ms": t.waited_ms, "enqueued_at": t.enqueued_at,
        })),
        "room": s.room.as_ref().map(|r| serde_json::json!({
            "id": r.id, "code": r.code, "mode": r.mode, "best_of": r.best_of, "status": r.status, "you_host": r.you_host,
            // `you` is decided HERE, from the signed-in player's id, so the page never has to guess
            // which member it is. Member order is the server's and carries no meaning.
            "members": r.members.iter().map(|m| serde_json::json!({
                "name": m.display_name, "ready": m.ready, "field": m.field_pick,
                "you": s.player.as_ref().is_some_and(|p| !p.id.is_empty() && p.id == m.player_id),
            })).collect::<Vec<_>>(),
        })),
        "match": s.matched.as_ref().map(|m| serde_json::json!({
            "id": m.id, "mode": m.mode, "status": m.status, "relay_state": m.relay_state,
            "relay_deadline": m.relay_deadline, "best_of": m.best_of, "you_host": m.you_host,
            "opponent": m.opponent.display_name, "field": m.field.as_deref().map(reborn_core::matchmaking::field_title),
            // Display only (spec 2026-10-02 §3.3): null until the server sends them, and the page then hides each spot.
            "opponent_elo": m.opponent.rating, "opponent_rank": m.opponent.rank,
            "opponent_team": m.opponent.team, "opponent_avatar_url": m.opponent.avatar_url,
            "your_field_pick": m.your_field_pick, "your_accepted": m.your_accepted, "your_ready": m.your_ready,
            "series": m.series, "wins_needed": m.wins_needed, "winner_seat": m.winner_seat,
            "hold_reason": m.hold_reason, "seat": m.seat,
        })),
        "chat": s.chat.messages.iter().map(|c| serde_json::json!({
            "seq": c.seq, "name": c.display_name, "body": c.body, "at": c.at,
        })).collect::<Vec<_>>(),
        "queue_depth": { "ranked": s.queue_depth.ranked, "casual": s.queue_depth.casual },
        "online": { "count": s.online.count, "players": s.online.players.iter().map(|p| serde_json::json!({
            "name": p.display_name, "activity": p.activity,
        })).collect::<Vec<_>>() },
    })
}

#[tauri::command]
pub async fn mm_queue(mode: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mode = parse_mode(&mode)?;
        let r = with_client(|c| c.enqueue(mode, None))?;
        Ok(serde_json::json!({ "matched": r.matched, "match_id": r.match_id }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mm_leave_queue() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| with_client(|c| c.leave_queue())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mm_rooms() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let rooms = with_client(|c| c.rooms())?;
        Ok(serde_json::json!(rooms
            .iter()
            .map(|r| serde_json::json!({ "code": r.code, "mode": r.mode, "best_of": r.best_of, "host": r.host }))
            .collect::<Vec<_>>()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mm_room_create(mode: String, public: bool) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mode = parse_mode(&mode)?;
        with_client(|c| c.create_room(mode, public))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mm_room_join(code: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || with_client(|c| c.join_room(code.trim()))).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mm_room_action(room_id: String, action: String, value: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let a = match action.as_str() {
            "leave" => RoomAction::Leave,
            "ready" => RoomAction::Ready(value.as_ref().and_then(|v| v.as_bool()).unwrap_or(true)),
            "chat" => RoomAction::Chat(value.as_ref().and_then(|v| v.as_str()).unwrap_or_default().to_string()),
            "start" => RoomAction::Start,
            _ => return Err("That isn't something a lobby can do.".to_string()),
        };
        with_client(|c| c.room_action(&room_id, a))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The match actions a PLAYER takes from the page. The relay steps a launcher takes on its own
/// (host-launched, host-code, observed, ...) are driven by the netplay code, not offered here, so a
/// player cannot click them out of order.
#[tauri::command]
pub async fn mm_match_action(match_id: String, action: String, value: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let a = match action.as_str() {
            "accept" => MatchAction::Accept(value.as_ref().and_then(|v| v.as_bool()).unwrap_or(true)),
            "leave" => MatchAction::Leave,
            "chat" => MatchAction::Chat(value.as_ref().and_then(|v| v.as_str()).unwrap_or_default().to_string()),
            "rematch" => MatchAction::Rematch,
            _ => return Err("That isn't something you can do from here.".to_string()),
        };
        with_client(|c| c.match_action(&match_id, a))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The page tells presence where the player is. Instant, not tied to the poll.
#[tauri::command]
pub fn presence_page(page: String) {
    lock(&STATE).page = page_label(&page);
    refresh_presence();
}

/// The visible off switch presence must have. Off clears Discord immediately.
#[tauri::command]
pub fn presence_set_enabled(enabled: bool) {
    lock(&STATE).presence_on = enabled;
    refresh_presence();
}

/// Tests swap the real Discord client for one that records nothing.
#[cfg(test)]
pub(crate) fn use_no_presence() {
    *lock(&PRESENCE) = Some(Box::new(reborn_core::presence::NoPresence));
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_room_view_says_whether_you_host_it() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r", "code": "K7QP2M", "mode": "casual", "you_host": true, "members": [] } })).unwrap();
        assert_eq!(snapshot_json(&s)["room"]["you_host"], true);
    }

    use super::*;

    #[test]
    fn exit_flags_are_spent_only_by_their_own_match_and_a_new_match_forgets_old_ones() {
        // One test owns both statics (tests run in parallel).
        *lock(&SERIES_CLOSING) = Some("m-old".into());
        *lock(&PEER_LOST) = Some("m-old".into());
        assert_eq!(spend_exit_flags("m-new"), (false, false), "another match's flags never apply");
        assert_eq!(lock(&SERIES_CLOSING).as_deref(), Some("m-old"), "and are not spent by it");
        assert_eq!(spend_exit_flags("m-old"), (true, true));
        assert_eq!(*lock(&SERIES_CLOSING), None);
        assert_eq!(*lock(&PEER_LOST), None);
        assert_eq!(spend_exit_flags("m-old"), (false, false), "spent once");

        // A rematch (new match id) clears a flag a failed close left behind.
        *lock(&SERIES_CLOSING) = Some("m-old".into());
        *lock(&PEER_LOST) = Some("m-old".into());
        forget_other_matches("m-old");
        assert_eq!(lock(&SERIES_CLOSING).as_deref(), Some("m-old"), "the current match keeps its flags");
        forget_other_matches("m-rematch");
        assert_eq!(*lock(&SERIES_CLOSING), None);
        assert_eq!(*lock(&PEER_LOST), None);
    }

    #[test]
    fn page_names_are_the_known_set_and_anything_else_is_home() {
        assert_eq!(page_label("mods"), "Mods");
        assert_eq!(page_label("Settings"), "Settings");
        // An unknown page must not leak whatever string the UI passed into Discord.
        assert_eq!(page_label("C:\\Users\\Adam\\secret"), "Home");
        assert_eq!(page_label(""), "Home");
    }

    #[test]
    fn modes_outside_the_two_are_refused_with_a_sentence() {
        assert_eq!(parse_mode("casual"), Ok(Mode::Casual));
        assert_eq!(parse_mode("ranked"), Ok(Mode::Ranked));
        assert!(parse_mode("hardcore").unwrap_err().contains("casual or ranked"));
    }

    #[test]
    fn presence_can_be_switched_off_and_on_without_discord() {
        use_no_presence();
        presence_set_enabled(false);
        assert!(!lock(&STATE).presence_on);
        presence_set_enabled(true);
        assert!(lock(&STATE).presence_on);
    }

    #[test]
    fn the_match_view_carries_the_opponents_standing_only_when_the_server_sends_it() {
        let with: Snapshot = serde_json::from_value(serde_json::json!({ "match": { "id": "m", "relay_state": "pairing",
            "opponent": { "id": "o", "display_name": "Jock", "rating": 1162, "rank": 9,
                "team": { "name": "Falcons", "logo_url": "/live-assets/teams/14-128.png" }, "avatar_url": "https://cdn.discordapp.com/avatars/1/a.png" } } }))
            .unwrap();
        let v = snapshot_json(&with);
        assert_eq!(v["match"]["opponent"], "Jock", "the name keeps its old key");
        assert_eq!(v["match"]["opponent_elo"], 1162);
        assert_eq!(v["match"]["opponent_rank"], 9);
        assert_eq!(v["match"]["opponent_team"]["name"], "Falcons");
        assert_eq!(v["match"]["opponent_avatar_url"], "https://cdn.discordapp.com/avatars/1/a.png");

        let without: Snapshot = serde_json::from_value(serde_json::json!({ "match": { "id": "m", "opponent": { "id": "o", "display_name": "Jock", "rating": null } } })).unwrap();
        let v = snapshot_json(&without);
        for k in ["opponent_elo", "opponent_rank", "opponent_team", "opponent_avatar_url"] {
            assert!(v["match"][k].is_null(), "{k} must be null when the server sends nothing");
        }
    }
}
