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
    /// "clean" | "error" | "crashed" | "terminated" | "interrupted" | "unknown"
    pub exit: String,
    pub exit_code: Option<i32>,
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
                exit: exit.to_string(),
                exit_code: code,
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
    let folder = entry.capture_folder.clone().unwrap_or_else(|| "offline".to_string());
    Some(serde_json::json!({
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
    }))
}

/// What the backend's answer means for the queue.
pub fn classify(status: u16, body: &str) -> SubmitOutcome {
    #[derive(serde::Deserialize, Default)]
    struct Reply { #[serde(default)] ok: bool, #[serde(default)] status: String, #[serde(default)] reason: Option<String>, #[serde(default)] error: Option<String> }
    let parsed: Option<Reply> = serde_json::from_str(body).ok();
    match status {
        401 => SubmitOutcome::Unauthorized,
        200..=299 => match parsed {
            Some(r) if r.ok => match r.status.as_str() {
                "accepted" | "duplicate" => SubmitOutcome::Accepted,
                "held" => SubmitOutcome::Held { reason: r.reason.unwrap_or_else(|| "held for review".to_string()) },
                "incomplete" => SubmitOutcome::Recorded { note: "Recorded as incomplete: the winner did not reach 36 points.".to_string() },
                other => SubmitOutcome::Held { reason: format!("server status {other:?}") },
            },
            Some(r) => SubmitOutcome::Rejected { reason: r.error.or(r.reason).unwrap_or_else(|| "the server said ok: false".to_string()) },
            None => SubmitOutcome::Rejected { reason: "the server answered 200 with something that is not its reply format".to_string() },
        },
        429 | 500..=599 => SubmitOutcome::Deferred { reason: format!("server answered {status}") },
        _ => SubmitOutcome::Rejected { reason: parsed.and_then(|r| r.error.or(r.reason)).unwrap_or_else(|| format!("refused with {status}: {}", body.chars().take(200).collect::<String>().trim())) },
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
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Civil time from seconds, no dependency: days since epoch to y-m-d (Howard Hinnant's algorithm).
    let days = (secs / 86_400) as i64;
    let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mth <= 2 { y + 1 } else { y };
    format!("{y:04}-{mth:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

impl Submitter for BackendSubmitter<'_> {
    fn submit(&self, entry: &QueueEntry, capture: &[u8]) -> SubmitOutcome {
        let facts = entry.session_id.as_deref().and_then(|id| self.sessions.get(id));
        let Some(env) = envelope(entry, capture, facts, &self.launcher_version, &now_iso()) else {
            return SubmitOutcome::Rejected { reason: "the capture is not a JSON object".to_string() };
        };
        let url = format!("{}/v1/results", self.origin.trim_end_matches('/'));
        match self.http.post_json(&url, &serde_json::to_vec(&env).unwrap_or_default(), Some(&self.token)) {
            Ok(reply) => classify(reply.status, &reply.body),
            Err(e) => SubmitOutcome::Deferred { reason: e.to_string() },
        }
    }
}

#[derive(Debug)]
pub enum SendAttempt {
    NotSignedIn,
    Sent(SubmitReport),
}

/// Send everything queued, if a token is stored. Clears the token when the server refuses it.
pub fn auto_submit(platform: &Platform, queue: &StatsQueue, history: &[SessionRecord]) -> Result<SendAttempt> {
    let Some(token) = identity::token(platform)? else { return Ok(SendAttempt::NotSignedIn) };
    let origin = crate::settings::Settings::load(&crate::settings::settings_path(platform)?)?
        .backend_origin
        .filter(|o| !o.trim().is_empty())
        .unwrap_or_else(|| identity::DEFAULT_ORIGIN.to_string());
    let http = crate::net::UreqHttp::new(&format!("NFL Street Reborn Launcher/{}", env!("CARGO_PKG_VERSION")));
    let sender = BackendSubmitter::new(&http, origin, token, session_facts(history), env!("CARGO_PKG_VERSION"));
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

    #[test]
    fn the_envelope_carries_the_capture_the_hash_the_folder_the_session_and_the_intent() {
        let facts = SessionFacts { session_id: "s1".into(), league_match: true, exit: "clean".into(), exit_code: Some(0) };
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

    #[test]
    fn a_legacy_folder_entry_is_sent_as_offline_and_no_session_means_no_league_intent() {
        let env = envelope(&entry("s9", None), CAP, None, "3.1.0", "t").unwrap();
        assert_eq!(env["capture_folder"], "offline");
        assert_eq!(env["intent"]["league"], false);
    }

    #[test]
    fn classify_maps_every_reply() {
        assert_eq!(classify(200, r#"{"ok":true,"status":"accepted","game_id":"g"}"#), SubmitOutcome::Accepted);
        assert_eq!(classify(200, r#"{"ok":true,"status":"duplicate","game_id":"g"}"#), SubmitOutcome::Accepted);
        assert!(matches!(classify(200, r#"{"ok":true,"status":"held","reason":"seat-conflict"}"#), SubmitOutcome::Held { reason } if reason.contains("seat-conflict")));
        assert!(matches!(classify(200, r#"{"ok":true,"status":"incomplete","reason":"below-target-score"}"#), SubmitOutcome::Recorded { .. }));
        assert_eq!(classify(401, "x"), SubmitOutcome::Unauthorized);
        assert!(matches!(classify(400, r#"{"ok":false,"error":"hash-mismatch"}"#), SubmitOutcome::Rejected { reason } if reason == "hash-mismatch"));
        assert!(matches!(classify(429, ""), SubmitOutcome::Deferred { .. }));
        assert!(matches!(classify(503, ""), SubmitOutcome::Deferred { .. }));
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
}
