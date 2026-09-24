//! A typed client for the backend's public live endpoints: `GET /v1/live` (the "Live now" list)
//! and `GET /v1/live/{id}` (one match's snapshot, for a native match view). Both are public --
//! no bearer, no sign-in -- so this is a thin `GET` plus status handling, built on the same
//! [`Http`] trait as every other client in this crate.
//!
//! Values pass through as [`serde_json::Value`] rather than a typed struct. The backend's README
//! documents the fields; this module does not retype them, so the backend stays free to add a
//! field without a launcher release. The UI reads what it needs and ignores the rest.
//!
//! [`LiveError`] is deliberately thin: it never carries a URL, a body, or a token, so whatever a
//! caller does with it (log it, show it) can never leak one by accident. [`LiveError::player_message`]
//! is the only wording a screen should ever show.

use serde_json::Value;

use crate::net::Http;

/// Why a call failed, in the four ways a caller actually has to handle differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveError {
    /// The id given to [`snapshot`] is not a canonical (lowercase) UUID. Checked before any
    /// request is made, so a malformed id -- someone else's URL fragment, `../x`, anything typed
    /// by hand -- never reaches the network at all.
    BadId,
    /// The server could not be reached: no route, DNS, timeout, refused.
    Unreachable,
    /// The server answered with an error status.
    Server(u16),
    /// The server answered success but the body was not the JSON this client expects.
    BadReply,
}

impl LiveError {
    /// Short wording a screen can show as is. Never a URL, never body text, never a status
    /// number: none of those mean anything to a player, and a status code in particular reads as
    /// noise at best and a support-forum "error 500" copy-paste at worst.
    pub fn player_message(&self) -> &'static str {
        match self {
            LiveError::BadId => "That live game link isn't valid.",
            LiveError::Unreachable | LiveError::Server(_) | LiveError::BadReply => {
                "Live games couldn't be loaded right now."
            }
        }
    }
}

/// Whether `id` is a canonical UUID: 36 characters, lowercase hex, hyphens at 8/13/18/23
/// (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`). Uppercase is refused rather than lowercased and
/// accepted, because the backend's own ids are always lowercase and a caller passing anything
/// else is not really quoting one back.
fn is_canonical_uuid(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    bytes.iter().enumerate().all(|(i, b)| match i {
        8 | 13 | 18 | 23 => *b == b'-',
        _ => b.is_ascii_digit() || (b'a'..=b'f').contains(b),
    })
}

/// `GET {origin}/v1/live`. Returns the parsed body's `matches` array as a JSON value; the UI
/// renders it directly. A body with no `matches` field is treated as an empty list rather than a
/// bad reply, since a server that omits it is still saying "nothing live" -- the same rule every
/// other client in this crate follows for a field the server might one day stop sending.
pub fn list(http: &dyn Http, origin: &str) -> Result<Value, LiveError> {
    let url = format!("{}/v1/live", origin.trim_end_matches('/'));
    let reply = http.get_text(&url).map_err(|_| LiveError::Unreachable)?;
    if !reply.is_success() {
        return Err(LiveError::Server(reply.status));
    }
    let body: Value = serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)?;
    Ok(body.get("matches").cloned().unwrap_or_else(|| Value::Array(Vec::new())))
}

/// `GET {origin}/v1/live/{id}`. `404` is not an error: the match has ended or was never live, and
/// the caller reads that as "nothing to show" rather than a failure. `match_id` is checked
/// against [`is_canonical_uuid`] before anything is sent.
pub fn snapshot(http: &dyn Http, origin: &str, match_id: &str) -> Result<Option<Value>, LiveError> {
    if !is_canonical_uuid(match_id) {
        return Err(LiveError::BadId);
    }
    let url = format!("{}/v1/live/{}", origin.trim_end_matches('/'), match_id);
    let reply = http.get_text(&url).map_err(|_| LiveError::Unreachable)?;
    if reply.status == 404 {
        return Ok(None);
    }
    if !reply.is_success() {
        return Err(LiveError::Server(reply.status));
    }
    let body: Value = serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)?;
    Ok(Some(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;
    use crate::net::HttpResponse;

    const ORIGIN: &str = "https://api.test";
    const ID: &str = "3f1c0f3e-0000-4000-8000-000000000001";

    /// A fake that panics if it is ever asked to make a request, for the id-validation tests:
    /// `snapshot` must refuse a bad id before it touches the network at all.
    struct NoCalls;
    impl Http for NoCalls {
        fn get_text(&self, url: &str) -> crate::Result<HttpResponse> {
            panic!("no request expected, got a GET for {url}");
        }
        fn get_to_file(&self, url: &str, _to: &std::path::Path) -> crate::Result<u64> {
            panic!("no request expected, got a download for {url}");
        }
        fn post_json(&self, url: &str, _body: &[u8], _bearer: Option<&str>) -> crate::Result<HttpResponse> {
            panic!("no request expected, got a POST for {url}");
        }
    }

    #[test]
    fn list_200_returns_the_matches_array() {
        let http = FakeHttp::default().reply(
            "https://api.test/v1/live",
            200,
            r#"{"ok":true,"matches":[{"id":"a"},{"id":"b"}]}"#,
        );
        let got = list(&http, ORIGIN).unwrap();
        assert_eq!(got, serde_json::json!([{"id":"a"},{"id":"b"}]));
    }

    #[test]
    fn list_500_is_a_server_error() {
        let http = FakeHttp::default().reply("https://api.test/v1/live", 500, "");
        assert_eq!(list(&http, ORIGIN).unwrap_err(), LiveError::Server(500));
    }

    #[test]
    fn list_with_an_unknown_url_is_unreachable() {
        let http = FakeHttp::default();
        assert_eq!(list(&http, ORIGIN).unwrap_err(), LiveError::Unreachable);
    }

    #[test]
    fn bad_json_is_an_error_not_a_panic() {
        let http = FakeHttp::default().reply("https://api.test/v1/live", 200, "not json");
        assert_eq!(list(&http, ORIGIN).unwrap_err(), LiveError::BadReply);
    }

    #[test]
    fn snapshot_404_is_none() {
        let url = format!("https://api.test/v1/live/{ID}");
        let http = FakeHttp::default().reply(&url, 404, "");
        assert_eq!(snapshot(&http, ORIGIN, ID).unwrap(), None);
    }

    #[test]
    fn snapshot_200_returns_the_body() {
        let url = format!("https://api.test/v1/live/{ID}");
        let http = FakeHttp::default().reply(&url, 200, r#"{"id":"x","clock":"Q3 04:12"}"#);
        assert_eq!(snapshot(&http, ORIGIN, ID).unwrap(), Some(serde_json::json!({"id":"x","clock":"Q3 04:12"})));
    }

    #[test]
    fn a_path_traversal_id_is_refused_and_no_request_is_made() {
        assert_eq!(snapshot(&NoCalls, ORIGIN, "../x").unwrap_err(), LiveError::BadId);
    }

    #[test]
    fn an_uppercase_uuid_is_refused_canonical_lowercase_only() {
        let upper = ID.to_ascii_uppercase();
        assert_eq!(snapshot(&NoCalls, ORIGIN, &upper).unwrap_err(), LiveError::BadId);
    }

    #[test]
    fn player_message_never_leaks_a_url_body_or_status_code() {
        use crate::player_sentence::assert_player_safe;
        for err in [LiveError::BadId, LiveError::Unreachable, LiveError::Server(500), LiveError::BadReply] {
            let msg = err.player_message();
            assert_player_safe(msg);
            assert!(!msg.contains("api.test"), "{msg}");
            assert!(!msg.contains("v1/live"), "{msg}");
        }
    }
}
