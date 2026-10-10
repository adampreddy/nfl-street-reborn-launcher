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
pub(crate) fn status_error(status: u16) -> LiveError {
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
/// against [`is_canonical_uuid`] before anything is sent. `token`, when present, goes as the `Authorization: Bearer`
/// header (never logged, never in an error).
pub fn snapshot(http: &dyn Http, origin: &str, token: Option<&str>, match_id: &str) -> Result<Option<Value>, LiveError> {
    if !is_canonical_uuid(match_id) {
        return Err(LiveError::BadId);
    }
    let url = format!("{}/v1/live/{}", origin.trim_end_matches('/'), match_id);
    // A league match answers only its two seated players, with the device token as a bearer (backend contract). A
    // retail match is public: with no token the request is the plain GET it always was.
    let reply = match token.filter(|t| !t.trim().is_empty()) {
        Some(t) => http.get_text_as(&url, Some(t)),
        None => http.get_text(&url),
    }
    .map_err(|_| LiveError::Unreachable)?;
    if reply.status == 404 {
        return Ok(None);
    }
    if !reply.is_success() {
        return Err(status_error(reply.status));
    }
    let body: Value = serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)?;
    Ok(Some(body))
}

/// Percent-encode a `before` cursor for a query string: every byte outside the RFC 3986
/// unreserved set (`A-Z a-z 0-9 - . _ ~`) becomes `%XX` (uppercase hex, UTF-8 bytes). The cursor
/// is normally an ISO-8601 timestamp, but it is a string the page hands back, so `&`, `#`, `=`,
/// spaces and anything else are encoded too and can never add or cut a query parameter.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
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
/// sensible". `before` is an opaque cursor -- the `first_seen_at` of the last game already shown,
/// the column the server paginates and orders by (`reborn-backend-pbp/src/players.js`:
/// `g.first_seen_at < $3 ORDER BY g.first_seen_at DESC`) -- percent-encoded and passed straight
/// through. `token`, when `Some`, becomes the `Authorization: Bearer` header this endpoint needs
/// to tell one player's games from another's.
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
/// box-score lines, and drives -- each read only when `game.plays_status == "verified"`. That
/// gate is enforced here, not by the server: `/boxscore` and `/drives` answer `200` with `[]` for
/// an unverified game, and `/plays` returns whatever play rows already exist regardless of
/// verification. This client reads those three sub-routes only for a verified game so an
/// unverified game's not-yet-final play data never reaches the Stats view.
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

/// `GET {origin}/v1/me/dashboard`: the signed-in player's Stats page (every game, offline included).
pub fn dashboard(http: &dyn Http, origin: &str, token: &str) -> Result<Value, LiveError> {
    get_json(http, &format!("{}/v1/me/dashboard", origin.trim_end_matches('/')), Some(token))
}

/// `GET {origin}/v1/leaderboard?limit={limit}[&offset={offset}]`: the ranked leaderboard's rows from rank `offset + 1`
/// (the Online page's card reads the top, launcher spec 2026-10-02 §4; the full leaderboard pages, 10-08) and, with a
/// token, the viewer's own row. `limit` is clamped to `1..=50`; an `offset` of 0 is not sent, so the card's request is
/// exactly what it always was. A server that does not page ignores `offset` and answers the top (its answer then has no
/// `offset`). A `404` is `Ok(None)`: a backend without the route (or with matchmaking off) has no leaderboard to show,
/// which the card reads as "Rankings start soon."
pub fn leaderboard(http: &dyn Http, origin: &str, token: Option<&str>, limit: u32, offset: u32) -> Result<Option<Value>, LiveError> {
    let mut url = format!("{}/v1/leaderboard?limit={}", origin.trim_end_matches('/'), limit.clamp(1, 50));
    if offset > 0 {
        url.push_str(&format!("&offset={}", offset.min(1_000_000)));
    }
    get_json_opt(http, &url, token)
}

/// `GET {origin}/v1/players/{id}/profile`: a player's public, online-only profile, or with `games_before` one more
/// page of their match log. No token: the profile is the same for everyone.
pub fn player_profile(http: &dyn Http, origin: &str, player_id: &str, games_before: Option<&str>) -> Result<Option<Value>, LiveError> {
    if !is_canonical_uuid(player_id) {
        return Err(LiveError::BadId);
    }
    let mut url = format!("{}/v1/players/{player_id}/profile", origin.trim_end_matches('/'));
    if let Some(b) = games_before {
        url.push_str("?section=matches&limit=50&before=");
        url.push_str(&percent_encode(b));
    }
    get_json_opt(http, &url, None)
}

/// `POST {origin}/v1/me/plays-as`: the team the player's profile shows, or `None` for "most played".
pub fn set_plays_as(http: &dyn Http, origin: &str, token: &str, tgid: Option<u16>) -> Result<Value, LiveError> {
    let body = serde_json::to_vec(&serde_json::json!({ "tgid": tgid })).map_err(|_| LiveError::BadReply)?;
    let reply = http
        .post_json(&format!("{}/v1/me/plays-as", origin.trim_end_matches('/')), &body, Some(token))
        .map_err(|_| LiveError::Unreachable)?;
    if !reply.is_success() {
        return Err(status_error(reply.status));
    }
    serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::{FakeHttp, PanicHttp};
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
        assert_eq!(snapshot(&http, ORIGIN, None, ID).unwrap(), None);
    }

    #[test]
    fn a_signed_in_snapshot_sends_the_bearer_and_a_signed_out_one_sends_none() {
        let url = format!("https://api.test/v1/live/{ID}");
        let http = FakeHttp::default().reply(&url, 200, r#"{"id":"x"}"#);
        assert!(snapshot(&http, ORIGIN, Some("tok"), ID).unwrap().is_some());
        assert_eq!(http.gets.lock().unwrap().as_slice(), &[(url.clone(), Some("tok".to_string()))]);
        // No token (or a blank one): the plain public GET, no Authorization header at all.
        for t in [None, Some(""), Some("  ")] {
            let http = FakeHttp::default().reply(&url, 200, r#"{"id":"x"}"#);
            assert!(snapshot(&http, ORIGIN, t, ID).unwrap().is_some(), "retail works without a token");
            assert!(http.gets.lock().unwrap().is_empty(), "no authenticated request was made");
        }
        // A seated-players-only answer the token did not satisfy is an ordinary 'nothing to show' / error, not a leak.
        let http = FakeHttp::default().reply(&url, 404, "");
        assert_eq!(snapshot(&http, ORIGIN, Some("tok"), ID).unwrap(), None);
    }

    #[test]
    fn the_snapshot_path_never_logs_or_prints_the_token() {
        for (name, src) in [("core", include_str!("live_view.rs")), ("ui", include_str!("../../reborn-ui/src/live_view.rs"))] {
            let from = src.find(&format!("{}{}", "pub fn snapshot", "(")).or_else(|| src.find(&format!("{}{}", "pub async fn live_", "match("))).unwrap();
            let body = &src[from..from + src[from..].find("
}
").unwrap()];
            for banned in ["tracing::", "println!", "eprintln!", "format!(\"{token", "{token:?}", "{t:?}", "dbg!"] {
                assert!(!body.contains(banned), "{name}: the snapshot path must not log ({banned})");
            }
        }
    }

    #[test]
    fn snapshot_200_returns_the_body() {
        let url = format!("https://api.test/v1/live/{ID}");
        let http = FakeHttp::default().reply(&url, 200, r#"{"id":"x","clock":"Q3 04:12"}"#);
        assert_eq!(snapshot(&http, ORIGIN, None, ID).unwrap(), Some(serde_json::json!({"id":"x","clock":"Q3 04:12"})));
    }

    #[test]
    fn a_path_traversal_id_is_refused_and_no_request_is_made() {
        assert_eq!(snapshot(&NoCalls, ORIGIN, None, "../x").unwrap_err(), LiveError::BadId);
    }

    #[test]
    fn an_uppercase_uuid_is_refused_canonical_lowercase_only() {
        let upper = ID.to_ascii_uppercase();
        assert_eq!(snapshot(&NoCalls, ORIGIN, None, &upper).unwrap_err(), LiveError::BadId);
    }

    #[test]
    fn a_429_is_a_rate_limit_not_a_generic_server_error() {
        let http = FakeHttp::default().reply("https://api.test/v1/live", 429, "");
        assert_eq!(list(&http, ORIGIN).unwrap_err(), LiveError::RateLimited);
        let url = format!("https://api.test/v1/live/{ID}");
        let http = FakeHttp::default().reply(&url, 429, "");
        assert_eq!(snapshot(&http, ORIGIN, None, ID).unwrap_err(), LiveError::RateLimited);
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
    fn percent_encode_escapes_every_byte_outside_unreserved() {
        assert_eq!(percent_encode("2026-09-27T00:00:00Z"), "2026-09-27T00%3A00%3A00Z");
        assert_eq!(percent_encode("2026-09-27T00:00:00+05:00"), "2026-09-27T00%3A00%3A00%2B05%3A00");
        assert_eq!(percent_encode("2026-09-27T00:00:00.123Z"), "2026-09-27T00%3A00%3A00.123Z", "`.` is unreserved");
        assert_eq!(percent_encode("a b&c=d#e/f?g%h"), "a%20b%26c%3Dd%23e%2Ff%3Fg%25h");
        assert_eq!(percent_encode("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(percent_encode("\u{e9}"), "%C3%A9", "non-ASCII is encoded as its UTF-8 bytes");
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
    fn player_games_500_is_a_server_error_the_same_way_list_maps_it() {
        let url = format!("https://api.test/v1/players/{ID}/games?limit=50");
        let http = FakeHttp::default().reply(&url, 500, "");
        assert_eq!(player_games(&http, ORIGIN, None, ID, 50, None).unwrap_err(), LiveError::Server(500));
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

    #[test]
    fn dashboard_sends_the_token_and_returns_the_body() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/me/dashboard"), 200, r#"{"ok":true,"games":[]}"#);
        let v = dashboard(&http, ORIGIN, "tok").unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(http.last_bearer().as_deref(), Some("tok"));
    }

    #[test]
    fn leaderboard_reads_the_board_sends_the_token_when_there_is_one_and_404_is_none() {
        let url = "https://api.test/v1/leaderboard?limit=10";
        let http = FakeHttp::default().reply(url, 200, r#"{"ok":true,"enabled":true,"total":2,"rows":[{"rank":1}],"you":null}"#);
        let got = leaderboard(&http, ORIGIN, Some("tok"), 10, 0).unwrap().unwrap();
        assert_eq!(got["total"], 2);
        assert_eq!(http.last_bearer().as_deref(), Some("tok"));
        // No such route yet (an older backend, matchmaking off): nothing to show, not an error.
        let none = FakeHttp::default().reply(url, 404, "");
        assert_eq!(leaderboard(&none, ORIGIN, None, 10, 0).unwrap(), None);
        let clamped = FakeHttp::default().reply("https://api.test/v1/leaderboard?limit=50", 200, "{}");
        assert!(leaderboard(&clamped, ORIGIN, None, 500, 0).unwrap().is_some(), "a silly limit is clamped, not refused");
        let broken = FakeHttp::default().reply(url, 500, "");
        assert_eq!(leaderboard(&broken, ORIGIN, None, 10, 0).unwrap_err(), LiveError::Server(500));
    }

    #[test]
    fn leaderboard_pages_with_an_offset_and_leaves_the_cards_request_unchanged() {
        let paged = FakeHttp::default().reply("https://api.test/v1/leaderboard?limit=50&offset=100", 200, r#"{"ok":true,"offset":100}"#);
        assert_eq!(leaderboard(&paged, ORIGIN, None, 50, 100).unwrap().unwrap()["offset"], 100);
        let far = FakeHttp::default().reply("https://api.test/v1/leaderboard?limit=50&offset=1000000", 200, "{}");
        assert!(leaderboard(&far, ORIGIN, None, 50, u32::MAX).unwrap().is_some(), "a silly offset is clamped, not refused");
    }

    #[test]
    fn player_profile_404_is_none_bad_id_is_refused_and_paging_builds_the_query() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/players/{ID}/profile"), 404, "{}");
        assert_eq!(player_profile(&http, ORIGIN, ID, None).unwrap(), None);
        assert_eq!(player_profile(&PanicHttp, ORIGIN, "../x", None), Err(LiveError::BadId));
        let url = format!("{ORIGIN}/v1/players/{ID}/profile?section=matches&limit=50&before=2026-09-30T01%3A02%3A03.000Z");
        let http = FakeHttp::default().reply(&url, 200, r#"{"ok":true,"games":[]}"#);
        assert!(player_profile(&http, ORIGIN, ID, Some("2026-09-30T01:02:03.000Z")).unwrap().is_some());
    }

    #[test]
    fn set_plays_as_posts_the_tgid_or_null() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/me/plays-as"), 200, r#"{"ok":true,"plays_as":null}"#);
        set_plays_as(&http, ORIGIN, "tok", None).unwrap();
        assert_eq!(http.last_body().as_deref(), Some(br#"{"tgid":null}"#.as_slice()));
    }
}
