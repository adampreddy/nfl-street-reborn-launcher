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
}

// ------------------------------------------------------------------------------------------------
// The relay: getting both Dolphins into one room. `relay_driver::plan` decides; this runs it.
// ------------------------------------------------------------------------------------------------

/// Once a second: the server's view of the match (already in memory from the long poll) and this
/// PC's Dolphin room (the fork's status file), planned and run. A tick with no match and no room of
/// ours costs two reads and nothing else.
fn drive() {
    let mut mem = relay_driver::Memory::default();
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
        for step in relay_driver::plan(matched.as_ref(), &room, &mut mem) {
            let result = run_step(matched.as_ref(), &step);
            match result {
                Ok(()) => {
                    mem.record(&step);
                    lock(&STATE).relay_error = None;
                }
                Err(e) => {
                    eprintln!("matchmaking relay: {step:?} did not land: {e}");
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
    }
}

fn run_step(m: Option<&MatchView>, step: &Step) -> Result<(), String> {
    let id = m.map(|m| m.id.clone()).unwrap_or_default();
    let post = |a: MatchAction| with_client(|c| c.match_action(&id, a)).map(|_| ());
    let (runtime, game, preset) = lock(&LAUNCH).clone();
    match step {
        Step::Ready => post(MatchAction::Ready {
            ready: true,
            preflight: crate::dolphin_settings::mm_preflight(runtime.as_deref(), game.as_deref()),
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
            ))
            .map(|_| ())
        }
        Step::AssignPorts(ports) => crate::dolphin_settings::netplay_action("ports".into(), Some(ports.to_vec())),
        Step::StartGame => crate::dolphin_settings::netplay_action("start".into(), None),
        Step::LeaveRoom => crate::dolphin_settings::netplay_action("leave".into(), None),
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
            Ok(snap) => {
                backoff = Duration::from_secs(2);
                // A new epoch means the server restarted. The snapshot is complete, so there is
                // nothing to reconcile - just stop trusting cursors minted by the old process.
                if !epoch.is_empty() && epoch != snap.epoch {
                    eprintln!("matchmaking: server restarted, re-reading from scratch");
                }
                epoch = snap.epoch.clone();
                since = snap.version;
                chat = snap.chat.latest_seq;
                let wait = if snap.restarting { 250 } else { snap.poll_after_ms.min(10_000) };
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
                eprintln!("matchmaking poll: {e:?}");
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
            "id": r.id, "code": r.code, "mode": r.mode, "best_of": r.best_of, "status": r.status,
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
    use super::*;

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
}
