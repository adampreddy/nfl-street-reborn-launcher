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

use reborn_core::matchmaking::{activity_for, Chat, ChatMessage, Client, MatchAction, MatchView, MmError, RoomAction, Snapshot};
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
    /// The conversation so far in the current room or match. The long poll carries only the lines after the
    /// launcher's chat cursor, so the log is built up here; showing each poll's delta as the whole log made every
    /// earlier line vanish on the next change (2026-10-03, "the chat doesn't work at all").
    chat_log: ChatLog,
}

static STATE: Mutex<State> = Mutex::new(State {
    link: Link::Starting,
    snapshot: None,
    last_error: None,
    page: "Home",
    presence_on: true,
    relay_error: None,
    chat_log: ChatLog::new(),
});

/// The most lines kept per room or match (the page draws the last 30).
const CHAT_KEEP: usize = 200;

/// One room's or match's chat, accumulated from the long poll's deltas. Seqs count per parent (snapshot.js Q4), so a
/// new parent (another room, the room's match starting) starts a new log, and a delta the server re-sent from 0
/// replaces it.
#[derive(Debug, PartialEq)]
struct ChatLog {
    /// `(scope, parent id)`: `room`/`match` and its id. None when there is nothing to chat in.
    parent: Option<(String, String)>,
    lines: Vec<ChatMessage>,
}

impl ChatLog {
    const fn new() -> Self {
        ChatLog { parent: None, lines: Vec::new() }
    }

    /// Takes in one changed snapshot's chat. Returns true when it was a delta from the middle of a conversation this
    /// log never read (a new parent with `since > 0`): the caller then asks again from 0, so no line is skipped.
    fn absorb(&mut self, chat: &Chat) -> bool {
        let Some(scope) = chat.scope.clone() else {
            self.clear();
            return false;
        };
        let key = Some((scope, chat.parent_id.clone().unwrap_or_default()));
        let mut gap = false;
        if key != self.parent {
            self.parent = key;
            self.lines.clear();
            gap = chat.since > 0;
        } else if chat.since == 0 {
            // The whole conversation again (a server restart, a cursor it clamped): it replaces what is held.
            self.lines.clear();
        }
        for m in &chat.messages {
            if self.lines.last().map_or(true, |l| m.seq > l.seq) {
                self.lines.push(m.clone());
            }
        }
        if self.lines.len() > CHAT_KEEP {
            self.lines.drain(..self.lines.len() - CHAT_KEEP);
        }
        gap
    }

    fn clear(&mut self) {
        self.parent = None;
        self.lines.clear();
    }
}

/// What the relay opens Dolphin with: the runtime, game and graphics preset the page's own Play and
/// private-room paths use (they live in the page's storage, so the page reports them).
static LAUNCH: Mutex<(Option<String>, Option<String>, String)> = Mutex::new((None, None, String::new()));

static PRESENCE: Mutex<Option<Box<dyn Presence + Send>>> = Mutex::new(None);
static POLLER: AtomicBool = AtomicBool::new(false);
/// This launcher's own tally for the match it is streaming: (match id, [seat0, seat1] wins, ms when it was read).
/// The live loop writes it every second (including the plays it has not sent yet), the relay loop reads it for the
/// ranked close, and the exit report waits for a reading taken after Dolphin exited.
static LOCAL_WINS: Mutex<Option<(String, [u32; 2], u64)>> = Mutex::new(None);
/// The match whose Dolphin the launcher is closing on purpose because the series is decided. Its exit is not a
/// leave (Task 19 reads this).
pub(crate) static SERIES_CLOSING: Mutex<Option<String>> = Mutex::new(None);
/// The game being played right now, by seat: (match id, [seat0, seat1]). The live loop writes it from the plays it
/// has read; Discord presence shows it as "you-them" (Adam 10-02).
static LIVE_SCORE: Mutex<Option<(String, [i64; 2])>> = Mutex::new(None);
/// The room lost its other player while this PC's Dolphin was still running: (match id, first sighting). Set by the
/// relay loop, cleared when the room shows both players again (final fix L-I1), read when Dolphin exits (ruling R1).
static PEER_LOST: Mutex<Option<(String, reborn_core::series::PeerSighting)>> = Mutex::new(None);
/// How long the exit report waits for the live loop to read the plays Dolphin wrote before it exited (review I1).
const EXIT_GRACE_MS: u64 = 5_000;
const EXIT_POLL_MS: u64 = 250;

/// The local tally for `match_id` and when it was read; no reading yet is 0-0 read at 0.
fn local_wins(match_id: &str) -> ([u32; 2], u64) {
    lock(&LOCAL_WINS).as_ref().filter(|(id, _, _)| id == match_id).map(|(_, w, at)| (*w, *at)).unwrap_or(([0, 0], 0))
}

/// The room of `match_id` was seen without its peer at `now`; `alone`: only our own player was left in it. The first
/// sighting's time is kept, and an "alone" reading upgrades a blank one (never the other way: our own teardown blanks
/// the room after the peer left).
fn note_peer_lost(match_id: &str, now: u64, alone: bool) {
    let mut g = lock(&PEER_LOST);
    match g.as_mut() {
        Some((id, s)) if id == match_id => { s.alone |= alone; }
        _ => { *g = Some((match_id.to_string(), reborn_core::series::PeerSighting { since_ms: now, alone })); }
    }
}

/// The room of `match_id` shows both players again: an earlier sighting was a glitch, not a lost peer.
fn clear_peer_lost(match_id: &str) {
    let mut g = lock(&PEER_LOST);
    if g.as_ref().is_some_and(|(id, _)| id == match_id) { *g = None; }
}

/// Take SERIES_CLOSING and PEER_LOST if (and only if) they name `match_id`: (closing, peer lost first seen at). A
/// flag naming another match is left for [`forget_other_matches`].
fn spend_exit_flags(match_id: &str) -> (bool, Option<reborn_core::series::PeerSighting>) {
    let closing = {
        let mut g = lock(&SERIES_CLOSING);
        if g.as_deref() == Some(match_id) { g.take(); true } else { false }
    };
    let peer = {
        let mut g = lock(&PEER_LOST);
        if g.as_ref().is_some_and(|(id, _)| id == match_id) { g.take().map(|(_, s)| s) } else { None }
    };
    (closing, peer)
}

/// A different match is current (a rematch, a new queue match): flags left by an older one are dropped, so a close
/// that never made Dolphin exit cannot silence the next match's real leave.
fn forget_other_matches(current: &str) {
    {
        let mut g = lock(&SERIES_CLOSING);
        if g.as_deref().is_some_and(|id| id != current) { *g = None; }
    }
    let mut g = lock(&PEER_LOST);
    if g.as_ref().is_some_and(|(id, _)| id != current) { *g = None; }
}

/// Look until a look says it is final or `grace_ms` has passed, sleeping between looks; the last look's value.
fn settle<T>(grace_ms: u64, now: &mut dyn FnMut() -> u64, sleep: &mut dyn FnMut(), mut look: impl FnMut() -> (T, bool)) -> T {
    let deadline = now().saturating_add(grace_ms);
    loop {
        let (value, last) = look();
        if last || now() >= deadline { return value; }
        sleep();
    }
}

fn series_running(m: &MatchView) -> bool {
    m.status == "running" && matches!(m.relay_state.as_str(), "running" | "intermission")
}

/// The NetPlay Dolphin exited (dolphin_settings' exit thread, on a thread of its own so filing the captures never
/// waits on this request, R-P9). Mid-series, tell the server why (spec §3.4, R13). Nothing is reported while the
/// launcher closes a decided series, nor when it is already decided but the close had not fired yet (I1).
/// `session_match` is the match id this Dolphin session started with (its SessionIntent): the report is about that
/// match only, never whichever match is current now (final fix L-I3), and never to a backend without series (L-I2).
pub fn dolphin_exited(exit: Option<&reborn_core::platform::ExitKind>, session_match: Option<String>) {
    let exited_at = now_ms();
    let matched = || lock(&STATE).snapshot.as_ref().and_then(|s| s.matched.clone());
    let Some(first) = matched() else { return };
    let (closing, sighting) = spend_exit_flags(&first.id);
    let peer_lost = reborn_core::series::peer_lost_before_exit(sighting, exited_at);
    if !series_running(&first) { return; }
    if !reborn_core::series::exit_report_applies(&first, session_match.as_deref()) {
        mm_log(&format!("dolphin exited ({exit:?}): not reported (session match {session_match:?}, current {}, outcome {})",
            first.id, if first.outcome.is_some() { "published" } else { "absent: older backend" }));
        return;
    }
    if closing {
        mm_log(&format!("dolphin exited while the launcher closed the decided series ({exit:?}): not reported"));
        return;
    }
    // Review I1: the deciding play may still be unread (written a moment before the exit, or the poster backing off),
    // and the server may still lack its second copy. Give the live loop a few seconds to read what Dolphin wrote,
    // re-reading the tally and the snapshot, before a winner's close could be reported as a leave.
    let mut now = now_ms;
    let mut sleep = || std::thread::sleep(Duration::from_millis(EXIT_POLL_MS));
    let (m, decided) = settle(EXIT_GRACE_MS, &mut now, &mut sleep, || {
        let m = matched().filter(|m| m.id == first.id).unwrap_or_else(|| first.clone());
        let (local, read_at) = local_wins(&m.id);
        let decided = reborn_core::series::decided_for_exit(
            reborn_core::series::closes_at_decider(&m), local, reborn_core::series::wins_needed(&m), m.outcome.as_ref().map(|o| o.kind.as_str()));
        let last = decided || !series_running(&m) || read_at >= exited_at;
        ((m, decided), last)
    });
    let action = match reborn_core::series::exit_report(exit, series_running(&m), decided, peer_lost) {
        reborn_core::series::ExitReport::Nothing => {
            if decided { mm_log(&format!("dolphin exited after the series was decided ({exit:?}): not reported")); }
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
        let from_mm = s.snapshot.as_ref().and_then(|snap| activity_for(snap, now_unix())).map(|a| match a {
            // The live score, from this launcher's own copy of the plays, for the match being played.
            Activity::InMatch { mode, opponent, field, series, score: None, start_unix } => {
                let m = s.snapshot.as_ref().and_then(|snap| snap.matched.as_ref());
                let score = m.and_then(|m| {
                    let seat = (m.seat as usize).min(1);
                    lock(&LIVE_SCORE).as_ref().filter(|(id, _)| id == &m.id)
                        .map(|(_, by)| (by[seat].max(0) as u32, by[1 - seat].max(0) as u32))
                });
                Activity::InMatch { mode, opponent, field, series, score, start_unix }
            }
            other => other,
        });
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
                        // The last reading, taken now: the exit report waits for one from after Dolphin exited.
                        if let Some((id, p)) = current.as_ref() {
                            *lock(&LOCAL_WINS) = Some((id.clone(), p.projected_wins(), now_ms()));
                        }
                        mm_log("live feed: match over, stopped");
                        current = None;
                        ended_at = None;
                        return;
                    }
                }
            }
            let Some((id, poster)) = current.as_mut() else { return };
            // Every second, unsent plays included (review I1): the series close and the exit report read it.
            let read_at = now_ms();
            *lock(&LOCAL_WINS) = Some((id.clone(), poster.projected_wins(), read_at));
            if let Some(by) = poster.tally().current_score_by_seat() {
                let changed = lock(&LIVE_SCORE).as_ref().is_none_or(|(i, b)| i != id || *b != by);
                if changed {
                    *lock(&LIVE_SCORE) = Some((id.clone(), by));
                    refresh_presence();
                }
            }
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
    // The match whose room this PC has seen running with both players (alone in a lobby after that is a lost peer).
    let mut played_together: (String, bool) = (String::new(), false);
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
        // R1 (review I2): did the room lose the other player, mid-game, between games, or by closing/erroring under
        // us, before our own Dolphin went away?
        if let Some(m) = matched.as_ref() {
            forget_other_matches(&m.id);
            if played_together.0 != m.id { played_together = (m.id.clone(), false); }
            if room.phase == "running" && room.players.len() >= 2 { played_together.1 = true; }
            // Final fix L-I1: both players back clears an earlier sighting; only us left counts at once; a blank room
            // (closed, error, empty) only when seen well before our own exit (peer_lost_before_exit).
            match reborn_core::series::peer_reading(&m.relay_state, &room.phase, room.players.len(), room.active, played_together.1) {
                reborn_core::series::PeerReading::Present => clear_peer_lost(&m.id),
                reborn_core::series::PeerReading::Alone => note_peer_lost(&m.id, now_ms(), true),
                reborn_core::series::PeerReading::Blank => note_peer_lost(&m.id, now_ms(), false),
                reborn_core::series::PeerReading::Unknown => {}
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
        // The series close (series spec §3.2; ruling R11): at the deciding win, wait for the deciding capture (30 s at
        // most), then close the room ~3 s later. Each launcher does this on its own. Ranked, and any other series the
        // server says is a best of more than one (casual is a best of 3 too, Adam 10-04).
        if let Some(m) = matched.as_ref().filter(|m| reborn_core::series::closes_at_decider(m)) {
            if close.0 != m.id { close = (m.id.clone(), Default::default()); }
            let (local, _) = local_wins(&m.id);
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
    let edition = m.and_then(edition_of);
    match step {
        Step::Ready => post(MatchAction::Ready {
            ready: true,
            // The seed the SERVER named for this match (regular = quick game, pickup; a ranked match's regular game
            // boots ranked.s01; a league room league.s01, or none when raw), so both peers hash the same file and boot
            // from it.
            preflight: crate::dolphin_settings::mm_preflight(runtime.as_deref(), game.as_deref(), m.map(|m| seed_for_match(m)), edition.as_deref()),
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
                m.map(|m| seed_for_match(m).to_string()),
                edition,
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
                // The chat cursor follows what was actually delivered: a capped delta (`truncated`) ends before the
                // parent's latest seq, so the cursor stops at its last line and the next poll (asked at once) carries on.
                let gap = lock(&STATE).chat_log.absorb(&snap.chat);
                chat = match snap.chat.messages.last() {
                    Some(last) if snap.chat.truncated => last.seq,
                    _ => snap.chat.latest_seq,
                };
                if snap.chat.truncated {
                    since = 0;
                }
                if gap {
                    // A new room or match read from an old cursor: ask again from the start of its conversation.
                    mm_log("poll: chat moved to a new room or match mid-cursor, re-reading it");
                    chat = 0;
                    since = 0;
                }
                let wait = if snap.restarting || snap.chat.truncated || gap { 250 } else { snap.poll_after_ms.min(10_000) };
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
                // Whoever signs in next starts with no cursor and no conversation.
                chat = 0;
                lock(&STATE).chat_log.clear();
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

fn with_client_mm<T>(f: impl FnOnce(&Client) -> Result<T, MmError>) -> Result<T, MmError> {
    let p = Platform::detect();
    let token = reborn_core::identity::token(&p).map_err(|e| MmError::Offline(e.to_string()))?.ok_or(MmError::SignedOut)?;
    let origin = reborn_core::identity::origin(&p).map_err(|e| MmError::Offline(e.to_string()))?;
    let http = crate::http_client();
    let client = Client::new(&http, origin, token);
    f(&client)
}

fn with_client<T>(f: impl FnOnce(&Client) -> Result<T, MmError>) -> Result<T, String> {
    with_client_mm(f).map_err(|e| e.to_string())
}

/// A join's rejection: the sentence it always was, or, for a league version refusal, the shape Plan 3a matches on.
pub(crate) fn join_error(e: MmError) -> serde_json::Value {
    match e {
        MmError::LeagueUpdateRequired { required } => serde_json::json!({
            "kind": "league_update_required", "message": reborn_core::matchmaking::LEAGUE_UPDATE_SENTENCE, "required": required
        }),
        other => serde_json::Value::String(other.to_string()),
    }
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
/// None for the 3rd and 4th player of a private room too: they have no seat in the series, so their results are
/// filed like a Direct connection game's and their Dolphin's exit reports nothing (they press Leave match to go).
pub fn current_match_id() -> Option<String> {
    let s = lock(&STATE);
    s.snapshot.as_ref()?.matched.as_ref()
        .filter(|m| !matches!(m.relay_state.as_str(), "complete" | "void" | "abandoned" | "held"))
        .filter(|m| !m.extra_player())
        .map(|m| m.id.clone())
}

/// The seed a match boots from. A League edition match plays the league seed unless its ruleset is `raw`; otherwise
/// the retail rule: a ranked ruleset (or, from an older server, ranked mode) boots ranked.s01, else the server's kind.
/// The backend names retail rooms `"retail"`, so only `"league"` is the League edition.
pub(crate) fn seed_for_match(m: &MatchView) -> &'static str {
    if m.edition.as_deref().map(str::trim) == Some("league") {
        return if m.ruleset.as_deref().map(str::trim) == Some("raw") { "raw" } else { "league" };
    }
    let by = if m.ruleset.as_deref().map(str::trim) == Some("ranked") { "ranked" } else { m.mode.as_str() };
    crate::dolphin_settings::match_seed_kind(m.seed_kind.as_deref(), Some(by))
}

/// `Some("league")` for a League edition match, else `None` (retail, whether the server said `"retail"` or nothing).
pub(crate) fn edition_of(m: &MatchView) -> Option<String> {
    m.edition.as_deref().map(str::trim).filter(|e| *e == "league").map(str::to_string)
}

/// The running match's league fixture, when it is a league game: its results then go up with `intent.league = true`
/// and the fixture, which forward.js needs. A league friendly has no fixture and is not a league game.
pub fn current_match_league() -> Option<u64> {
    let s = lock(&STATE);
    let m = s.snapshot.as_ref()?.matched.as_ref()?;
    if edition_of(m).is_none() {
        return None;
    }
    m.league.as_ref()?.get("fixture_id")?.as_u64()
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
        "snapshot": s.snapshot.as_ref().map(|snap| snapshot_json(snap, &s.chat_log.lines)),
    })
}

/// The snapshot as the page wants it: the fields it draws, not the wire's whole shape. `chat` is the conversation
/// accumulated so far (`ChatLog`), never the one poll's delta the snapshot carries.
///
/// Every `name` is the name the player is SHOWN by (`shown_name`: their nickname, else their Discord name), and
/// `discord` is their Discord name beside it when they set a nickname (null otherwise), for a subtitle or tooltip.
/// Every player also carries their account id (`player_id`, the match's `opponent_id`; null when the server sent
/// none), so the page can open their profile from their name (10-08).
fn snapshot_json(s: &Snapshot, chat: &[ChatMessage]) -> serde_json::Value {
    use reborn_core::matchmaking::{discord_aside, shown_name};
    let id = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty());
    serde_json::json!({
        "player": s.player.as_ref().map(|p| serde_json::json!({
            "player_id": id(&p.id),
            "name": shown_name(p.nickname.as_deref(), &p.display_name),
            "discord": discord_aside(p.nickname.as_deref(), &p.display_name),
            "region": p.region,
        })),
        "ticket": s.ticket.as_ref().map(|t| serde_json::json!({
            "mode": t.mode, "field": t.field_pick, "waited_ms": t.waited_ms, "enqueued_at": t.enqueued_at,
            // Casual's Quick game or Pickup (null from a server that does not say): the searching bar names it.
            "seed_kind": t.seed_kind,
        })),
        "room": s.room.as_ref().map(|r| serde_json::json!({
            "id": r.id, "code": r.code, "mode": r.mode, "best_of": r.best_of, "status": r.status, "you_host": r.you_host,
            // 4 for a 2v2 room, 2 for a room an older launcher opened (or an older server: it sends 0).
            "capacity": r.capacity.max(2),
            // The League edition (Plan 3b): the page names a league room by these, never by guessing from the code.
            "edition": r.edition, "ruleset": r.ruleset, "league": r.league, "league_game_version": r.league_game_version,
            "seed_kind": crate::dolphin_settings::seed_kind_or_default(r.seed_kind.as_deref()),
            // `you` is decided HERE, from the signed-in player's id, so the page never has to guess
            // which member it is. Member order is the server's and carries no meaning.
            "members": r.members.iter().map(|m| serde_json::json!({
                "player_id": id(&m.player_id),
                "name": shown_name(m.nickname.as_deref(), &m.display_name),
                "discord": discord_aside(m.nickname.as_deref(), &m.display_name),
                "ready": m.ready, "field": m.field_pick,
                "you": s.player.as_ref().is_some_and(|p| !p.id.is_empty() && p.id == m.player_id),
            })).collect::<Vec<_>>(),
        })),
        "match": s.matched.as_ref().map(match_json),
        "chat": chat.iter().map(|c| serde_json::json!({
            // The line under both names: `body` (what the page always read) and `text` (the server's own name).
            "seq": c.seq, "player_id": id(&c.player_id), "name": shown_name(c.nickname.as_deref(), &c.display_name),
            "discord": discord_aside(c.nickname.as_deref(), &c.display_name), "body": c.body, "text": c.body, "at": c.at,
        })).collect::<Vec<_>>(),
        "queue_depth": { "ranked": s.queue_depth.ranked, "casual": s.queue_depth.casual },
        "online": { "count": s.online.count, "players": s.online.players.iter().map(|p| serde_json::json!({
            "player_id": id(&p.id),
            "name": shown_name(p.nickname.as_deref(), &p.display_name),
            "discord": discord_aside(p.nickname.as_deref(), &p.display_name), "activity": p.activity,
        })).collect::<Vec<_>>() },
    })
}


/// The match as `snapshot_json` gives it to the page (its own function: one `json!` this size is past the macro's
/// recursion limit).
fn match_json(m: &MatchView) -> serde_json::Value {
    use reborn_core::matchmaking::{discord_aside, shown_name};
    let id = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty());
    serde_json::json!({
        "id": m.id, "mode": m.mode, "status": m.status, "relay_state": m.relay_state,
        "relay_deadline": m.relay_deadline, "best_of": m.best_of, "you_host": m.you_host,
        "opponent": shown_name(m.opponent.nickname.as_deref(), &m.opponent.display_name),
        "opponent_discord": discord_aside(m.opponent.nickname.as_deref(), &m.opponent.display_name),
        "opponent_id": id(&m.opponent.id),
        "field": m.field.as_deref().map(reborn_core::matchmaking::field_title),
        // Display only (spec 2026-10-02 §3.3): null until the server sends them, and the page then hides each spot.
        "opponent_elo": m.opponent.rating, "opponent_rank": m.opponent.rank,
        "opponent_team": m.opponent.team, "opponent_avatar_url": m.opponent.avatar_url,
        "your_field_pick": m.your_field_pick, "your_accepted": m.your_accepted, "your_ready": m.your_ready,
        "series": m.series, "wins_needed": m.wins_needed, "winner_seat": m.winner_seat,
        "hold_reason": m.hold_reason, "seat": m.seat,
        "edition": m.edition, "ruleset": m.ruleset, "league": m.league,
        // Adam 10-09: the series-over screen offers Rematch only when the server does (true from an older server).
        "rematch_available": m.rematch_available,
        "seed_kind": crate::dolphin_settings::seed_kind_or_default(m.seed_kind.as_deref()),
        // The series-over screen (series spec 2026-10-02 §3.3): null/empty from a server before the series build.
        "outcome": m.outcome, "series_live": m.series_live, "rules": m.rules,
        // A 3-4 player room's match: everyone in it, by seat ([] for two players). Names only; tags stay here.
        "party": m.party.iter().filter(|p| p.active).map(|p| serde_json::json!({
            "seat": p.seat, "you": p.you,
            "name": Some(shown_name(p.nickname.as_deref(), p.display_name.as_deref().unwrap_or(""))).filter(|n| !n.is_empty()),
            "discord": discord_aside(p.nickname.as_deref(), p.display_name.as_deref().unwrap_or("")),
        })).collect::<Vec<_>>(),
    })
}

/// `seed_kind`: casual's Quick game (`regular`) or Pickup. Ranked ignores it (`Client::enqueue` never sends one).
#[tauri::command]
pub async fn mm_queue(mode: String, seed_kind: Option<String>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mode = parse_mode(&mode)?;
        let r = with_client(|c| c.enqueue(mode, None, seed_kind.as_deref()))?;
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
pub async fn mm_room_create(mode: String, seed_kind: Option<String>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mode = parse_mode(&mode)?;
        // A room is joined by its code only (no public lobbies, 2026-10-03). Host a room sends no kind: a Quick game
        // (`regular`) room, which the host can switch to Pickup from the room (`mm_room_action` `kind`).
        let seed_kind = crate::dolphin_settings::seed_kind_or_default(seed_kind.as_deref());
        with_client(|c| c.create_room(mode, seed_kind))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mm_room_join(code: String) -> Result<serde_json::Value, serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || {
        // The installed league build goes with every join: a league room refuses a mismatched one before anyone boots
        // (league_update_required); a retail room ignores it. A launcher without league files sends nothing.
        let version = crate::league_edition::installed_league_version();
        with_client_mm(|c| c.join_room_as(code.trim(), version.as_deref())).map_err(join_error)
    })
    .await
    .map_err(|e| serde_json::Value::String(e.to_string()))?
}

/// Who invited the player to a room (spec 2026-10-02 §3.10): `{ host_name, mode }`, or null when the server does not
/// say. Display only; joining is still `mm_room_join`.
#[tauri::command]
pub async fn mm_room_lookup(code: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let found = with_client(|c| c.room_lookup(code.trim()))?;
        Ok(found.map(|r| serde_json::json!({ "host_name": r.host_name, "mode": r.mode })).unwrap_or(serde_json::Value::Null))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mm_room_action(room_id: String, action: String, value: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let a = match action.as_str() {
            "leave" => RoomAction::Leave,
            "ready" => RoomAction::Ready(value.as_ref().and_then(|v| v.as_bool()).unwrap_or(true)),
            "chat" => RoomAction::Chat(value.as_ref().and_then(|v| v.as_str()).unwrap_or_default().to_string()),
            "start" => RoomAction::Start,
            "kind" => RoomAction::Kind(value.as_ref().and_then(|v| v.as_str()).unwrap_or_default().to_string()),
            _ => return Err("That isn't something a room can do.".to_string()),
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
        assert_eq!(snapshot_json(&s, &[])["room"]["you_host"], true);
    }

    use super::*;

    #[test]
    fn every_name_on_the_page_is_the_nickname_with_the_discord_name_beside_it() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({
            "player": { "id": "me", "display_name": "adam_discord", "nickname": "Slick", "name": "Slick" },
            "room": { "id": "r", "code": "K7QP2M", "members": [
                { "player_id": "me", "display_name": "adam_discord", "nickname": "Slick" },
                { "player_id": "o", "display_name": "Jock" } ] },
            "match": { "id": "m", "opponent": { "id": "o", "display_name": "jock_discord", "nickname": "Jock", "name": "Jock" },
                "party": [ { "seat": 0, "display_name": "h_discord", "nickname": "Host", "nsr_tag": "nsr:0", "active": true } ] },
            "online": { "count": 1, "players": [ { "id": "o", "display_name": "jock_discord", "nickname": "Jock", "activity": "idle" } ] },
            "chat": { "messages": [ { "seq": 1, "player_id": "o", "display_name": "jock_discord", "nickname": "Jock", "text": "gg" } ] } })).unwrap();
        let chat = s.chat.messages.clone();
        let v = snapshot_json(&s, &chat);
        assert_eq!((v["player"]["name"].as_str(), v["player"]["discord"].as_str()), (Some("Slick"), Some("adam_discord")));
        assert_eq!(v["room"]["members"][0]["name"], "Slick");
        assert_eq!(v["room"]["members"][0]["discord"], "adam_discord");
        assert_eq!(v["room"]["members"][1]["name"], "Jock");
        assert!(v["room"]["members"][1]["discord"].is_null(), "no nickname: nothing beside the Discord name");
        assert_eq!((v["match"]["opponent"].as_str(), v["match"]["opponent_discord"].as_str()), (Some("Jock"), Some("jock_discord")));
        assert_eq!(v["match"]["party"][0]["name"], "Host");
        assert_eq!(v["online"]["players"][0]["name"], "Jock");
        assert_eq!((v["chat"][0]["name"].as_str(), v["chat"][0]["discord"].as_str()), (Some("Jock"), Some("jock_discord")));
    }

    #[test]
    fn every_player_on_the_page_carries_their_account_id_for_their_profile() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({
            "player": { "id": "me", "display_name": "Adam" },
            "room": { "id": "r", "code": "K7QP2M", "members": [ { "player_id": "me", "display_name": "Adam" }, { "display_name": "NoId" } ] },
            "match": { "id": "m", "opponent": { "id": "o", "display_name": "Jock" } },
            "online": { "count": 1, "players": [ { "id": "o", "display_name": "Jock", "activity": "idle" } ] },
            "chat": { "messages": [ { "seq": 1, "player_id": "o", "display_name": "Jock", "text": "gg" } ] } })).unwrap();
        let chat = s.chat.messages.clone();
        let v = snapshot_json(&s, &chat);
        assert_eq!(v["player"]["player_id"], "me");
        assert_eq!(v["room"]["members"][0]["player_id"], "me");
        assert!(v["room"]["members"][1]["player_id"].is_null(), "no id from the server: null, never an empty string");
        assert_eq!(v["match"]["opponent_id"], "o");
        assert_eq!(v["online"]["players"][0]["player_id"], "o");
        assert_eq!(v["chat"][0]["player_id"], "o");
    }

    #[test]
    fn a_server_without_nicknames_reads_exactly_as_before() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({
            "player": { "id": "me", "display_name": "Adam" },
            "room": { "id": "r", "code": "K7QP2M", "members": [ { "player_id": "o", "display_name": "Jock" } ] },
            "match": { "id": "m", "opponent": { "id": "o", "display_name": "Jock" } } })).unwrap();
        assert_eq!(s.player.as_ref().unwrap().nickname, None);
        assert_eq!(s.matched.as_ref().unwrap().opponent.nickname, None);
        let v = snapshot_json(&s, &[]);
        assert_eq!(v["player"]["name"], "Adam");
        assert!(v["player"]["discord"].is_null());
        assert_eq!(v["room"]["members"][0]["name"], "Jock");
        assert_eq!(v["match"]["opponent"], "Jock");
        assert!(v["match"]["opponent_discord"].is_null());
    }

    #[test]
    fn the_room_and_match_views_name_their_game_kind_and_default_to_a_regular_game() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({
            "room": { "id": "r", "code": "K7QP2M", "seed_kind": "pickup", "members": [] },
            "match": { "id": "m", "seed_kind": "pickup", "opponent": { "id": "o", "display_name": "Jock" } } })).unwrap();
        let v = snapshot_json(&s, &[]);
        assert_eq!(v["room"]["seed_kind"], "pickup");
        assert_eq!(v["match"]["seed_kind"], "pickup");
        let s: Snapshot = serde_json::from_value(serde_json::json!({
            "room": { "id": "r", "code": "K7QP2M", "members": [] },
            "match": { "id": "m", "opponent": { "id": "o", "display_name": "Jock" } } })).unwrap();
        let v = snapshot_json(&s, &[]);
        assert_eq!(v["room"]["seed_kind"], "regular", "an older server sends none: a regular game");
        assert_eq!(v["match"]["seed_kind"], "regular");
    }

    fn delta(scope: &str, parent: &str, since: u64, lines: &[(u64, &str)]) -> Chat {
        serde_json::from_value(serde_json::json!({ "scope": scope, "parent_id": parent, "since": since,
            "latest_seq": lines.last().map_or(since, |l| l.0), "messages": lines.iter().map(|(seq, t)| serde_json::json!({
                "seq": seq, "player_id": "p", "display_name": "Jock", "text": t, "at": "2026-10-03T12:00:00.000Z" })).collect::<Vec<_>>() }))
        .unwrap()
    }

    fn said(log: &ChatLog) -> Vec<&str> {
        log.lines.iter().map(|m| m.body.as_str()).collect()
    }

    #[test]
    fn the_chat_log_keeps_every_line_across_polls_and_starts_over_for_a_new_room_or_match() {
        let mut log = ChatLog::new();
        assert!(!log.absorb(&delta("room", "r1", 0, &[(1, "yo"), (2, "ready?")])));
        // The next changed poll carries only the line after the cursor (or none: a ready toggle). Nothing is lost.
        assert!(!log.absorb(&delta("room", "r1", 2, &[(3, "go")])));
        assert!(!log.absorb(&delta("room", "r1", 3, &[])));
        assert_eq!(said(&log), vec!["yo", "ready?", "go"]);
        // A line seen twice (a re-sent delta) is kept once.
        log.absorb(&delta("room", "r1", 2, &[(3, "go")]));
        assert_eq!(said(&log), vec!["yo", "ready?", "go"]);
        // The server re-sent the whole conversation (since 0): it replaces the log.
        log.absorb(&delta("room", "r1", 0, &[(1, "yo"), (2, "ready?"), (3, "go"), (4, "gl")]));
        assert_eq!(said(&log), vec!["yo", "ready?", "go", "gl"]);
        // The match starts: a new parent, a new conversation.
        assert!(!log.absorb(&delta("match", "m1", 0, &[(1, "gg")])));
        assert_eq!(said(&log), vec!["gg"]);
        // A new parent read from the middle (an old cursor the server did not clamp): flagged so the poller re-reads.
        assert!(log.absorb(&delta("room", "r2", 1, &[(2, "late")])));
        // Nothing to chat in: the log is empty.
        log.absorb(&Chat::default());
        assert!(log.lines.is_empty() && log.parent.is_none());
    }

    #[test]
    fn the_chat_log_keeps_the_newest_lines_when_it_is_long() {
        let mut log = ChatLog::new();
        let lines: Vec<(u64, String)> = (1..=250).map(|i| (i, format!("line {i}"))).collect();
        let refs: Vec<(u64, &str)> = lines.iter().map(|(i, t)| (*i, t.as_str())).collect();
        log.absorb(&delta("room", "r1", 0, &refs));
        assert_eq!(log.lines.len(), CHAT_KEEP);
        assert_eq!(log.lines.last().unwrap().body, "line 250");
        assert_eq!(log.lines.first().unwrap().body, "line 51");
    }

    #[test]
    fn the_page_gets_the_whole_conversation_not_the_last_delta() {
        let mut log = ChatLog::new();
        log.absorb(&delta("room", "r1", 0, &[(1, "yo")]));
        log.absorb(&delta("room", "r1", 1, &[(2, "go")]));
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r1", "code": "K7QP2M", "members": [] },
            "chat": { "scope": "room", "parent_id": "r1", "since": 1, "latest_seq": 2, "messages": [
                { "seq": 2, "display_name": "Jock", "text": "go" } ] } })).unwrap();
        let v = snapshot_json(&s, &log.lines);
        let bodies: Vec<&str> = v["chat"].as_array().unwrap().iter().map(|c| c["body"].as_str().unwrap()).collect();
        assert_eq!(bodies, vec!["yo", "go"]);
        assert_eq!(v["chat"][0]["name"], "Jock");
    }

    #[test]
    fn the_exit_grace_stops_at_the_first_final_look_and_never_runs_past_its_bound() {
        use std::cell::Cell;
        let clock = Cell::new(1_000u64);
        let mut now = || clock.get();
        let mut sleep = || clock.set(clock.get() + 250);
        // Final on the third look: two sleeps, the third value.
        let mut looks = 0;
        let got = settle(5_000, &mut now, &mut sleep, || { looks += 1; (looks, looks == 3) });
        assert_eq!((got, looks), (3, 3));
        // Never final: bounded by the grace (5 s of 250 ms sleeps = 21 looks), returns the last look.
        clock.set(1_000);
        let mut looks = 0;
        let got = settle(5_000, &mut now, &mut sleep, || { looks += 1; (looks, false) });
        assert_eq!(got, 21);
        assert_eq!(clock.get(), 6_000);
        // Final at once: one look, no sleep.
        let mut looks = 0;
        let got = settle(5_000, &mut now, &mut sleep, || { looks += 1; (looks, true) });
        assert_eq!((got, clock.get()), (1, 6_000));
    }

    #[test]
    fn exit_flags_are_spent_only_by_their_own_match_and_a_new_match_forgets_old_ones() {
        // One test owns both statics (tests run in parallel).
        let blank = |t| Some(reborn_core::series::PeerSighting { since_ms: t, alone: false });
        let alone = |t| Some(reborn_core::series::PeerSighting { since_ms: t, alone: true });
        *lock(&SERIES_CLOSING) = Some("m-old".into());
        note_peer_lost("m-old", 1_000, false);
        note_peer_lost("m-old", 2_000, false);
        assert_eq!(*lock(&PEER_LOST), Some(("m-old".to_string(), blank(1_000).unwrap())), "the first sighting is kept");
        assert_eq!(spend_exit_flags("m-new"), (false, None), "another match's flags never apply");
        assert_eq!(lock(&SERIES_CLOSING).as_deref(), Some("m-old"), "and are not spent by it");
        assert_eq!(spend_exit_flags("m-old"), (true, blank(1_000)));
        assert_eq!(*lock(&SERIES_CLOSING), None);
        assert_eq!(*lock(&PEER_LOST), None);
        assert_eq!(spend_exit_flags("m-old"), (false, None), "spent once");

        // A rematch (new match id) clears a flag a failed close left behind.
        *lock(&SERIES_CLOSING) = Some("m-old".into());
        note_peer_lost("m-old", 1_000, false);
        forget_other_matches("m-old");
        assert_eq!(lock(&SERIES_CLOSING).as_deref(), Some("m-old"), "the current match keeps its flags");
        assert!(lock(&PEER_LOST).is_some());
        note_peer_lost("m-rematch", 5_000, false);
        assert_eq!(*lock(&PEER_LOST), Some(("m-rematch".to_string(), blank(5_000).unwrap())), "a new match's sighting replaces an old one");
        forget_other_matches("m-next");
        assert_eq!(*lock(&SERIES_CLOSING), None);
        assert_eq!(*lock(&PEER_LOST), None);

        // Final fix L-I1: a blank sighting upgrades to "only us left" (keeping when it was first seen), and a room
        // that shows both players again clears the flag, so one bad reading never turns every later close into a hold.
        note_peer_lost("m-a", 7_000, false);
        note_peer_lost("m-a", 8_000, true);
        assert_eq!(*lock(&PEER_LOST), Some(("m-a".to_string(), alone(7_000).unwrap())));
        note_peer_lost("m-a", 9_000, false);
        assert_eq!(*lock(&PEER_LOST), Some(("m-a".to_string(), alone(7_000).unwrap())), "our own teardown never downgrades it");
        clear_peer_lost("m-other");
        assert!(lock(&PEER_LOST).is_some(), "another match's room never clears it");
        clear_peer_lost("m-a");
        assert_eq!(*lock(&PEER_LOST), None);
        assert_eq!(spend_exit_flags("m-a"), (false, None));
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
        let v = snapshot_json(&with, &[]);
        assert_eq!(v["match"]["opponent"], "Jock", "the name keeps its old key");
        assert_eq!(v["match"]["opponent_elo"], 1162);
        assert_eq!(v["match"]["opponent_rank"], 9);
        assert_eq!(v["match"]["opponent_team"]["name"], "Falcons");
        assert_eq!(v["match"]["opponent_avatar_url"], "https://cdn.discordapp.com/avatars/1/a.png");

        let without: Snapshot = serde_json::from_value(serde_json::json!({ "match": { "id": "m", "opponent": { "id": "o", "display_name": "Jock", "rating": null } } })).unwrap();
        let v = snapshot_json(&without, &[]);
        for k in ["opponent_elo", "opponent_rank", "opponent_team", "opponent_avatar_url"] {
            assert!(v["match"][k].is_null(), "{k} must be null when the server sends nothing");
        }
    }

    #[test]
    fn the_match_view_passes_the_series_outcome_live_tally_and_rules_to_the_page() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "match": { "id": "m", "relay_state": "complete",
            "series_live": [2, 0], "outcome": { "kind": "win", "winner_seat": 0, "official": false, "left_seat": null },
            "rules": { "ranked_best_of": 3, "casual_max_games": 20, "ranked_leave_forfeits": true,
                "ranked_rematch_cooldown_hours": 24.0, "casual_rematch_unlimited": true } } }))
            .unwrap();
        let v = snapshot_json(&s, &[]);
        assert_eq!(v["match"]["series_live"], serde_json::json!([2, 0]));
        assert_eq!(v["match"]["outcome"]["kind"], "win");
        assert_eq!(v["match"]["outcome"]["winner_seat"], 0);
        assert_eq!(v["match"]["outcome"]["official"], false);
        assert_eq!(v["match"]["rules"]["ranked_best_of"], 3);

        let old: Snapshot = serde_json::from_value(serde_json::json!({ "match": { "id": "m" } })).unwrap();
        let v = snapshot_json(&old, &[]);
        assert!(v["match"]["outcome"].is_null() && v["match"]["rules"].is_null(), "an older server sends neither");
    }

    #[test]
    fn a_two_v_two_room_and_match_reach_the_page_with_their_size_and_players() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({
            "room": { "id": "r", "capacity": 4 },
            "match": { "id": "m", "seat": 2, "party": [
                { "seat": 0, "display_name": "Host", "nsr_tag": "nsr:00000000", "active": true },
                { "seat": 1, "display_name": "Guest", "nsr_tag": "nsr:11111111", "active": true },
                { "seat": 2, "display_name": "Me", "nsr_tag": "nsr:22222222", "active": true, "you": true },
                { "seat": 3, "display_name": "Gone", "nsr_tag": "nsr:33333333", "active": false } ] } })).unwrap();
        let v = snapshot_json(&s, &[]);
        assert_eq!(v["room"]["capacity"], 4);
        let party = v["match"]["party"].as_array().unwrap();
        assert_eq!(party.len(), 3, "a player who left is not drawn");
        assert_eq!(party[2]["name"], "Me");
        assert_eq!(party[2]["you"], true);
        assert!(party.iter().all(|p| p.get("nsr_tag").is_none()), "tags never reach the page");
        let old: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r" }, "match": { "id": "m" } })).unwrap();
        let v = snapshot_json(&old, &[]);
        assert_eq!(v["room"]["capacity"], 2, "an older server's room is a room for two");
        assert_eq!(v["match"]["party"], serde_json::json!([]));
    }

    #[test]
    fn the_series_close_and_the_exit_report_are_for_any_series_not_ranked_only() {
        // Adam 10-04: casual is a best of 3 too and must close out at its decider like ranked. Both gates go through
        // series::closes_at_decider (best_of from the server), never a `mode == "ranked"` test.
        let src = include_str!("online_mm.rs");
        let drive = &src[src.find("\nfn drive()").unwrap()..src.find("\nfn run_step(").unwrap()];
        assert!(drive.contains(".filter(|m| reborn_core::series::closes_at_decider(m))"), "the close follows the series");
        assert!(!drive.contains("m.mode == \"ranked\""), "no ranked-only close");
        let exit = &src[src.find("\npub fn dolphin_exited(").unwrap()..src.find("\npub(crate) fn mm_log(").unwrap()];
        assert!(exit.contains("decided_for_exit(\n            reborn_core::series::closes_at_decider(&m)"), "the exit report follows the series");
    }

    fn mv(json: &str) -> reborn_core::matchmaking::MatchView {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_seed_follows_edition_then_ruleset_then_mode() {
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","edition":"league","ruleset":"league","mode":"casual"}"#)), "league");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","edition":"league","ruleset":"raw"}"#)), "raw");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","edition":"league"}"#)), "league", "a league room without a ruleset plays league rules");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","edition":"retail","ruleset":"ranked","mode":"casual"}"#)), "ranked", "retail is sent explicitly");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","mode":"casual","ruleset":"ranked","seed_kind":"regular"}"#)), "ranked");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","mode":"ranked"}"#)), "ranked", "an older server still boots ranked by mode");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","mode":"casual","seed_kind":"pickup"}"#)), "pickup");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","mode":"casual"}"#)), "regular");
        assert_eq!(edition_of(&mv(r#"{"id":"m","edition":"league"}"#)).as_deref(), Some("league"));
        assert_eq!(edition_of(&mv(r#"{"id":"m","edition":"retail"}"#)), None, "the backend names retail; it is still retail");
        assert_eq!(edition_of(&mv(r#"{"id":"m"}"#)), None);
    }

    #[test]
    fn every_seed_choice_goes_through_seed_for_match() {
        let src = include_str!("online_mm.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        assert!(!production.contains("Some(&m.mode)"), "a seed is still picked from mode alone");
        assert_eq!(production.matches("seed_for_match(m)").count(), 2, "ready's preflight and the room launch");
    }

    #[test]
    fn a_league_match_is_launched_as_one() {
        let snap: reborn_core::matchmaking::Snapshot = serde_json::from_str(
            r#"{"match":{"id":"m1","relay_state":"launching","edition":"league","league":{"league_id":36,"fixture_id":1234,"week":6}}}"#).unwrap();
        lock(&STATE).snapshot = Some(snap);
        assert_eq!(current_match_league(), Some(1234));
        let snap: reborn_core::matchmaking::Snapshot = serde_json::from_str(r#"{"match":{"id":"m1","relay_state":"launching","edition":"league"}}"#).unwrap();
        lock(&STATE).snapshot = Some(snap);
        assert_eq!(current_match_league(), None, "a friendly is not a league game");
        lock(&STATE).snapshot = None;
        assert_eq!(current_match_league(), None);
    }

    #[test]
    fn join_error_shapes() {
        let update = join_error(reborn_core::matchmaking::MmError::LeagueUpdateRequired { required: "2026.10.09".into() });
        assert_eq!(update, serde_json::json!({"kind":"league_update_required","message":reborn_core::matchmaking::LEAGUE_UPDATE_SENTENCE,"required":"2026.10.09"}));
        let plain = join_error(reborn_core::matchmaking::MmError::Refused("No such lobby.".into()));
        assert_eq!(plain, serde_json::Value::String("No such lobby.".into()), "every other refusal stays the sentence it was");
    }

    #[test]
    fn a_join_carries_the_installed_league_build() {
        let src = include_str!("online_mm.rs");
        let join = &src[src.find("\npub async fn mm_room_join(").unwrap()..];
        let join = &join[..join.find("\n}\n").unwrap()];
        assert!(join.contains("crate::league_edition::installed_league_version()"));
        assert!(join.contains("c.join_room_as(code.trim(), version.as_deref())"));
        assert!(join.contains(".map_err(join_error)"));
    }

    #[test]
    fn the_page_sees_edition_ruleset_league_and_whether_a_rematch_is_offered() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({
            "room": { "id": "r", "edition": "league", "ruleset": "raw", "league_game_version": "2026.10.08",
                      "league": { "league_id": 36, "fixture_id": 1234, "week": 6 } },
            "match": { "id": "m", "edition": "league", "ruleset": "league", "rematch_available": false,
                       "league": { "league_id": 36, "fixture_id": 1234, "week": 6 } } })).unwrap();
        let v = snapshot_json(&s, &[]);
        assert_eq!((v["room"]["edition"].as_str(), v["room"]["ruleset"].as_str(), v["room"]["league_game_version"].as_str()), (Some("league"), Some("raw"), Some("2026.10.08")));
        assert_eq!(v["room"]["league"]["week"], 6);
        assert_eq!((v["match"]["edition"].as_str(), v["match"]["ruleset"].as_str()), (Some("league"), Some("league")));
        assert_eq!(v["match"]["league"]["fixture_id"], 1234);
        assert_eq!(v["match"]["rematch_available"], false);
        let old: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r" }, "match": { "id": "m" } })).unwrap();
        let v = snapshot_json(&old, &[]);
        assert_eq!(v["match"]["rematch_available"], true, "an older server offers a rematch");
        assert!(v["match"]["edition"].is_null() && v["room"]["edition"].is_null() && v["match"]["league"].is_null());
    }
}
