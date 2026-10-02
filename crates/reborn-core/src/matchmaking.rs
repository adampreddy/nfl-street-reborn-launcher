//! The launcher's side of matchmaking: a thin client over the backend's `/v1/mm` routes.
//!
//! Built on the existing [`Http`] trait rather than a new transport, for three reasons that were
//! weighed and judged in the design panel (`Reborn Backend/docs/MATCHMAKING_SPEC.md`):
//!   * `reborn-core` is synchronous `ureq` and forbids unsafe; a websocket client would drag an
//!     async runtime or a second networking stack into it for no gain at ~100 concurrent players.
//!   * the server's `/v1/mm/live` is a LONG POLL returning a FULL SNAPSHOT plus a cursor and an
//!     epoch - state transfer, not event transfer - so this client never applies deltas, never
//!     replays missed events and never reconciles. It just replaces what it had.
//!   * `FakeHttp` already exists, so every call here is testable without a network.
//!
//! TOLERANT BY DESIGN. Every type is `#[serde(default)]` and unknown fields are ignored, because
//! the server will keep evolving while shipped launchers stay put for months (the main install is
//! still 3.0.0). A field the server adds must never break an old launcher; a field it removes must
//! degrade to a default, not an error.
//!
//! The fixtures under `tests/fixtures/mm/` are REAL responses captured from the running backend,
//! not hand-written, so the tests below decode what the server actually sends.

use serde::{Deserialize, Serialize};

use crate::net::Http;
use crate::presence::{Activity, Mode, Series};

/// Why a call failed, in the four ways the launcher actually has to handle differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MmError {
    /// 401: the device token is gone or revoked. The launcher offers sign-in; it does not retry.
    SignedOut,
    /// The server understood and said no, with a sentence meant for the player - show it as is.
    Refused(String),
    /// The server could not be reached. Ordinary; the next poll tries again.
    Offline(String),
    /// A reply that did not parse. A server/launcher mismatch, worth logging, never a panic.
    Protocol(String),
    /// The server has matchmaking switched off (`MM_ENABLED=0` unmounts its routes, so the long
    /// poll is a 404) - or it is a server from before matchmaking. Either way the launcher hides
    /// the matchmaking section rather than showing an error: this is the kill switch.
    Disabled,
}

impl std::fmt::Display for MmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MmError::SignedOut => f.write_str("You're signed out. Sign in with Discord to play online."),
            MmError::Refused(s) => f.write_str(s),
            MmError::Offline(_) => f.write_str("Can't reach the Reborn servers right now. Trying again."),
            MmError::Protocol(_) => f.write_str("The Reborn servers sent something unexpected. Try again in a moment."),
            MmError::Disabled => f.write_str("Online matchmaking is switched off right now."),
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Wire types. Only what the launcher uses; everything defaulted.
// ------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Snapshot {
    pub ok: bool,
    pub changed: bool,
    /// Changes when the server restarts. A new epoch means "re-read everything", which the full
    /// snapshot already does - it is kept so the UI can say "reconnected".
    pub epoch: String,
    pub version: u64,
    pub server_time: String,
    /// How long the server will hold the NEXT poll open. Server-owned so it can be tuned without
    /// shipping a launcher.
    pub hold_ms: u64,
    /// How long to wait before polling again. Also server-owned.
    pub poll_after_ms: u64,
    /// The server is draining for a redeploy. Poll again shortly; nothing is wrong.
    pub restarting: bool,
    pub player: Option<PlayerView>,
    pub ticket: Option<Ticket>,
    pub room: Option<Room>,
    #[serde(rename = "match")]
    pub matched: Option<MatchView>,
    pub chat: Chat,
    pub queue_depth: QueueDepth,
    pub online: Online,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlayerView {
    pub id: String,
    pub display_name: String,
    pub region: Option<String>,
    pub presence_hidden: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Ticket {
    pub id: String,
    pub mode: String,
    pub region: Option<String>,
    pub field_pick: Option<String>,
    pub enqueued_at: String,
    pub expires_at: String,
    pub waited_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Room {
    pub id: String,
    pub code: String,
    pub mode: String,
    pub visibility: String,
    pub best_of: u8,
    pub status: String,
    pub members: Vec<RoomMember>,
    /// The signed-in player hosts this room (the server's `you_host`): only the host is offered Start.
    pub you_host: bool,
    /// The game the host picked: `regular` (Quick game) or `pickup`. Absent from an older server.
    pub seed_kind: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RoomMember {
    pub player_id: String,
    pub display_name: String,
    pub ready: bool,
    pub field_pick: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Opponent {
    pub id: String,
    pub display_name: String,
    /// Elo, rank on the leaderboard, plays-as team and Discord picture: display only (the Online page's matchup card,
    /// spec 2026-10-02 §3.3). Each is absent while the server does not send it (Elo off: no rating, no rank).
    pub rating: Option<i64>,
    pub rank: Option<u32>,
    pub team: Option<serde_json::Value>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct MatchView {
    pub id: String,
    pub mode: String,
    pub status: String,
    pub relay_state: String,
    pub relay_deadline: Option<String>,
    pub best_of: u8,
    pub seat: u8,
    pub you_host: bool,
    pub opponent: Opponent,
    pub field: Option<String>,
    pub your_field_pick: Option<String>,
    pub seed_kind: Option<String>,
    pub buffer_size: Option<u8>,
    pub your_accepted: bool,
    pub your_ready: bool,
    /// This seat's opaque NetPlay nickname. Peer-visible and unauthenticated - a label that makes a
    /// mismatch detectable, never a credential.
    pub nsr_tag: Option<String>,
    pub opponent_nsr_tag: Option<String>,
    /// The traversal code, relayed by the server. Never typed by a player.
    pub host_code: Option<String>,
    /// `[seat0 wins, seat1 wins]`.
    pub series: Vec<u8>,
    /// `[seat0, seat1]` games the live feed decided (unofficial). Empty from a server before the series build.
    pub series_live: Vec<u8>,
    pub outcome: Option<Outcome>,
    pub rules: Option<SeriesRules>,
    pub wins_needed: u8,
    pub winner_seat: Option<u8>,
    pub hold_reason: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
}

/// The series outcome the server publishes (series spec §1). `kind`: open | win | forfeit | split | held | abandoned.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Outcome {
    pub kind: String,
    pub winner_seat: Option<u8>,
    pub official: bool,
    pub left_seat: Option<u8>,
}

/// The rules a match started under (series spec §2).
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct SeriesRules {
    pub ranked_best_of: u8,
    pub casual_max_games: u8,
    pub ranked_leave_forfeits: bool,
    pub ranked_rematch_cooldown_hours: f64,
    pub casual_rematch_unlimited: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Chat {
    pub scope: Option<String>,
    pub latest_seq: u64,
    pub truncated: bool,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChatMessage {
    pub seq: u64,
    pub player_id: String,
    pub display_name: String,
    pub body: String,
    pub at: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct QueueDepth {
    pub ranked: u32,
    pub casual: u32,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Online {
    pub count: u32,
    pub players: Vec<OnlinePlayer>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct OnlinePlayer {
    pub id: String,
    pub display_name: String,
    pub activity: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct QueueReply {
    pub ok: bool,
    pub ticket: Option<Ticket>,
    pub matched: bool,
    pub match_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RoomSummary {
    pub id: String,
    pub code: String,
    pub mode: String,
    pub best_of: u8,
    #[serde(deserialize_with = "host_name")]
    pub host: Option<String>,
    pub created_at: String,
}

/// The lobby browser's `host`: the server sends `{ id, display_name }` (rooms.js), an older one a plain name. Either
/// reads as the name; anything else as no name.
fn host_name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Object(o)) => o.get("display_name").and_then(|n| n.as_str()).map(str::to_string),
        _ => None,
    })
}

/// `GET /v1/rooms/<code>`: who hosts a room and its mode, for the invite line on the Online page.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RoomLookup {
    pub host_name: Option<String>,
    pub mode: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Rooms {
    rooms: Vec<RoomSummary>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Refusal {
    ok: bool,
    error: Option<String>,
}

// ------------------------------------------------------------------------------------------------
// The client.
// ------------------------------------------------------------------------------------------------

pub struct Client<'a> {
    http: &'a dyn Http,
    origin: String,
    token: String,
}

/// Hand-written, never derived: the client holds the player's BEARER TOKEN, and a derived `Debug`
/// would print it into any log line that formats the client. Same rule identity.rs follows for its
/// device code.
impl std::fmt::Debug for Client<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("origin", &self.origin).field("token", &"<redacted>").finish()
    }
}

impl<'a> Client<'a> {
    pub fn new(http: &'a dyn Http, origin: impl Into<String>, token: impl Into<String>) -> Self {
        Self { http, origin: origin.into().trim_end_matches('/').to_string(), token: token.into() }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.origin, path)
    }

    /// Turn an HTTP reply into a typed value, mapping the server's refusal shape onto
    /// [`MmError::Refused`] so the player sees the server's own sentence.
    fn decode<T: for<'de> Deserialize<'de>>(status: u16, body: &[u8]) -> Result<T, MmError> {
        if status == 401 {
            return Err(MmError::SignedOut);
        }
        if !(200..300).contains(&status) {
            let r: Refusal = serde_json::from_slice(body).unwrap_or_default();
            return Err(match (r.ok, r.error) {
                (false, Some(e)) if !e.trim().is_empty() => MmError::Refused(e),
                _ => MmError::Protocol(format!("HTTP {status} with no readable reason")),
            });
        }
        serde_json::from_slice(body).map_err(|e| MmError::Protocol(format!("{e}")))
    }

    fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, MmError> {
        let r = self.http.get_text_as(&self.url(path), Some(&self.token)).map_err(|e| MmError::Offline(e.to_string()))?;
        Self::decode(r.status, r.body.as_bytes())
    }

    fn post<T: for<'de> Deserialize<'de>>(&self, path: &str, body: serde_json::Value) -> Result<T, MmError> {
        let bytes = serde_json::to_vec(&body).map_err(|e| MmError::Protocol(e.to_string()))?;
        let r = self.http.post_json(&self.url(path), &bytes, Some(&self.token)).map_err(|e| MmError::Offline(e.to_string()))?;
        Self::decode(r.status, r.body.as_bytes())
    }

    /// The long poll. `since` is the last `version` seen; `chat` the last chat seq seen.
    pub fn live(&self, since: u64, chat: u64) -> Result<Snapshot, MmError> {
        let url = self.url(&format!("/v1/mm/live?since={since}&chat={chat}"));
        let r = self.http.get_text_as(&url, Some(&self.token)).map_err(|e| MmError::Offline(e.to_string()))?;
        // Only the long poll reads 404 as "switched off": it is the one route that always exists
        // while matchmaking is on. A 404 from an action is an ordinary refusal.
        if r.status == 404 {
            return Err(MmError::Disabled);
        }
        Self::decode(r.status, r.body.as_bytes())
    }

    /// Join the queue. There is no field pick (Adam, 2026-09-22): the game plays on whatever field
    /// the shipped seed starts on, so the body is the mode and region only.
    pub fn enqueue(&self, mode: Mode, region: Option<&str>) -> Result<QueueReply, MmError> {
        let mode = match mode {
            Mode::Casual => "casual",
            Mode::Ranked => "ranked",
        };
        self.post("/v1/mm/queue", serde_json::json!({ "mode": mode, "region": region }))
    }

    /// Leave the queue. The server route is a DELETE, not a POST.
    pub fn leave_queue(&self) -> Result<(), MmError> {
        let r = self.http.delete(&self.url("/v1/mm/queue"), Some(&self.token)).map_err(|e| MmError::Offline(e.to_string()))?;
        let _: serde_json::Value = Self::decode(r.status, r.body.as_bytes())?;
        Ok(())
    }

    pub fn rooms(&self) -> Result<Vec<RoomSummary>, MmError> {
        Ok(self.get::<Rooms>("/v1/mm/rooms")?.rooms)
    }

    /// `seed_kind` is the host's game: `pickup`, or anything else is a `regular` (Quick game) one. A server without
    /// the field ignores it and plays a regular game.
    pub fn create_room(&self, mode: Mode, public: bool, seed_kind: &str) -> Result<serde_json::Value, MmError> {
        let mode = if mode == Mode::Ranked { "ranked" } else { "casual" };
        let best_of = if mode == "ranked" { 3 } else { 1 };
        let seed_kind = if seed_kind.trim() == "pickup" { "pickup" } else { "regular" };
        self.post(
            "/v1/mm/rooms",
            serde_json::json!({ "mode": mode, "visibility": if public { "public" } else { "code" }, "best_of": best_of, "seed_kind": seed_kind }),
        )
    }

    pub fn join_room(&self, code: &str) -> Result<serde_json::Value, MmError> {
        self.post(&format!("/v1/mm/rooms/{}/join", url_path(code)), serde_json::json!({}))
    }

    /// Who invited the player (launcher spec 2026-10-02 §3.10). `None` on a 404: no such open room, or a server
    /// without the route, so the page simply leaves the name out. The code reaches the path as letters, digits and
    /// dashes only; a code with nothing left of it asks nothing.
    pub fn room_lookup(&self, code: &str) -> Result<Option<RoomLookup>, MmError> {
        let code = url_path(code);
        if code.is_empty() {
            return Ok(None);
        }
        let url = self.url(&format!("/v1/rooms/{code}"));
        let r = self.http.get_text_as(&url, Some(&self.token)).map_err(|e| MmError::Offline(e.to_string()))?;
        if r.status == 404 {
            return Ok(None);
        }
        Self::decode(r.status, r.body.as_bytes()).map(Some)
    }

    pub fn room_action(&self, room_id: &str, action: RoomAction) -> Result<serde_json::Value, MmError> {
        let (path, body) = match action {
            RoomAction::Leave => ("leave", serde_json::json!({})),
            RoomAction::Ready(r) => ("ready", serde_json::json!({ "ready": r })),
            RoomAction::Chat(ref t) => ("chat", serde_json::json!({ "text": t })),
            RoomAction::Start => ("start", serde_json::json!({})),
        };
        self.post(&format!("/v1/mm/rooms/{}/{}", url_path(room_id), path), body)
    }

    pub fn match_action(&self, match_id: &str, action: MatchAction) -> Result<serde_json::Value, MmError> {
        let (path, body) = match action {
            MatchAction::Accept(a) => ("accept", serde_json::json!({ "accept": a })),
            MatchAction::Ready { ready, ref preflight, rtt_ms } => {
                ("ready", serde_json::json!({ "ready": ready, "preflight": preflight, "rtt_ms": rtt_ms }))
            }
            MatchAction::HostLaunched => ("host-launched", serde_json::json!({})),
            MatchAction::JoinerLaunched => ("joiner-launched", serde_json::json!({})),
            MatchAction::HostCode(ref c) => ("host-code", serde_json::json!({ "code": c })),
            MatchAction::Observed { ref ports, ref players } => {
                ("observed", serde_json::json!({ "ports": ports, "players": players }))
            }
            MatchAction::RoomLost(ref r) => ("room-lost", serde_json::json!({ "reason": r })),
            MatchAction::Leave => ("leave", serde_json::json!({})),
            MatchAction::LeaveWith(r) => ("leave", serde_json::json!({ "reason": r.as_str() })),
            MatchAction::Chat(ref t) => ("chat", serde_json::json!({ "text": t })),
            MatchAction::Rematch => ("rematch", serde_json::json!({})),
        };
        self.post(&format!("/v1/mm/match/{}/{}", url_path(match_id), path), body)
    }
}

/// A room or match id in a URL path. Ids are uuids and codes are alphanumeric, so anything else is
/// refused rather than escaped - a path segment is not the place to be creative.
fn url_path(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect()
}

#[derive(Debug, Clone)]
pub enum RoomAction {
    Leave,
    Ready(bool),
    Chat(String),
    Start,
}

#[derive(Debug, Clone)]
pub enum MatchAction {
    Accept(bool),
    Ready { ready: bool, preflight: serde_json::Value, rtt_ms: Option<u32> },
    HostLaunched,
    JoinerLaunched,
    HostCode(String),
    Observed { ports: Vec<u8>, players: serde_json::Value },
    RoomLost(String),
    Leave,
    /// `/leave` naming why (series spec §3.4): `leave`, `dolphin-closed` or `dolphin-crashed`.
    LeaveWith(crate::series::LeaveReason),
    Chat(String),
    Rematch,
}

// ------------------------------------------------------------------------------------------------
// The bridge to Discord presence. Pure.
// ------------------------------------------------------------------------------------------------

fn mode_of(s: &str) -> Mode {
    if s.eq_ignore_ascii_case("ranked") {
        Mode::Ranked
    } else {
        Mode::Casual
    }
}

/// A server field id (`city_limits`) as a person would say it (`City Limits`).
pub fn field_title(id: &str) -> String {
    id.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pre-game relay states: matched, but the game is not running yet.
const PREGAME: &[&str] = &[
    "pairing", "accepted", "field", "ready", "go", "host_launching", "code_ready", "joining", "room_formed",
];

/// What presence should show for this snapshot, or `None` when matchmaking has nothing to say and
/// the launcher's own page should decide (Launcher, Browsing, Offline, Menus).
pub fn activity_for(snap: &Snapshot, now_unix: i64) -> Option<Activity> {
    if let Some(m) = &snap.matched {
        let mode = mode_of(&m.mode);
        let opponent = if m.opponent.display_name.trim().is_empty() {
            "an opponent".to_string()
        } else {
            m.opponent.display_name.clone()
        };
        let field = m.field.as_deref().map(field_title);

        if PREGAME.contains(&m.relay_state.as_str()) {
            return Some(Activity::Preparing { mode, opponent, field });
        }
        if m.relay_state == "running" || m.relay_state == "intermission" {
            let seat = m.seat as usize;
            let (mine, theirs) = (
                m.series.get(seat).copied().unwrap_or(0),
                m.series.get(1 - seat.min(1)).copied().unwrap_or(0),
            );
            let series = (m.best_of > 1).then(|| Series {
                game: mine + theirs + 1,
                best_of: m.best_of,
                wins_self: mine,
                wins_opponent: theirs,
            });
            let start = m.started_at.as_deref().and_then(iso_to_unix).unwrap_or(now_unix);
            // No live score exists yet: the stat tracker reads scores only at the Game Stats screen.
            return Some(Activity::InMatch { mode, opponent, field, series, score: None, start_unix: start });
        }
        // Held, complete, void, abandoned: the match is over as far as presence is concerned.
    }

    if let Some(r) = &snap.room {
        let players = r.members.len().min(u8::MAX as usize) as u8;
        return Some(Activity::Lobby { mode: mode_of(&r.mode), code: r.code.clone(), players, capacity: 2 });
    }

    if let Some(t) = &snap.ticket {
        let start = iso_to_unix(&t.enqueued_at).unwrap_or(now_unix);
        return Some(Activity::Queued { mode: mode_of(&t.mode), start_unix: start });
    }

    None
}

/// Unix seconds from the server's timestamps.
///
/// Every timestamp this client reads comes from JavaScript's `Date.toISOString()`, which always
/// emits `YYYY-MM-DDTHH:MM:SS(.sss)Z` in UTC. That fixed format is parsed here directly rather than
/// adding a date crate for one field. Anything that is not that shape returns `None`, and every
/// caller falls back to "now" - an elapsed timer that starts late beats a crash.
pub fn iso_to_unix(s: &str) -> Option<i64> {
    let s = s.trim();
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-');
    let (y, mo, da): (i64, i64, i64) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    let time = time.split('.').next()?;
    let mut t = time.split(':');
    let (h, mi, se): (i64, i64, i64) = (t.next()?.parse().ok()?, t.next()?.parse().ok()?, t.next()?.parse().ok()?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    // Days from the civil calendar (Howard Hinnant's algorithm), proleptic Gregorian.
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3_600 + mi * 60 + se)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_lobby_hosts_name_reads_from_either_shape() {
        // rooms.js sends `host: { id, display_name }`; a plain name, null or nothing must read too. Before this, one
        // public lobby made the whole list fail to decode, so the lobby list showed a protocol error.
        let rooms: Rooms = serde_json::from_value(serde_json::json!({ "rooms": [
            { "code": "K7QP2M", "mode": "casual", "host": { "id": "h1", "display_name": "Jock" } },
            { "code": "R4NK3D", "mode": "casual", "host": "Tuna" },
            { "code": "X1", "mode": "casual", "host": null },
            { "code": "X2", "mode": "casual" } ] })).unwrap();
        let hosts: Vec<Option<&str>> = rooms.rooms.iter().map(|r| r.host.as_deref()).collect();
        assert_eq!(hosts, vec![Some("Jock"), Some("Tuna"), None, None]);
    }

    #[test]
    fn a_room_says_whether_you_host_it() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r", "code": "K7QP2M", "you_host": true, "members": [] } })).unwrap();
        assert!(s.room.unwrap().you_host);
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r", "code": "K7QP2M", "members": [] } })).unwrap();
        assert!(!s.room.unwrap().you_host, "an older server says nothing: not the host");
    }

    use super::*;
    use crate::net::fake::FakeHttp;

    #[test]
    fn a_room_lookup_names_the_host_and_a_404_is_no_answer() {
        let url = "https://api.test/v1/rooms/K7QP2M";
        let http = FakeHttp::default().reply(url, 200, r#"{"ok":true,"host_name":"Jock","mode":"casual"}"#);
        let c = Client::new(&http, ORIGIN, "tok");
        assert_eq!(c.room_lookup("K7QP2M").unwrap(), Some(RoomLookup { host_name: Some("Jock".into()), mode: Some("casual".into()) }));
        assert_eq!(http.last_bearer().as_deref(), Some("tok"));
        let gone = FakeHttp::default().reply(url, 404, r#"{"ok":false,"error":"No such lobby."}"#);
        assert_eq!(Client::new(&gone, ORIGIN, "tok").room_lookup("K7QP2M").unwrap(), None, "no such room, or a server without the route");
        assert!(Client::new(&http, ORIGIN, "tok").room_lookup("K7/../QP2M?x").is_err(), "only letters, digits and dashes reach the path");
        let asked = http.gets.lock().unwrap().last().map(|(u, _)| u.clone());
        assert_eq!(asked.as_deref(), Some("https://api.test/v1/rooms/K7QP2Mx"), "the slashes, dots and query never reach the URL");
        let none = FakeHttp::default();
        assert_eq!(Client::new(&none, ORIGIN, "tok").room_lookup("/../?#").unwrap(), None, "nothing left of the code: no request");
        assert!(none.gets.lock().unwrap().is_empty());
    }

    #[test]
    fn a_new_room_sends_the_hosts_game_kind_and_anything_unknown_is_a_regular_game() {
        let url = "https://api.test/v1/mm/rooms";
        let http = FakeHttp::default().reply(url, 200, r#"{"ok":true,"room":{"id":"r","code":"K7QP2M"}}"#);
        let c = Client::new(&http, ORIGIN, "tok");
        let body = |http: &FakeHttp| serde_json::from_slice::<serde_json::Value>(&http.last_body().unwrap()).unwrap();
        c.create_room(Mode::Casual, false, "pickup").unwrap();
        let b = body(&http);
        assert_eq!(b["seed_kind"], "pickup");
        assert_eq!(b["mode"], "casual");
        assert_eq!(b["visibility"], "code");
        assert_eq!(b["best_of"], 1);
        c.create_room(Mode::Casual, false, "regular").unwrap();
        assert_eq!(body(&http)["seed_kind"], "regular");
        c.create_room(Mode::Ranked, true, "../evil").unwrap();
        assert_eq!(body(&http)["seed_kind"], "regular", "only the two known kinds reach the server");
    }

    #[test]
    fn a_room_carries_its_game_kind_when_the_server_sends_one() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r", "code": "K7QP2M", "seed_kind": "pickup", "members": [] } })).unwrap();
        assert_eq!(s.room.unwrap().seed_kind.as_deref(), Some("pickup"));
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r", "code": "K7QP2M", "members": [] } })).unwrap();
        assert_eq!(s.room.unwrap().seed_kind, None, "an older server says nothing");
    }

    const ORIGIN: &str = "https://api.test";

    #[test]
    fn leaving_with_a_reason_posts_it() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/mm/match/m1/leave"), 200, r#"{"ok":true}"#);
        Client::new(&http, ORIGIN, "t").match_action("m1", MatchAction::LeaveWith(crate::series::LeaveReason::DolphinCrashed)).unwrap();
        let body = http.last_body().unwrap();
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap()["reason"], "dolphin-crashed");
    }
    const IDLE: &str = include_str!("../tests/fixtures/mm/live_idle.json");
    const QUEUED: &str = include_str!("../tests/fixtures/mm/live_queued.json");
    const MATCHED: &str = include_str!("../tests/fixtures/mm/live_matched.json");
    const QUEUE_MATCHED: &str = include_str!("../tests/fixtures/mm/queue_matched_reply.json");
    const QUEUE_REFUSED: &str = include_str!("../tests/fixtures/mm/queue_refused_reply.json");

    fn snap(s: &str) -> Snapshot {
        serde_json::from_str(s).expect("a real server reply must decode")
    }

    #[test]
    fn every_real_server_reply_decodes() {
        // These are captured from the running backend, so a failure here is a genuine contract
        // break between the server and this launcher - not a test-data typo.
        for (name, body) in [("idle", IDLE), ("queued", QUEUED), ("matched", MATCHED)] {
            let s: Snapshot = serde_json::from_str(body).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(s.ok, "{name} should be ok");
            assert!(s.hold_ms > 0, "{name}: the server owns the cadence and must say so");
        }
        let _: QueueReply = serde_json::from_str(QUEUE_MATCHED).expect("queue reply decodes");
    }

    #[test]
    fn a_field_the_server_adds_later_never_breaks_this_launcher() {
        // Shipped launchers stay put for months while the server moves. An unknown field must be
        // ignored and a missing one defaulted, never an error.
        let mut v: serde_json::Value = serde_json::from_str(MATCHED).unwrap();
        v["brand_new_top_level"] = serde_json::json!({ "anything": [1, 2, 3] });
        v["match"]["brand_new_match_field"] = serde_json::json!("x");
        v["match"].as_object_mut().unwrap().remove("host_code");
        let s: Snapshot = serde_json::from_value(v).expect("tolerant decode");
        assert!(s.matched.is_some());
    }

    #[test]
    fn the_real_idle_queued_and_matched_snapshots_read_as_what_they_are() {
        let idle = snap(IDLE);
        assert!(idle.ticket.is_none() && idle.room.is_none() && idle.matched.is_none());
        assert_eq!(idle.player.as_ref().unwrap().display_name, "Adam");

        let queued = snap(QUEUED);
        let t = queued.ticket.expect("queued snapshot has a ticket");
        assert_eq!(t.mode, "casual");
        assert_eq!(t.field_pick.as_deref(), Some("the_pit"));

        let matched = snap(MATCHED);
        let m = matched.matched.expect("matched snapshot has a match");
        assert_eq!(m.opponent.display_name, "Jock");
        assert_eq!(m.relay_state, "pairing");
        assert!(m.you_host, "seat 0 hosts");
        assert!(m.nsr_tag.as_deref().unwrap_or("").starts_with("nsr:"));
    }

    #[test]
    fn presence_follows_the_real_snapshots() {
        assert_eq!(activity_for(&snap(IDLE), 0), None, "idle: the launcher's own page decides");

        match activity_for(&snap(QUEUED), 0) {
            Some(Activity::Queued { mode: Mode::Casual, start_unix }) => {
                assert!(start_unix > 1_700_000_000, "elapsed timer starts from the real enqueue time");
            }
            other => panic!("queued should be Queued, got {other:?}"),
        }

        match activity_for(&snap(MATCHED), 0) {
            Some(Activity::Preparing { mode: Mode::Casual, opponent, .. }) => assert_eq!(opponent, "Jock"),
            other => panic!("a pairing match should be Preparing, got {other:?}"),
        }
    }

    #[test]
    fn a_running_best_of_three_reads_from_this_seats_side() {
        let mut m = snap(MATCHED).matched.unwrap();
        m.relay_state = "running".into();
        m.best_of = 3;
        m.series = vec![1, 0];
        m.field = Some("city_limits".into());
        m.started_at = Some("2026-09-22T20:40:00.000Z".into());

        // Seat 0 is 1-0 up.
        let s0 = Snapshot { matched: Some(m.clone()), ..Default::default() };
        match activity_for(&s0, 0) {
            Some(Activity::InMatch { series: Some(s), field, start_unix, score, .. }) => {
                assert_eq!((s.game, s.wins_self, s.wins_opponent), (2, 1, 0));
                assert_eq!(field.as_deref(), Some("City Limits"));
                assert_eq!(start_unix, iso_to_unix("2026-09-22T20:40:00.000Z").unwrap());
                assert_eq!(score, None, "no live score source exists yet, so none is invented");
            }
            other => panic!("{other:?}"),
        }

        // The SAME match seen from seat 1 is 0-1 down. Reading the series from the wrong side
        // would tell the losing player they are winning.
        m.seat = 1;
        let s1 = Snapshot { matched: Some(m), ..Default::default() };
        match activity_for(&s1, 0) {
            Some(Activity::InMatch { series: Some(s), .. }) => assert_eq!((s.wins_self, s.wins_opponent), (0, 1)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_finished_or_held_match_stops_showing_as_in_progress() {
        for state in ["held", "complete", "void", "abandoned"] {
            let mut m = snap(MATCHED).matched.unwrap();
            m.relay_state = state.into();
            let s = Snapshot { matched: Some(m), ..Default::default() };
            assert_eq!(activity_for(&s, 0), None, "{state} must not show as a live match");
        }
    }

    #[test]
    fn the_client_sends_the_token_and_decodes_a_real_reply() {
        let url = format!("{ORIGIN}/v1/mm/live?since=3&chat=9");
        let http = FakeHttp::default().reply(&url, 200, MATCHED);
        let c = Client::new(&http, ORIGIN, "tok-123");
        let s = c.live(3, 9).expect("live decodes");
        assert_eq!(s.matched.unwrap().opponent.display_name, "Jock");
        let gets = http.gets.lock().unwrap();
        assert_eq!(gets[0], (url, Some("tok-123".to_string())), "the bearer must be sent");
    }

    #[test]
    fn the_servers_own_refusal_sentence_reaches_the_player_verbatim() {
        let url = format!("{ORIGIN}/v1/mm/queue");
        let http = FakeHttp::default().reply(&url, 409, QUEUE_REFUSED);
        let c = Client::new(&http, ORIGIN, "t");
        // The fixture is a real refusal captured before the field pick was removed; any refusal
        // sentence the server sends travels the same way.
        let err = c.enqueue(Mode::Casual, Some("na-east")).unwrap_err();
        assert_eq!(err, MmError::Refused("Pick a field first.".into()));
        assert_eq!(err.to_string(), "Pick a field first.");
    }

    /// The server's "nothing changed" answer (its 25 s hold ran out) carries cursors and nothing
    /// else. It must decode as `changed: false`, never as a snapshot with no match in it.
    #[test]
    fn a_quiet_poll_answer_is_not_an_empty_snapshot() {
        let url = format!("{ORIGIN}/v1/mm/live?since=22&chat=0");
        let quiet = r#"{"ok":true,"changed":false,"epoch":"e1","version":22,"hold_ms":25000,"poll_after_ms":0,"restarting":false}"#;
        let http = FakeHttp::default().reply(&url, 200, quiet);
        let s = Client::new(&http, ORIGIN, "t").live(22, 0).expect("decodes");
        assert!(!s.changed);
        assert_eq!(s.version, 22);
        assert!(s.matched.is_none() && s.ticket.is_none(), "and it names nothing - which is why it must not replace the last snapshot");
        let full: Snapshot = serde_json::from_str(MATCHED).unwrap();
        assert!(full.changed, "a real snapshot says so");
    }

    #[test]
    fn a_server_with_matchmaking_off_is_the_kill_switch_not_an_error() {
        let url = format!("{ORIGIN}/v1/mm/live?since=0&chat=0");
        let http = FakeHttp::default().reply(&url, 404, r#"{"ok":false,"error":"Not found"}"#);
        assert_eq!(Client::new(&http, ORIGIN, "t").live(0, 0).unwrap_err(), MmError::Disabled);
        // An action's 404 is still the server's own sentence, not the switch.
        let q = format!("{ORIGIN}/v1/mm/queue");
        let http = FakeHttp::default().reply(&q, 404, r#"{"ok":false,"error":"Not found"}"#);
        assert_eq!(Client::new(&http, ORIGIN, "t").enqueue(Mode::Casual, None).unwrap_err(), MmError::Refused("Not found".into()));
    }

    #[test]
    fn a_revoked_token_reads_as_signed_out_not_as_a_network_fault() {
        let url = format!("{ORIGIN}/v1/mm/live?since=0&chat=0");
        let http = FakeHttp::default().reply(&url, 401, r#"{"ok":false,"error":"Signed out."}"#);
        assert_eq!(Client::new(&http, ORIGIN, "t").live(0, 0).unwrap_err(), MmError::SignedOut);
    }

    #[test]
    fn an_unreachable_server_is_offline_and_never_a_panic() {
        // FakeHttp treats an unlisted URL as a transport failure.
        let http = FakeHttp::default();
        match Client::new(&http, ORIGIN, "t").live(0, 0) {
            Err(MmError::Offline(_)) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_queue_body_carries_the_mode_and_no_field() {
        let url = format!("{ORIGIN}/v1/mm/queue");
        let http = FakeHttp::default().reply(&url, 200, QUEUE_MATCHED);
        let c = Client::new(&http, ORIGIN, "t");
        let r = c.enqueue(Mode::Ranked, Some("na-east")).expect("queue");
        assert!(r.matched);
        let posts = http.posts.lock().unwrap();
        let sent: serde_json::Value = serde_json::from_slice(&posts[0].1).unwrap();
        assert_eq!(sent["mode"], "ranked");
        assert!(sent.get("field").is_none(), "no field is sent: {sent}");
    }

    #[test]
    fn an_id_cannot_smuggle_a_path_into_the_url() {
        assert_eq!(url_path("f772e15e-28e4-40ab-b667-70483f736542"), "f772e15e-28e4-40ab-b667-70483f736542");
        assert_eq!(url_path("../../admin/mm/matches"), "adminmmmatches");
        assert_eq!(url_path("ABC123?x=1"), "ABC123x1");
    }

    #[test]
    fn server_timestamps_parse_and_anything_else_is_refused() {
        assert_eq!(iso_to_unix("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(iso_to_unix("2000-03-01T00:00:00Z"), Some(951_868_800)); // after a century leap day
        assert_eq!(iso_to_unix("2026-09-22T20:34:50.425Z"), Some(1_790_109_290));
        for bad in ["", "2026-09-22", "2026-09-22T20:34:50-04:00", "not a date", "2026-13-01T00:00:00Z"] {
            assert_eq!(iso_to_unix(bad), None, "{bad:?} must not parse");
        }
    }

    #[test]
    fn the_bearer_token_never_appears_in_debug_output() {
        let http = FakeHttp::default();
        let c = Client::new(&http, ORIGIN, "super-secret-token-value");
        let shown = format!("{c:?}");
        assert!(!shown.contains("super-secret-token-value"), "token leaked into Debug: {shown}");
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn a_field_id_reads_as_a_place_name() {
        assert_eq!(field_title("city_limits"), "City Limits");
        assert_eq!(field_title("the_pit"), "The Pit");
        assert_eq!(field_title("ea_field"), "Ea Field");
    }

    #[test]
    fn the_opponents_standing_is_read_when_sent_and_absent_when_not() {
        let mut v: serde_json::Value = serde_json::from_str(MATCHED).unwrap();
        v["match"]["opponent"]["rating"] = serde_json::json!(1162);
        v["match"]["opponent"]["rank"] = serde_json::json!(9);
        v["match"]["opponent"]["team"] = serde_json::json!({ "name": "Falcons", "logo_url": "/live-assets/teams/14-128.png" });
        v["match"]["opponent"]["avatar_url"] = serde_json::json!("https://cdn.discordapp.com/avatars/1/a.png");
        let o = serde_json::from_value::<Snapshot>(v).unwrap().matched.unwrap().opponent;
        assert_eq!(o.rating, Some(1162));
        assert_eq!(o.rank, Some(9));
        assert_eq!(o.team.as_ref().and_then(|t| t["name"].as_str()), Some("Falcons"));
        assert_eq!(o.avatar_url.as_deref(), Some("https://cdn.discordapp.com/avatars/1/a.png"));

        // Today's server: `rating: null` and nothing else. Every field is simply absent.
        let o = snap(MATCHED).matched.unwrap().opponent;
        assert_eq!((o.rating, o.rank, o.team, o.avatar_url), (None, None, None, None));
    }

    #[test]
    fn a_match_view_reads_the_series_fields_and_an_older_server_without_them() {
        let m: MatchView = serde_json::from_value(serde_json::json!({
            "id": "m", "mode": "casual", "status": "complete", "best_of": 1, "series": [0, 0], "series_live": [2, 0],
            "outcome": { "kind": "win", "winner_seat": 0, "official": false, "left_seat": null },
            "rules": { "ranked_best_of": 3, "casual_max_games": 20, "ranked_leave_forfeits": true,
                       "ranked_rematch_cooldown_hours": 3, "casual_rematch_unlimited": true },
        })).unwrap();
        assert_eq!(m.series_live, vec![2, 0]);
        assert_eq!(m.outcome.as_ref().unwrap().kind, "win");
        assert_eq!(m.rules.as_ref().unwrap().casual_max_games, 20);
        let old: MatchView = serde_json::from_value(serde_json::json!({ "id": "m", "best_of": 1, "series": [0, 0] })).unwrap();
        assert!(old.outcome.is_none() && old.series_live.is_empty());
    }
}
