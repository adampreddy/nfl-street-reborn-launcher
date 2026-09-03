//! Sending match results to the league site.
//!
//! This is the launcher-side port of Dolphin's `NflsStatUpload`, which
//! `ONLINE_AND_RESULTS_PROTOCOL.md` section 10 retires from the shipping
//! binary along with the token compiled into it. The contract is the live
//! `ingest.php`, read from its source rather than guessed:
//!
//!   POST the raw `nsr.game.v1` capture as JSON, with `Authorization: Bearer`.
//!   200 with `{"ok":true,"status":"applied"|"duplicate"|"review"}`.
//!   Anything else with a body is a refusal that says why.
//!
//! What each answer MEANS is the rule Dolphin's uploader learned in
//! production and this keeps: applied and duplicate are both accepted and
//! never sent again; review is a success that a person will finish; a
//! server refusal is not retried; a transport failure is deferred and retried
//! later. The queue in [`crate::stats`] enforces the never-again part.
//!
//! The token is never held here longer than a call and never appears in any
//! message this module produces.

use crate::net::Http;
use crate::stats::{QueueEntry, SubmitOutcome, Submitter};

/// Where results go when nothing else is set: the league site's ingest
/// endpoint, the same one Dolphin's uploader has posted to since 2026.
pub const DEFAULT_INGEST_URL: &str = "https://nflstreetreborn.com/league/stats/ingest.php";

/// The credential vault account the bearer token is stored under.
pub const TOKEN_ACCOUNT: &str = "nsr.league.ingest";

/// Sends captures to the league endpoint.
pub struct LeagueSubmitter<'a> {
    pub http: &'a dyn Http,
    pub url: String,
    token: String,
}

impl<'a> LeagueSubmitter<'a> {
    pub fn new(http: &'a dyn Http, url: impl Into<String>, token: impl Into<String>) -> Self {
        Self { http, url: url.into(), token: token.into() }
    }
}

impl std::fmt::Debug for LeagueSubmitter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token is deliberately absent.
        f.debug_struct("LeagueSubmitter").field("url", &self.url).finish()
    }
}

/// What `ingest.php` answers.
#[derive(Debug, serde::Deserialize)]
struct IngestReply {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    status: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Turn a reply into what the queue should do with the capture.
pub fn classify(status: u16, body: &str) -> SubmitOutcome {
    let parsed: Option<IngestReply> = serde_json::from_str(body).ok();
    match (status, parsed) {
        (200..=299, Some(reply)) if reply.ok => match reply.status.as_str() {
            "applied" | "duplicate" => SubmitOutcome::Accepted,
            "review" => SubmitOutcome::Held {
                reason: reply.reason.unwrap_or_else(|| "held for commissioner review".to_string()),
            },
            other => SubmitOutcome::Held { reason: format!("server status {other:?}") },
        },
        (200..=299, Some(reply)) => SubmitOutcome::Rejected {
            reason: reply.error.or(reply.reason).unwrap_or_else(|| "the server said ok: false".to_string()),
        },
        (200..=299, None) => SubmitOutcome::Rejected {
            reason: "the server answered 200 with something that is not its reply format".to_string(),
        },
        // The server is up and refusing. Retrying the same bytes will not help;
        // 5xx is the one exception, where the server itself is the problem.
        (500..=599, _) => SubmitOutcome::Deferred { reason: format!("server error {status}") },
        (code, Some(reply)) => SubmitOutcome::Rejected {
            reason: reply.error.or(reply.reason).unwrap_or_else(|| format!("refused with {code}")),
        },
        (code, None) => SubmitOutcome::Rejected {
            reason: format!("refused with {code}: {}", body.chars().take(200).collect::<String>().trim()),
        },
    }
}

impl Submitter for LeagueSubmitter<'_> {
    fn submit(&self, _entry: &QueueEntry, capture: &[u8]) -> SubmitOutcome {
        match self.http.post_json(&self.url, capture, Some(&self.token)) {
            Ok(reply) => classify(reply.status, &reply.body),
            Err(e) => SubmitOutcome::Deferred { reason: e.to_string() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;

    #[test]
    fn the_live_endpoints_three_answers_map_to_accepted_accepted_and_held() {
        assert_eq!(classify(200, r#"{"ok":true,"status":"applied","import_id":5}"#), SubmitOutcome::Accepted);
        assert_eq!(classify(200, r#"{"ok":true,"status":"duplicate","import_id":5}"#), SubmitOutcome::Accepted);
        assert!(matches!(
            classify(200, r#"{"ok":true,"status":"review","reason":"sides not verified"}"#),
            SubmitOutcome::Held { reason } if reason == "sides not verified"
        ));
    }

    #[test]
    fn a_refusal_is_final_and_a_server_error_or_no_route_is_deferred() {
        assert!(matches!(classify(422, r#"{"ok":false,"error":"Unsupported schema"}"#), SubmitOutcome::Rejected { reason } if reason == "Unsupported schema"));
        assert!(matches!(classify(401, "nope"), SubmitOutcome::Rejected { .. }));
        assert!(matches!(classify(503, ""), SubmitOutcome::Deferred { .. }));

        let http = FakeHttp::default();
        let sender = LeagueSubmitter::new(&http, "https://league/ingest.php", "secret");
        let entry: QueueEntry = serde_json::from_str(
            r#"{"key":"0000000000000000000000000000000000000000000000000000000000000000","state":{"kind":"queued"},"attempts":0,"first_seen_ms":1}"#,
        )
        .unwrap();
        let out = sender.submit(&entry, b"{}");
        assert!(matches!(out, SubmitOutcome::Deferred { .. }), "{out:?}");
    }

    #[test]
    fn the_capture_goes_as_the_body_with_the_bearer_and_the_token_is_never_printed() {
        let http = FakeHttp::default().reply("https://league/ingest.php", 200, r#"{"ok":true,"status":"applied"}"#);
        let sender = LeagueSubmitter::new(&http, "https://league/ingest.php", "s3cr3t");
        let entry: QueueEntry = serde_json::from_str(
            r#"{"key":"0000000000000000000000000000000000000000000000000000000000000000","state":{"kind":"queued"},"attempts":0,"first_seen_ms":1}"#,
        )
        .unwrap();
        assert_eq!(sender.submit(&entry, br#"{"schema":"nsr.game.v1"}"#), SubmitOutcome::Accepted);
        let posts = http.posts.lock().unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].1, br#"{"schema":"nsr.game.v1"}"#.to_vec());
        assert_eq!(posts[0].2.as_deref(), Some("s3cr3t"));
        assert!(!format!("{sender:?}").contains("s3cr3t"));
    }
}
