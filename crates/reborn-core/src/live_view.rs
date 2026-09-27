//! A typed client for the backend's live and stats-history endpoints: `GET /v1/live` (the "Live
//! now" list) and `GET /v1/live/{id}` (one match's snapshot, for a native match view) are public
//! -- no bearer, no sign-in. [`player_games`] and [`game_record`] back the Stats tab instead: they
//! read a player's own game history and one game's full record, signed with that player's bearer
//! token when the caller has one. All four are a thin `GET` plus status handling, built on the
//! same [`Http`] trait as every other client in this crate.
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

/// Why a call failed, in the five ways a caller actually has to handle differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveError {
    /// The id given to [`snapshot`] is not a canonical (lowercase) UUID. Checked before any
    /// request is made, so a malformed id -- someone else's URL fragment, `../x`, anything typed
    /// by hand -- never reaches the network at all.
    BadId,
    /// The server could not be reached: no route, DNS, timeout, refused.
    Unreachable,
    /// The server answered `429 Too Many Requests`. Kept apart from [`LiveError::Server`] so a
    /// page can back off for a full minute instead of retrying on its usual short schedule: a
    /// LAN party behind one NAT address shares one rate limit.
    RateLimited,
    /// The server answered with an error status (other than 429).
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
            LiveError::RateLimited => "Live games are busy right now. Trying again in a minute.",
            LiveError::Unreachable | LiveError::Server(_) | LiveError::BadReply => {
                "Live games couldn't be loaded right now."
            }
        }
    }
}

/// A non-success status as a [`LiveError`]: 429 is its own kind, everything else is `Server`.
fn status_error(status: u16) -> LiveError {
    if status == 429 {
        LiveError::RateLimited
    } else {
        LiveError::Server(status)
    }
}

/// Whether `id` is a canonical UUID: 36 characters, lowercase hex, hyphens at 8/13/18/23
/// (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`). Uppercase is refused rather than lowercased and
/// accepted, because the backend's own ids are always lowercase and a caller passing anything
/// else is not really quoting one back.
///
/// `pub` so `reborn-ui`'s `open_website` can reuse this exact rule for the live-match page link
/// rather than writing a second UUID check that could drift from this one.
pub fn is_canonical_uuid(id: &str) -> bool {
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
        return Err(status_error(reply.status));
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
        return Err(status_error(reply.status));
    }
    let body: Value = serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)?;
    Ok(Some(body))
}

/// Percent-encode the two characters a `before` cursor (an ISO-8601 timestamp) can contain that
/// are not safe verbatim in a query string: `:` and `+`. `before` always comes from a timestamp
/// this crate or the backend generated, never arbitrary user text, so this does not need to be a
/// general-purpose encoder.
fn percent_encode(s: &str) -> String {
    s.replace(':', "%3A").replace('+', "%2B")
}

/// `GET url` with an optional bearer, parsed as JSON on success. A non-2xx status becomes the
/// same [`LiveError`] mapping [`list`] uses; a transport failure is [`LiveError::Unreachable`].
fn get_json(http: &dyn Http, url: &str, token: Option<&str>) -> Result<Value, LiveError> {
    let reply = http.get_text_as(url, token).map_err(|_| LiveError::Unreachable)?;
    if !reply.is_success() {
        return Err(status_error(reply.status));
    }
    serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)
}

/// [`get_json`], but a `404` is `Ok(None)` rather than an error -- the same rule [`snapshot`]
/// uses for a match that has ended or never existed (or, here, one this viewer may not see).
fn get_json_opt(http: &dyn Http, url: &str, token: Option<&str>) -> Result<Option<Value>, LiveError> {
    let reply = http.get_text_as(url, token).map_err(|_| LiveError::Unreachable)?;
    if reply.status == 404 {
        return Ok(None);
    }
    if !reply.is_success() {
        return Err(status_error(reply.status));
    }
    Ok(Some(serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)?))
}

/// `GET {origin}/v1/players/{player_id}/games`: the player's own games, newest first, with the
/// fields the Stats tab's list draws (mode, teams, status, score, result, `plays_status`, ...).
/// `limit` is clamped to `1..=200` rather than rejected outright, since a caller-side mistake
/// (`0`, a huge number) should not become a hard error for a screen that just wants "as many as
/// sensible". `before` is an opaque cursor -- the `captured_at` of the last game already shown --
/// percent-encoded and passed straight through. `token`, when `Some`, becomes the `Authorization:
/// Bearer` header this endpoint needs to tell one player's games from another's.
pub fn player_games(
    http: &dyn Http,
    origin: &str,
    token: Option<&str>,
    player_id: &str,
    limit: u32,
    before: Option<&str>,
) -> Result<Value, LiveError> {
    if !is_canonical_uuid(player_id) {
        return Err(LiveError::BadId);
    }
    let mut url = format!(
        "{}/v1/players/{player_id}/games?limit={}",
        origin.trim_end_matches('/'),
        limit.clamp(1, 200)
    );
    if let Some(b) = before {
        url.push_str("&before=");
        url.push_str(&percent_encode(b));
    }
    let body = get_json(http, &url, token)?;
    Ok(body.get("games").cloned().unwrap_or_else(|| Value::Array(Vec::new())))
}

/// One game's full record: the detail `GET {origin}/v1/games/{game_id}` answers, plus its plays,
/// box-score lines, and drives -- each read only when `game.plays_status == "verified"`, since an
/// unverified game's per-play detail is not final yet and the three sub-routes 404 for it anyway.
/// A `404` on the detail itself is `Ok(None)`: the game does not exist, or this viewer (by
/// `token`) may not see it.
pub fn game_record(http: &dyn Http, origin: &str, token: Option<&str>, game_id: &str) -> Result<Option<Value>, LiveError> {
    if !is_canonical_uuid(game_id) {
        return Err(LiveError::BadId);
    }
    let base = format!("{}/v1/games/{game_id}", origin.trim_end_matches('/'));
    let detail = match get_json_opt(http, &base, token)? {
        Some(d) => d,
        None => return Ok(None),
    };
    let game = detail.get("game").cloned().unwrap_or_else(|| detail.clone());
    let verified = game.get("plays_status").and_then(Value::as_str) == Some("verified");
    let part = |path: &str, key: &str| -> Result<Value, LiveError> {
        if !verified {
            return Ok(Value::Array(Vec::new()));
        }
        let body = get_json(http, &format!("{base}/{path}"), token)?;
        Ok(body.get(key).cloned().unwrap_or_else(|| Value::Array(Vec::new())))
    };
    Ok(Some(serde_json::json!({
        "game": game,
        "plays": part("plays", "plays")?,
        "lines": part("boxscore", "lines")?,
        "drives": part("drives", "drives")?,
    })))
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
    fn a_429_is_a_rate_limit_not_a_generic_server_error() {
        let http = FakeHttp::default().reply("https://api.test/v1/live", 429, "");
        assert_eq!(list(&http, ORIGIN).unwrap_err(), LiveError::RateLimited);
        let url = format!("https://api.test/v1/live/{ID}");
        let http = FakeHttp::default().reply(&url, 429, "");
        assert_eq!(snapshot(&http, ORIGIN, ID).unwrap_err(), LiveError::RateLimited);
    }

    #[test]
    fn player_message_never_leaks_a_url_body_or_status_code() {
        use crate::player_sentence::assert_player_safe;
        for err in [LiveError::BadId, LiveError::Unreachable, LiveError::Server(500), LiveError::BadReply, LiveError::RateLimited] {
            let msg = err.player_message();
            assert_player_safe(msg);
            assert!(!msg.contains("api.test"), "{msg}");
            assert!(!msg.contains("v1/live"), "{msg}");
        }
    }

    #[test]
    fn percent_encode_escapes_colon_and_plus_only() {
        assert_eq!(percent_encode("2026-09-27T00:00:00Z"), "2026-09-27T00%3A00%3A00Z");
        assert_eq!(percent_encode("2026-09-27T00:00:00+05:00"), "2026-09-27T00%3A00%3A00%2B05%3A00");
    }

    #[test]
    fn player_games_reads_the_list_with_the_bearer_token_and_refuses_a_bad_id() {
        let url = format!("https://api.test/v1/players/{ID}/games?limit=50");
        let http = FakeHttp::default().reply(
            &url,
            200,
            r#"{"ok":true,"games":[{"id":"g1","score":[36,6],"result":"loss","plays_status":"verified"}]}"#,
        );
        let got = player_games(&http, ORIGIN, Some("tok"), ID, 50, None).unwrap();
        assert_eq!(got[0]["result"], "loss");
        assert_eq!(http.gets.lock().unwrap()[0], (url, Some("tok".to_string())), "the bearer token is sent as-is");

        assert_eq!(
            player_games(&NoCalls, ORIGIN, None, "../x", 50, None).unwrap_err(),
            LiveError::BadId,
            "a bad id is refused before any request is made"
        );
    }

    #[test]
    fn player_games_clamps_the_limit_and_percent_encodes_before() {
        let url = format!("https://api.test/v1/players/{ID}/games?limit=200&before=2026-09-27T00%3A00%3A00Z");
        let http = FakeHttp::default().reply(&url, 200, r#"{"ok":true,"games":[]}"#);
        let got = player_games(&http, ORIGIN, None, ID, 9_999, Some("2026-09-27T00:00:00Z")).unwrap();
        assert_eq!(got, serde_json::json!([]));
        assert_eq!(http.gets.lock().unwrap()[0].0, url);
    }

    #[test]
    fn player_games_missing_field_is_an_empty_array_not_a_bad_reply() {
        let url = format!("https://api.test/v1/players/{ID}/games?limit=50");
        let http = FakeHttp::default().reply(&url, 200, r#"{"ok":true}"#);
        assert_eq!(player_games(&http, ORIGIN, None, ID, 50, None).unwrap(), serde_json::json!([]));
    }

    #[test]
    fn game_record_is_none_on_404() {
        let url = format!("https://api.test/v1/games/{ID}");
        let http = FakeHttp::default().reply(&url, 404, r#"{"ok":false}"#);
        assert_eq!(game_record(&http, ORIGIN, None, ID).unwrap(), None);
    }

    #[test]
    fn game_record_reads_only_the_detail_for_an_unverified_game() {
        let url = format!("https://api.test/v1/games/{ID}");
        let http = FakeHttp::default().reply(&url, 200, r#"{"ok":true,"game":{"id":"g","plays_status":"absent","teams":[]}}"#);
        let v = game_record(&http, ORIGIN, None, ID).unwrap().unwrap();
        assert_eq!(v["game"]["id"], "g");
        assert_eq!(v["plays"].as_array().unwrap().len(), 0);
        assert_eq!(v["lines"].as_array().unwrap().len(), 0);
        assert_eq!(v["drives"].as_array().unwrap().len(), 0);
        assert_eq!(http.gets.lock().unwrap().len(), 1, "an unverified game reads only the detail");
    }

    #[test]
    fn game_record_reads_plays_box_and_drives_for_a_verified_game() {
        let base = format!("https://api.test/v1/games/{ID}");
        let http = FakeHttp::default()
            .reply(&base, 200, r#"{"ok":true,"game":{"id":"g","plays_status":"verified","teams":[]}}"#)
            .reply(&format!("{base}/plays"), 200, r#"{"ok":true,"plays":[{"body":{"seq":1}}]}"#)
            .reply(&format!("{base}/boxscore"), 200, r#"{"ok":true,"lines":[{"digp":1}]}"#)
            .reply(&format!("{base}/drives"), 200, r#"{"ok":true,"drives":[{"side":0}]}"#);
        let v = game_record(&http, ORIGIN, Some("tok"), ID).unwrap().unwrap();
        assert_eq!(v["plays"][0]["body"]["seq"], 1);
        assert_eq!(v["lines"][0]["digp"], 1);
        assert_eq!(v["drives"][0]["side"], 0);
        let gets = http.gets.lock().unwrap();
        assert_eq!(gets.len(), 4, "the detail plus the three sub-routes");
        assert!(gets.iter().all(|(_, bearer)| bearer.as_deref() == Some("tok")), "every request carries the token");
    }

    #[test]
    fn game_record_refuses_a_bad_id_before_any_request() {
        assert_eq!(game_record(&NoCalls, ORIGIN, None, "../x").unwrap_err(), LiveError::BadId);
    }
}
