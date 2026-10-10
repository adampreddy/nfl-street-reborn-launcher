//! Sending results to the Reborn backend. The launcher-side owner of the network
//! half of the stats pipeline; Dolphin never posts anything.
//!
//! One destination, always: the backend decides internally whether a game is
//! offline, online, or a league fixture, from the folder Dolphin wrote into, the
//! capture's own identity block, and the league intent this launcher recorded
//! at launch. Nothing here reads a capture to decide where it goes.

use std::collections::BTreeMap;

use crate::identity;
use crate::net::Http;
use crate::platform::{Platform, Result};
use crate::stats::{QueueEntry, StatsQueue, SubmitOutcome, SubmitReport, Submitter};
use crate::supervisor::{SessionEnd, SessionRecord};

/// What the launcher knew about a session, carried into every envelope from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFacts {
    pub session_id: String,
    pub league_match: bool,
    /// The session booted the league edition. With no fixture it is an offline or friendly game, which never uploads.
    pub league_edition: bool,
    /// "clean" | "error" | "crashed" | "terminated" | "interrupted" | "unknown"
    pub exit: String,
    pub exit_code: Option<i32>,
    /// The matchmade match the session played, from its intent. None for everything else.
    pub match_id: Option<String>,
    /// The league fixture, from the intent, when this was a league game.
    pub league_fixture_id: Option<u64>,
}

pub fn session_facts(history: &[SessionRecord]) -> Vec<SessionFacts> {
    history
        .iter()
        .map(|r| {
            let (exit, code) = match &r.end {
                Some(SessionEnd::Exited { code: Some(0) }) => ("clean", Some(0)),
                Some(SessionEnd::Exited { code }) => ("error", *code),
                Some(SessionEnd::Crashed { .. }) => ("crashed", None),
                Some(SessionEnd::Terminated) => ("terminated", None),
                Some(SessionEnd::Interrupted) => ("interrupted", None),
                None => ("unknown", None),
            };
            SessionFacts {
                session_id: r.id.clone(),
                league_match: r.intent.as_ref().is_some_and(|i| i.league_match),
                league_edition: r.intent.as_ref().is_some_and(|i| i.league_edition),
                exit: exit.to_string(),
                exit_code: code,
                match_id: r.intent.as_ref().and_then(|i| i.match_id.clone()),
                league_fixture_id: r.intent.as_ref().and_then(|i| i.league_fixture_id),
            }
        })
        .collect()
}

/// Wrap one capture. None when the bytes are not a JSON object (the queue quarantines those before this).
pub fn envelope(entry: &QueueEntry, capture_bytes: &[u8], facts: Option<&SessionFacts>, launcher_version: &str, now: &str) -> Option<serde_json::Value> {
    let capture: serde_json::Value = serde_json::from_slice(capture_bytes).ok()?;
    if !capture.is_object() {
        return None;
    }
    let hash = crate::stats::wire_hash_of(capture_bytes)?;
    let runtime_version = capture.pointer("/reborn_match/runtime_version").cloned().unwrap_or(serde_json::Value::Null);
    // Not "offline". A capture with no folder came from the legacy folder,
    // whose mode nobody recorded, and saying "offline" is the launcher
    // asserting a fact it does not have. An offline game, a league game and a
    // ranked game make byte identical captures, so a mode is exactly the kind
    // of thing that must never be invented on the way out.
    let folder = entry
        .capture_folder
        .clone()
        .unwrap_or_else(|| crate::stats::UNKNOWN_MODE.to_string());
    let mut env = serde_json::json!({
        "schema": "nsr.submission.v1",
        "capture": capture,
        "capture_hash": hash,
        "capture_folder": folder,
        "session": {
            "session_id": entry.session_id.clone().unwrap_or_default(),
            "launcher_version": launcher_version,
            "runtime_version": runtime_version,
            "exit": facts.map(|f| f.exit.as_str()).unwrap_or("unknown"),
            "exit_code": facts.and_then(|f| f.exit_code),
        },
        "intent": { "league": facts.is_some_and(|f| f.league_match) },
        "flags": entry.summary.as_ref().map(|s| s.flags.clone()).unwrap_or_default(),
        "submitted_at": now,
    });
    // A league game names its fixture beside `intent.league` (Plan 2: forward.js reads both). Only for a league game.
    if let Some(fixture) = facts.filter(|f| f.league_match).and_then(|f| f.league_fixture_id) {
        env["intent"]["fixture_id"] = serde_json::Value::from(fixture);
    }
    // Series spec §3.1 (MATCHMAKING_SPEC T12): a result from a matchmade session names its match at the top level, so
    // results.js links it into the series. Only a canonical uuid goes out; nothing else in the envelope changes.
    if let Some(id) = facts.and_then(|f| f.match_id.as_deref()).filter(|id| crate::live_view::is_canonical_uuid(id)) {
        env["match_id"] = serde_json::Value::String(id.to_string());
    }
    Some(env)
}

/// One sentence a player can read, for every answer that is not a refusal the
/// launcher could actually read. Deliberately says nothing about status codes,
/// bodies or hosts: this string is carried on the queue entry as `last_error`
/// and shown on the results screen.
/// Shown on the results list for a league game that does not count toward a league.
const LEAGUE_NOT_UPLOADED: &str = "This league game was not part of a league fixture, so it stays on this computer and is not sent.";

const COULD_NOT_DELIVER: &str = "The server could not take this result just now. It is being kept and will be sent again.";

/// The refusal the server states, in words a player can act on.
///
/// # Why the server's own words are not shown
///
/// The backend answers a refusal with a code out of its own vocabulary:
/// `hash-mismatch`, `seat-conflict`, `below-target-score`. Those went straight
/// into [`SubmitOutcome::Rejected`], onto the entry as its reason, and out
/// through `StatsEntryView::reason` onto the results list. A player reading
/// "hash-mismatch" learns nothing and can do nothing.
///
/// They are also not text this product controls. The backend ships from a
/// different plan, its results endpoint is not built yet, and there is no
/// agreed list of codes anywhere in `docs/architecture`. Passing whatever
/// arrives straight through to a player is a promise this launcher cannot
/// keep, in a product whose rule is that no raw technical text ever reaches a
/// screen.
///
/// # What happens to the code instead
///
/// It is not discarded. Every code the launcher knows is mapped to a sentence
/// that says what it means and what happens next; anything else gets
/// [`REFUSED_UNKNOWN`], which is honest about the refusal without inventing a
/// cause. Either way the code itself goes to `tracing::warn!`, so the one
/// person who can act on `hash-mismatch` -- whoever is looking at the queue
/// with a log open -- still has it.
fn refused(stated: Option<String>) -> SubmitOutcome {
    let code = stated.filter(|s| !s.is_empty());
    SubmitOutcome::Rejected {
        reason: refusal_sentence(code.as_deref().unwrap_or_default()),
        // The server's own word, as received, beside the sentence rather than
        // inside it. A rejection is permanent and the way back is a person
        // running a command; from the sentence alone that person cannot tell a
        // hash mismatch from a seat conflict.
        code,
    }
}

fn refusal_sentence(code: &str) -> String {
    let normalized = code.trim().to_ascii_lowercase().replace([' ', '_'], "-");
    let sentence = match normalized.as_str() {
        "hash-mismatch" => REFUSED_CHANGED,
        "seat-conflict" => REFUSED_SEAT,
        "no-such-fixture" | "unknown-fixture" => REFUSED_NO_FIXTURE,
        "below-target-score" | "incomplete-game" => REFUSED_INCOMPLETE,
        _ => REFUSED_UNKNOWN,
    };
    tracing::warn!("the server refused a result and gave the reason {code:?}");
    sentence.to_string()
}

/// The capture and the server disagree about what was played.
const REFUSED_CHANGED: &str =
    "The server would not take this result because it does not match the game that was played. \
     It is kept on this PC, and it can be sent again once that is sorted out.";

/// Two seats in the same match disagree, or the seat is already taken.
const REFUSED_SEAT: &str =
    "The server would not take this result because it disagrees with the other player's copy of \
     the same game. It is kept on this PC while that is sorted out.";

/// The server has no fixture this result belongs to.
const REFUSED_NO_FIXTURE: &str =
    "The server would not take this result because it has no record of this game. It is kept on \
     this PC, and it can be sent again once that is sorted out.";

/// The game did not reach the finish the league counts.
const REFUSED_INCOMPLETE: &str =
    "The server would not take this result because the game did not finish. It is kept on this PC.";

/// A refusal in words this launcher does not know. It still says the true and
/// useful things: refused, kept, and can be sent again.
const REFUSED_UNKNOWN: &str =
    "The server would not take this result. It is kept on this PC, and it can be sent again later.";

/// A success: stored, and a person will decide. The wording says so plainly,
/// because a player who reads "held" as a failure is the mistake that produced
/// a real outage on this project.
const HELD_FOR_REVIEW: &str =
    "This result reached the server, and a league moderator will look at it before it counts. \
     Nothing more is needed from you.";

/// The queue quarantines anything that is not a capture before it ever gets
/// here, so this is a should-never-happen. It is still a sentence.
const NOT_A_CAPTURE: &str =
    "This result could not be sent because the file the game wrote cannot be read. It is being kept.";

/// What the backend's answer means for the queue.
///
/// # Only a refusal the launcher can read is permanent
///
/// [`SubmitOutcome::Rejected`] is irreversible by design: it writes a ledger
/// row, the row is applied over the record on every read, and nothing in the
/// launcher retries it. That is the right answer to a server that has looked
/// at a result and said no, and the wrong answer to everything else.
///
/// Every status that was not 2xx, 401, 429 or 5xx used to become a rejection,
/// with the status code and 200 characters of the body as the reason. So a
/// wrong path answering 404, an edge rule answering 403, or a proxy answering
/// 400 with its own error page destroyed every queued game permanently, and
/// put raw transport text on the player's screen while doing it.
///
/// Now a refusal has to be legible to count: a reply in the server's own
/// format that says why. Anything else defers, and the capture stays sendable.
///
/// # What "the server's own format" means, and why the first attempt at this
/// was not enough
///
/// The first version parsed the body into a reply type whose every field
/// defaults, then asked whether it carried an `error` or a `reason`. Any JSON
/// object at all parses into a type like that, so the test was really "is this
/// JSON, and does it happen to have an error field". Content delivery networks
/// and API gateways answer 403 and 404 with exactly that shape as a matter of
/// course, so the case this rule exists for -- an edge refusing the request
/// before it ever reaches the server -- still destroyed every queued game
/// permanently whenever the edge answered in JSON rather than in HTML.
///
/// Our server always states whether it succeeded. So `ok` has to be **present**
/// for a body to be ours at all, and a non-2xx refusal has to say `ok: false`.
/// A body that merely happens to contain an error field is not our server
/// talking, and nothing that is not our server may make a refusal permanent.
pub fn classify(status: u16, body: &str) -> SubmitOutcome {
    #[derive(serde::Deserialize, Default)]
    struct Reply { #[serde(default)] ok: Option<bool>, #[serde(default)] status: String, #[serde(default)] reason: Option<String>, #[serde(default)] error: Option<String> }
    // Ours only if it says whether it succeeded. Everything else is somebody
    // between us and the server, however well formed it looks.
    let parsed: Option<Reply> = serde_json::from_str(body).ok().filter(|r: &Reply| r.ok.is_some());
    // The server's own words for why it refused, or nothing.
    let stated = |r: Reply| r.error.or(r.reason).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    match status {
        // Only our server, saying so in its own format, retires the token.
        // A 401 from something in front of it (a gateway asking for its own
        // sign-in, a proxy) is a delivery failure, not a revoked token.
        401 => match parsed {
            Some(r) if r.ok == Some(false) => SubmitOutcome::Unauthorized,
            _ => SubmitOutcome::Deferred { reason: COULD_NOT_DELIVER.to_string() },
        },
        200..=299 => match parsed {
            Some(r) if r.ok == Some(true) => match r.status.as_str() {
                "accepted" | "duplicate" => SubmitOutcome::Accepted,
                "held" => {
                    if let Some(why) = r.reason.as_deref() {
                        tracing::warn!("the server held a result and gave the reason {why:?}");
                    }
                    SubmitOutcome::Held { reason: HELD_FOR_REVIEW.to_string() }
                }
                "incomplete" => SubmitOutcome::Recorded { note: "Recorded as incomplete: the winner did not reach 36 points.".to_string() },
                // A status this launcher does not know. Kept and never resent,
                // which is the safe side, and described as what it is: with the
                // server, waiting on a person.
                other => {
                    tracing::warn!("the server answered with the status {other:?}, which this launcher does not know");
                    SubmitOutcome::Held { reason: HELD_FOR_REVIEW.to_string() }
                }
            },
            // ok: false in the server's own format IS a considered refusal.
            Some(r) => refused(stated(r)),
            // A 2xx carrying something that is not the reply format is most
            // likely a proxy's page over a request that worked. Sending it
            // again is safe, because the server answers a repeat as a
            // duplicate; destroying it is not.
            None => SubmitOutcome::Deferred { reason: COULD_NOT_DELIVER.to_string() },
        },
        429 | 500..=599 => SubmitOutcome::Deferred { reason: COULD_NOT_DELIVER.to_string() },
        // Terminal only when our server says, in its own words, that it
        // looked at this result and did not store it.
        _ => match parsed.filter(|r| r.ok == Some(false)).and_then(stated) {
            Some(code) => refused(Some(code)),
            None => SubmitOutcome::Deferred { reason: COULD_NOT_DELIVER.to_string() },
        },
    }
}

pub struct BackendSubmitter<'a> {
    pub http: &'a dyn Http,
    pub origin: String,
    token: String,
    sessions: BTreeMap<String, SessionFacts>,
    launcher_version: String,
}

impl<'a> BackendSubmitter<'a> {
    pub fn new(http: &'a dyn Http, origin: impl Into<String>, token: impl Into<String>, sessions: Vec<SessionFacts>, launcher_version: impl Into<String>) -> Self {
        Self {
            http,
            origin: origin.into(),
            token: token.into(),
            sessions: sessions.into_iter().map(|s| (s.session_id.clone(), s)).collect(),
            launcher_version: launcher_version.into(),
        }
    }
}

impl std::fmt::Debug for BackendSubmitter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token is deliberately absent.
        f.debug_struct("BackendSubmitter").field("origin", &self.origin).finish()
    }
}

fn now_iso() -> String {
    // One clock for the whole crate: the log stamps its lines with the same
    // one, so a line in the log and an envelope on the wire can be lined up.
    crate::utc_now_iso()
}

impl Submitter for BackendSubmitter<'_> {
    fn submit(&self, entry: &QueueEntry, capture: &[u8]) -> SubmitOutcome {
        let facts = entry.session_id.as_deref().and_then(|id| self.sessions.get(id));
        // Ruling 2026-10-10 (A): league stats are fixture games only. A league edition session with no fixture (the League
        // tab's Play offline, a league friendly) keeps its result on this machine and sends nothing.
        if facts.is_some_and(|f| f.league_edition && !(f.league_match && f.league_fixture_id.is_some())) {
            return SubmitOutcome::Skipped { reason: LEAGUE_NOT_UPLOADED.to_string() };
        }
        let Some(env) = envelope(entry, capture, facts, &self.launcher_version, &now_iso()) else {
            tracing::warn!("a queued capture is not a JSON object, so no envelope could be built for it");
            return SubmitOutcome::Rejected { reason: NOT_A_CAPTURE.to_string(), code: None };
        };
        let url = format!("{}/v1/results", self.origin.trim_end_matches('/'));
        match self.http.post_json(&url, &serde_json::to_vec(&env).unwrap_or_default(), Some(&self.token)) {
            Ok(reply) => classify(reply.status, &reply.body),
            // The transport's own words name hosts, ports and libraries, and
            // this reason is shown on the results screen.
            Err(_) => SubmitOutcome::Deferred { reason: COULD_NOT_DELIVER.to_string() },
        }
    }
}

#[derive(Debug)]
pub enum SendAttempt {
    NotSignedIn,
    Sent(SubmitReport),
}

/// One send at a time in this process.
///
/// The single-instance lock used to do this job by accident, because every
/// caller held it across the whole send. It cannot any more: holding it across
/// a network round trip made a player pressing Play be told that another
/// launcher was running, when the only thing holding the lock was this
/// launcher's own background send.
///
/// So the serialization is stated rather than borrowed. Two passes over the
/// same queue at the same time would each read the same pending entries and
/// each post them, and while the server answers a repeat as a duplicate, a
/// game reaching it twice is not a thing to leave to the server's manners.
static ONE_SEND_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Send everything queued, if a token is stored. Clears the token when the server refuses it.
///
/// Serialized against every other send in this process; see
/// [`ONE_SEND_AT_A_TIME`]. Call it without holding the single-instance lock.
pub fn auto_submit(platform: &Platform, queue: &StatsQueue, history: &[SessionRecord]) -> Result<SendAttempt> {
    // A poisoned mutex means a previous send panicked. The queue is durable
    // and every entry is still exactly where it was, so the right answer is to
    // carry on rather than refuse every send from here on.
    let _one_at_a_time = ONE_SEND_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let Some(token) = identity::token(platform)? else { return Ok(SendAttempt::NotSignedIn) };
    let origin = crate::settings::Settings::load(&crate::settings::settings_path(platform)?)?
        .backend_origin
        .filter(|o| !o.trim().is_empty())
        .unwrap_or_else(|| identity::DEFAULT_ORIGIN.to_string());
    let http = crate::net::UreqHttp::new(&format!("NFL Street Reborn Launcher/{}", crate::build_version()));
    let sender = BackendSubmitter::new(&http, origin, token, session_facts(history), crate::build_version());
    let report = queue.submit_pending(&sender, 5)?;
    if report.waiting_for_sign_in {
        identity::clear_token(platform)?;
    }
    Ok(SendAttempt::Sent(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;
    use crate::stats::{EntryState, QueueEntry, SubmitOutcome};

    fn entry(session: &str, folder: Option<&str>) -> QueueEntry {
        serde_json::from_str(&format!(
            r#"{{"key":"{}","state":{{"kind":"queued"}},"attempts":0,"first_seen_ms":1,"session_id":"{session}"{}}}"#,
            "0".repeat(64),
            folder.map(|f| format!(r#","capture_folder":"{f}""#)).unwrap_or_default()
        )).unwrap()
    }
    const CAP: &[u8] = br#"{"schema":"nsr.game.v1","source":"dolphin","captured_at":"2026-09-08T00:00:00Z","teams":[{"name":"A","score":36},{"name":"B","score":24}],"style":[1,2],"players":[],"reborn_match":{"schema":"nsr.reborn_match.v1","netplay_running":false,"runtime_version":"1.1.0","local_pads":[]}}"#;

    const MATCH: &str = "644488b4-16f1-4b76-b207-20403fbaea90";

    #[test]
    fn a_matchmade_session_names_its_match_in_the_envelope_and_nothing_else_does() {
        let facts = |match_id: Option<&str>| SessionFacts {
            session_id: "s1".into(), league_match: false, league_edition: false, exit: "clean".into(), exit_code: Some(0),
            match_id: match_id.map(str::to_string), league_fixture_id: None,
        };
        let env = envelope(&entry("s1", Some("online")), CAP, Some(&facts(Some(MATCH))), "3.1.0", "t").unwrap();
        assert_eq!(env["match_id"], MATCH, "series spec §3.1: the envelope's top-level match_id");
        let plain = envelope(&entry("s1", Some("online")), CAP, Some(&facts(None)), "3.1.0", "t").unwrap();
        assert!(plain.get("match_id").is_none(), "a game that was not matchmade says nothing about a match");
        let junk = envelope(&entry("s1", Some("online")), CAP, Some(&facts(Some("../../admin"))), "3.1.0", "t").unwrap();
        assert!(junk.get("match_id").is_none(), "only a canonical uuid ever leaves the launcher");
        assert!(envelope(&entry("s1", None), CAP, None, "3.1.0", "t").unwrap().get("match_id").is_none());
    }

    #[test]
    fn session_facts_carry_the_match_from_the_session_intent() {
        use crate::supervisor::{RecordedBuild, SessionEnd, SessionIntent, SessionRecord};
        let rec = SessionRecord {
            id: "1-online".into(), build: RecordedBuild::Online, started_ms: 1, ended_ms: Some(2),
            end: Some(SessionEnd::Exited { code: Some(0) }),
            intent: Some(SessionIntent { league_match: false, match_id: Some(MATCH.into()), league_fixture_id: None, league_edition: false }),
        };
        assert_eq!(session_facts(&[rec])[0].match_id.as_deref(), Some(MATCH));
        // A record an older build wrote has no match_id and still reads.
        let old: SessionIntent = serde_json::from_str(r#"{"league_match":true}"#).unwrap();
        assert_eq!(old.match_id, None);
    }

    #[test]
    fn the_envelope_carries_the_capture_the_hash_the_folder_the_session_and_the_intent() {
        let facts = SessionFacts { session_id: "s1".into(), league_match: true, league_edition: false, exit: "clean".into(), exit_code: Some(0), match_id: None, league_fixture_id: None };
        let env = envelope(&entry("s1", Some("offline")), CAP, Some(&facts), "3.1.0", "2026-09-08T21:00:00Z").unwrap();
        assert_eq!(env["schema"], "nsr.submission.v1");
        assert_eq!(env["capture_hash"], crate::stats::wire_hash_of(CAP).unwrap());
        assert_eq!(env["capture_folder"], "offline");
        assert_eq!(env["capture"]["schema"], "nsr.game.v1");
        assert_eq!(env["session"]["session_id"], "s1");
        assert_eq!(env["session"]["launcher_version"], "3.1.0");
        assert_eq!(env["session"]["runtime_version"], "1.1.0");
        assert_eq!(env["session"]["exit"], "clean");
        assert_eq!(env["intent"]["league"], true);
        assert_eq!(env["submitted_at"], "2026-09-08T21:00:00Z");
    }

    /// A legacy capture's mode is not known, and the envelope has to say so.
    ///
    /// It used to say "offline". That is not a default, it is an assertion:
    /// the launcher telling the backend which mode a game was played in, about
    /// a game it has no mode for. An offline game, a league game and a ranked
    /// game make byte identical captures, so a mode is exactly the kind of
    /// thing that must never be invented.
    ///
    /// It also collided with the drift sweep: a capture found in a folder
    /// nobody agreed on is stamped `unknown` on the entry, and flattening that
    /// to "offline" on the way out would undo the whole point of recording it.
    #[test]
    fn a_capture_with_no_known_mode_is_sent_as_unknown_rather_than_guessed_offline() {
        let env = envelope(&entry("s9", None), CAP, None, "3.1.0", "t").unwrap();
        assert_eq!(
            env["capture_folder"], crate::stats::UNKNOWN_MODE,
            "the launcher stated a mode it does not know"
        );
        assert_eq!(env["intent"]["league"], false);

        // A mode it does know still goes out as itself, including the one the
        // drift sweep records.
        for mode in ["online", "offline", crate::stats::UNKNOWN_MODE] {
            let env = envelope(&entry("s1", Some(mode)), CAP, None, "3.1.0", "t").unwrap();
            assert_eq!(env["capture_folder"], mode);
        }
    }

    #[test]
    fn classify_maps_every_reply() {
        assert_eq!(classify(200, r#"{"ok":true,"status":"accepted","game_id":"g"}"#), SubmitOutcome::Accepted);
        assert_eq!(classify(200, r#"{"ok":true,"status":"duplicate","game_id":"g"}"#), SubmitOutcome::Accepted);
        // The server's own `reason` is no longer repeated back: it is its
        // vocabulary, not a player's. See
        // `every_reason_this_module_produces_is_a_sentence_a_player_can_act_on`.
        assert!(matches!(classify(200, r#"{"ok":true,"status":"held","reason":"seat-conflict"}"#), SubmitOutcome::Held { reason } if reason == HELD_FOR_REVIEW));
        assert!(matches!(classify(200, r#"{"ok":true,"status":"incomplete","reason":"below-target-score"}"#), SubmitOutcome::Recorded { .. }));
        assert_eq!(classify(401, r#"{"ok":false,"error":"token revoked"}"#), SubmitOutcome::Unauthorized);
        // A 401 that is not in the server's own words came from something in
        // front of it; the token is kept and the game sent again later.
        assert!(matches!(classify(401, "x"), SubmitOutcome::Deferred { .. }));
        assert!(matches!(classify(401, r#"{"error":"login required"}"#), SubmitOutcome::Deferred { .. }));
        assert!(matches!(classify(400, r#"{"ok":false,"error":"hash-mismatch"}"#), SubmitOutcome::Rejected { reason, .. } if reason == REFUSED_CHANGED));
        assert!(matches!(classify(429, ""), SubmitOutcome::Deferred { .. }));
        assert!(matches!(classify(503, ""), SubmitOutcome::Deferred { .. }));
    }

    /// A rejection is permanent and nothing in the product could undo it: the
    /// ledger row is applied over the record on every read, the stall retry
    /// does not touch rejected entries, and enqueuing the same game again
    /// answers AlreadySubmitted forever.
    ///
    /// So a status the launcher cannot read a refusal out of must never
    /// produce one. A wrong path answering 404, an edge rule answering 403, a
    /// proxy's own error page: none of those is this server refusing this
    /// result, and treating them as one turns a recoverable outage into every
    /// queued game destroyed at once.
    #[test]
    fn an_unstructured_refusal_leaves_the_capture_sendable() {
        for (status, body) in [
            (404, "<html><title>404 Not Found</title></html>"),
            (403, ""),
            (400, "Bad Request"),
            (400, "{"),
            (413, "{\"detail\":\"too large\"}"),
            (502, "<html>bad gateway</html>"),
        ] {
            assert!(
                matches!(classify(status, body), SubmitOutcome::Deferred { .. }),
                "{status} with {body:?} was made permanent, so the game is gone"
            );
        }
    }

    /// An edge that answers in JSON is still not this server refusing this
    /// result.
    ///
    /// The previous rule was "the body parses as the reply type and carries an
    /// error or a reason". Every field of that type defaults, so any JSON
    /// object at all satisfied it, and content delivery networks and API
    /// gateways return exactly this shape on 403 and 404 as a matter of
    /// course. So the case the rule was written to fix -- an edge refusing the
    /// request -- still destroyed every queued game permanently whenever the
    /// edge answered in JSON rather than in HTML.
    ///
    /// Our server always states whether it succeeded. A body that merely
    /// happens to contain an error field is not our server talking, and
    /// nothing that is not our server may make a refusal permanent.
    #[test]
    fn an_edge_answering_in_json_is_not_this_server_refusing_this_result() {
        for (status, body) in [
            // Cloudflare, and anything modeled on it.
            (403, r#"{"error":"Forbidden"}"#),
            (403, r#"{"success":false,"errors":[{"code":1020,"message":"Access denied"}]}"#),
            // API gateways and reverse proxies.
            (404, r#"{"message":"Not Found","error":"NotFound"}"#),
            (400, r#"{"error":"invalid_request","error_description":"bad host"}"#),
            (451, r#"{"reason":"blocked by policy"}"#),
            // A bare object with nothing in it at all.
            (409, r#"{}"#),
        ] {
            assert!(
                matches!(classify(status, body), SubmitOutcome::Deferred { .. }),
                "{status} with {body} was made permanent by something that is not our \
                 server, so every game in the queue is gone"
            );
        }
    }

    /// The other half. A reply the launcher can read, which says it was
    /// refused and why, IS terminal: that is the server's considered answer
    /// and resending it forever would be the retry loop this design exists to
    /// prevent.
    ///
    /// It used to assert that the reason was the server's own code, verbatim.
    /// That assertion **was** the leak: `hash-mismatch` is the backend's
    /// vocabulary and it went straight to the results list. What has to be
    /// true instead is that the refusal is still terminal, that the launcher
    /// still tells the three refusals it knows apart rather than flattening
    /// them into one, and that each one reads as a sentence.
    #[test]
    fn a_structured_refusal_is_still_terminal_and_says_which_refusal_it_was() {
        for (status, body, want) in [
            (400, r#"{"ok":false,"error":"hash-mismatch"}"#, REFUSED_CHANGED),
            (422, r#"{"ok":false,"reason":"no such fixture"}"#, REFUSED_NO_FIXTURE),
            (409, r#"{"ok":false,"error":"seat-conflict"}"#, REFUSED_SEAT),
        ] {
            assert!(
                matches!(classify(status, body), SubmitOutcome::Rejected { reason, .. } if reason == want),
                "{status} with {body} should be a refusal saying {want:?}"
            );
        }
        // Three different refusals, three different sentences: mapping them is
        // not the same as discarding them.
        let mut seen = std::collections::BTreeSet::new();
        for code in [REFUSED_CHANGED, REFUSED_NO_FIXTURE, REFUSED_SEAT, REFUSED_INCOMPLETE, REFUSED_UNKNOWN] {
            assert!(seen.insert(code), "two refusals share one sentence: {code}");
        }
    }

    /// The server's own word for a refusal is carried out of here, beside the
    /// sentence rather than inside it.
    ///
    /// A rejection is permanent, and the documented way back is a person
    /// running `rebornctl stats retry --apply`. Mapping the code to a sentence
    /// is right for the player and leaves that person retrying blind: from
    /// "The server would not take this result..." nobody can tell a hash
    /// mismatch from a seat conflict. The code went to `tracing::warn!` and
    /// nowhere else, which is not enough for a fact about one specific game
    /// that has to outlive log rotation.
    #[test]
    fn a_refusal_carries_the_servers_own_code_beside_the_sentence() {
        for (status, body, want_sentence, want_code) in [
            (400, r#"{"ok":false,"error":"hash-mismatch"}"#, REFUSED_CHANGED, "hash-mismatch"),
            (409, r#"{"ok":false,"error":"seat-conflict"}"#, REFUSED_SEAT, "seat-conflict"),
            // A code this launcher has no sentence for still reaches the
            // operator as itself. That is the case the field matters most for.
            (400, r#"{"ok":false,"error":"roster-drift"}"#, REFUSED_UNKNOWN, "roster-drift"),
            (200, r#"{"ok":false,"reason":"seat-conflict"}"#, REFUSED_SEAT, "seat-conflict"),
        ] {
            match classify(status, body) {
                SubmitOutcome::Rejected { reason, code } => {
                    assert_eq!(reason, want_sentence, "{status} with {body}");
                    assert_eq!(
                        code.as_deref(),
                        Some(want_code),
                        "the server's own word for the refusal was thrown away, so the person who has to retry it cannot tell which refusal it was"
                    );
                }
                other => panic!("{status} with {body} should be a refusal, got {other:?}"),
            }
        }
    }

    /// No errors ever: these strings are carried on the entry as `last_error`
    /// and shown on the results screen.
    #[test]
    fn nothing_this_module_decides_puts_a_status_code_or_a_raw_body_on_screen() {
        for (status, body, fragment) in [
            (404, "<html><h1>404 Not Found</h1></html>", "html"),
            (429, "rate limited by upstream", "upstream"),
            (503, "Service Unavailable: pool exhausted", "pool"),
            (400, "panic at src/main.rs:41", "src/main.rs"),
        ] {
            let reason = match classify(status, body) {
                SubmitOutcome::Deferred { reason }
                | SubmitOutcome::Rejected { reason, .. }
                | SubmitOutcome::Held { reason } => reason,
                other => panic!("{status} produced {other:?}, which carries no reason"),
            };
            assert!(
                !reason.contains(&status.to_string()),
                "the status code reached the player: {reason}"
            );
            assert!(
                !reason.contains(fragment),
                "the raw body reached the player: {reason}"
            );
        }
    }

    /// The server's own words for a refusal are not player-facing text.
    ///
    /// `hash-mismatch`, `seat-conflict`, `below-target-score`: those are the
    /// backend's internal vocabulary, and they land verbatim in
    /// `EntryState::Rejected { reason }` and `Held { reason }`, out through
    /// `StatsEntryView::reason`, and onto the results list. A player reading
    /// "hash-mismatch" learns nothing and can do nothing.
    ///
    /// So every reason this module produces is a sentence, whether the launcher
    /// recognized the code or not. The code itself is not discarded: it goes to
    /// `tracing::warn!` at the point it is read.
    #[test]
    fn every_reason_this_module_produces_is_a_sentence_a_player_can_act_on() {
        use crate::player_sentence::assert_player_safe;
        let reason_of = |status, body: &str| match classify(status, body) {
            SubmitOutcome::Deferred { reason }
            | SubmitOutcome::Rejected { reason, .. }
            | SubmitOutcome::Held { reason } => reason,
            SubmitOutcome::Recorded { note } => note,
            other => panic!("{status} produced {other:?}, which carries no reason"),
        };
        for (status, body, code) in [
            (400, r#"{"ok":false,"error":"hash-mismatch"}"#, "hash-mismatch"),
            (409, r#"{"ok":false,"error":"seat-conflict"}"#, "seat-conflict"),
            (422, r#"{"ok":false,"reason":"no such fixture"}"#, "fixture"),
            (400, r#"{"ok":false,"error":"WHAT-EVEN-IS-THIS"}"#, "WHAT-EVEN-IS-THIS"),
            (200, r#"{"ok":false,"error":"seat-conflict"}"#, "seat-conflict"),
            (200, r#"{"ok":true,"status":"held","reason":"seat-conflict"}"#, "seat-conflict"),
            (200, r#"{"ok":true,"status":"parked-pending-audit"}"#, "parked-pending-audit"),
            (200, r#"{"ok":true,"status":"incomplete","reason":"below-target-score"}"#, "below-target"),
        ] {
            let reason = reason_of(status, body);
            assert_player_safe(&reason);
            assert!(
                !reason.contains(code),
                "the server's own vocabulary reached the player: {reason}"
            );
        }
        // The transport and the catch-all, already sentences, kept under the
        // same mechanical rules so a reword cannot break them.
        assert_player_safe(&reason_of(503, ""));
        assert_player_safe(&reason_of(404, "<html><h1>404 Not Found</h1></html>"));
        assert_player_safe(COULD_NOT_DELIVER);
    }

    #[test]
    fn the_submitter_posts_with_the_bearer_and_never_prints_it() {
        let http = FakeHttp::default().reply("https://b/v1/results", 200, r#"{"ok":true,"status":"accepted"}"#);
        let s = BackendSubmitter::new(&http, "https://b", "s3cr3t", vec![], "3.1.0");
        assert_eq!(s.submit(&entry("s1", Some("online")), CAP), SubmitOutcome::Accepted);
        let posts = http.posts.lock().unwrap();
        assert_eq!(posts[0].0, "https://b/v1/results");
        assert_eq!(posts[0].2.as_deref(), Some("s3cr3t"));
        assert!(!format!("{s:?}").contains("s3cr3t"));
        let _ = EntryState::Queued;
    }

    #[test]
    fn a_league_upload_names_its_fixture() {
        let facts = SessionFacts { session_id: "s1".into(), league_match: true, league_edition: true, exit: "clean".into(), exit_code: Some(0), match_id: None, league_fixture_id: Some(1234) };
        let env = envelope(&entry("s1", Some("online")), CAP, Some(&facts), "3.1.0", "2026-10-08T21:00:00Z").unwrap();
        assert_eq!((env["intent"]["league"].as_bool(), env["intent"]["fixture_id"].as_u64()), (Some(true), Some(1234)));
        let casual = SessionFacts { league_match: false, league_fixture_id: None, ..facts };
        let env = envelope(&entry("s1", Some("online")), CAP, Some(&casual), "3.1.0", "2026-10-08T21:00:00Z").unwrap();
        assert!(env["intent"].get("fixture_id").is_none(), "only a league game names a fixture");
    }

    fn facts_of(league_edition: bool, league_match: bool, fixture: Option<u64>) -> SessionFacts {
        SessionFacts { session_id: "s1".into(), league_match, league_edition, exit: "clean".into(), exit_code: Some(0), match_id: None, league_fixture_id: fixture }
    }

    /// Adam's ruling 10-10 (A): only a league FIXTURE game uploads. The League tab's Play offline and a league
    /// friendly (edition league, no fixture) never post, and are not left to retry.
    #[test]
    fn a_league_offline_game_and_a_league_friendly_are_never_posted() {
        let http = FakeHttp::default().reply("https://b/v1/results", 200, r#"{"ok":true,"status":"accepted"}"#);
        for facts in [facts_of(true, false, None), facts_of(true, true, None)] {
            let s = BackendSubmitter::new(&http, "https://b", "tok", vec![facts], "3.1.0");
            assert!(matches!(s.submit(&entry("s1", Some("offline")), CAP), SubmitOutcome::Skipped { .. }));
            assert!(matches!(s.submit(&entry("s1", Some("online")), CAP), SubmitOutcome::Skipped { .. }));
        }
        assert!(http.posts.lock().unwrap().is_empty(), "nothing reached the network");
    }

    #[test]
    fn a_league_fixture_game_still_uploads_with_its_fixture_and_retail_is_unchanged() {
        let http = FakeHttp::default().reply("https://b/v1/results", 200, r#"{"ok":true,"status":"accepted"}"#);
        let s = BackendSubmitter::new(&http, "https://b", "tok", vec![facts_of(true, true, Some(1234))], "3.1.0");
        assert_eq!(s.submit(&entry("s1", Some("online")), CAP), SubmitOutcome::Accepted);
        let body: serde_json::Value = serde_json::from_slice(&http.posts.lock().unwrap()[0].1).unwrap();
        assert_eq!((body["intent"]["league"].as_bool(), body["intent"]["fixture_id"].as_u64()), (Some(true), Some(1234)));

        let http = FakeHttp::default().reply("https://b/v1/results", 200, r#"{"ok":true,"status":"accepted"}"#);
        let s = BackendSubmitter::new(&http, "https://b", "tok", vec![facts_of(false, false, None)], "3.1.0");
        assert_eq!(s.submit(&entry("s1", Some("offline")), CAP), SubmitOutcome::Accepted);
        // No session facts at all (a legacy capture) also posts as before.
        let s = BackendSubmitter::new(&http, "https://b", "tok", vec![], "3.1.0");
        assert_eq!(s.submit(&entry("s1", Some("offline")), CAP), SubmitOutcome::Accepted);
        assert_eq!(http.posts.lock().unwrap().len(), 2);
    }

    #[test]
    fn session_facts_know_a_league_edition_session_from_its_intent() {
        use crate::supervisor::{RecordedBuild, SessionEnd, SessionIntent, SessionRecord};
        let rec = |league_edition| SessionRecord {
            id: "1-offline".into(), build: RecordedBuild::Offline, started_ms: 1, ended_ms: Some(2),
            end: Some(SessionEnd::Exited { code: Some(0) }),
            intent: Some(SessionIntent { league_match: false, match_id: None, league_fixture_id: None, league_edition }),
        };
        assert!(session_facts(&[rec(true)])[0].league_edition);
        assert!(!session_facts(&[rec(false)])[0].league_edition);
        let old: SessionIntent = serde_json::from_str(r#"{"league_match":true}"#).unwrap();
        assert!(!old.league_edition, "a record an older build wrote is not a league edition session");
    }

    #[test]
    fn session_facts_carry_the_fixture_from_the_intent() {
        use crate::supervisor::SessionIntent;
        let old: SessionIntent = serde_json::from_str(r#"{"league_match":true}"#).unwrap();
        assert_eq!(old.league_fixture_id, None, "a record an older build wrote still reads");
        let new: SessionIntent = serde_json::from_str(r#"{"league_match":true,"league_fixture_id":1234}"#).unwrap();
        assert_eq!(new.league_fixture_id, Some(1234));
    }
}
