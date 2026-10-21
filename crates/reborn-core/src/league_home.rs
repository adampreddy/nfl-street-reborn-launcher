//! The League tab's data client: `GET /v1/league/me` (the signed-in member's team home, or `{ member: false }`).
//! Values pass through as [`serde_json::Value`] for the same reason as `live_view`: the backend can add a field without
//! a launcher release. Errors are [`LiveError`], so nothing a screen shows can carry a URL, a body or a token.
//! The League edition (install, rooms, seeds) is Plan 3b's, not this module's.
//!
//! Named `league_home`, not `league`: `lib.rs` keeps a test that the retired `league.rs` uploader stays gone.
//!
//! The last good answer is kept in `league.json` beside `identity.json`. The nav reads it without the network, so
//! the League link is there on the first frame, and a member whose league server is down still sees their team,
//! marked `stale`. Signing out removes it (`identity::clear_token`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::live_view::{status_error, LiveError};
use crate::net::Http;
use crate::platform::{DataDir, Platform};

const CACHE_FILE: &str = "league.json";

/// What `league.json` holds. The default is "not a member", the answer for no file, an unreadable one, and signed out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LeagueCache {
    pub member: bool,
    #[serde(default)]
    pub home: Option<Value>,
    #[serde(default)]
    pub fetched_at_ms: u64,
}

pub fn cache_path(platform: &Platform) -> crate::platform::Result<PathBuf> {
    Ok(platform.data_dir(DataDir::Config)?.join(CACHE_FILE))
}

/// A cache whose `home` is not a JSON object (hand-edited, or from a future shape) is no cache at all.
pub fn load_cache(path: &Path) -> LeagueCache {
    let cache: LeagueCache = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if cache.home.as_ref().is_some_and(|h| !h.is_object()) {
        return LeagueCache::default();
    }
    cache
}

/// Best effort: a cache that cannot be written only costs the next first frame its League link. Written to a
/// sibling `.tmp` and renamed over the target, so a crash mid-write never leaves a truncated `league.json` (which
/// would read as "not a member" and drop the tab during an outage).
pub fn save_cache(path: &Path, cache: &LeagueCache) {
    let Ok(bytes) = serde_json::to_vec(cache) else { return };
    let tmp = path.with_file_name(format!("{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or(CACHE_FILE)));
    let result = (|| -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        tracing::warn!("league: the cache could not be written at {}: {e}", path.display());
    }
}

fn url(origin: &str, path: &str) -> String {
    format!("{}{}", origin.trim_end_matches('/'), path)
}

/// `GET {origin}/v1/league/me`. A reply that is not JSON, or has no boolean `member`, is not this endpoint's answer.
pub fn fetch_home(http: &dyn Http, origin: &str, token: &str) -> Result<Value, LiveError> {
    let reply = http.get_text_as(&url(origin, "/v1/league/me"), Some(token)).map_err(|_| LiveError::Unreachable)?;
    if !reply.is_success() {
        return Err(status_error(reply.status));
    }
    let v: Value = serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)?;
    if !v.get("member").is_some_and(Value::is_boolean) {
        return Err(LiveError::BadReply);
    }
    Ok(v)
}

/// The page's answer and the cache to write (`None`: leave it). A fresh answer is returned with `stale: false` and
/// cached. A failure falls back to a cached member's last answer, `stale: true`. A 401 never does: the session is
/// over, so the cache resets and the tab goes.
pub fn resolve(fetched: Result<Value, LiveError>, cache: &LeagueCache, now_ms: u64) -> (Result<Value, LiveError>, Option<LeagueCache>) {
    match fetched {
        Ok(mut v) => {
            let member = v["member"].as_bool().unwrap_or(false);
            let next = LeagueCache { member, home: member.then(|| v.clone()), fetched_at_ms: now_ms };
            // The backend marks its saved copy stale while the league site is down (backend migration 033): keep it.
            let stale = v["stale"] == Value::Bool(true);
            v["stale"] = Value::Bool(stale);
            (Ok(v), Some(next))
        }
        Err(LiveError::Server(401)) => (Err(LiveError::Server(401)), Some(LeagueCache::default())),
        Err(e) => match (&cache.home, cache.member) {
            (Some(home), true) if home.is_object() => {
                let mut v = home.clone();
                v["stale"] = Value::Bool(true);
                v["fetched_at_ms"] = Value::from(cache.fetched_at_ms);
                (Ok(v), None)
            }
            _ => (Err(e), None),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;

    const ORIGIN: &str = "https://api.test";
    const MEMBER: &str = r#"{"member":true,"team":{"id":485,"name":"Atlanta Falcons"},"matchup":null}"#;

    #[test]
    fn fetch_sends_the_bearer_and_returns_the_body() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/league/me"), 200, MEMBER);
        let v = fetch_home(&http, ORIGIN, "tok").unwrap();
        assert_eq!(v["team"]["id"], 485);
        assert_eq!(http.last_bearer().as_deref(), Some("tok"));
    }

    #[test]
    fn a_body_without_a_member_flag_is_a_bad_reply() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/league/me"), 200, r#"{"team":{}}"#);
        assert_eq!(fetch_home(&http, ORIGIN, "tok").unwrap_err(), LiveError::BadReply);
    }

    #[test]
    fn a_body_that_is_not_json_is_a_bad_reply() {
        // The league site can leak an HTML warning through the backend (display_errors); never trust a non-JSON 200.
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/league/me"), 200, "<br /><b>Warning</b>");
        assert_eq!(fetch_home(&http, ORIGIN, "tok").unwrap_err(), LiveError::BadReply);
    }

    #[test]
    fn statuses_map_like_every_other_client() {
        for (status, want) in [(401, LiveError::Server(401)), (429, LiveError::RateLimited), (503, LiveError::Server(503))] {
            let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/league/me"), status, "{}");
            assert_eq!(fetch_home(&http, ORIGIN, "tok").unwrap_err(), want, "{status}");
        }
        assert_eq!(fetch_home(&FakeHttp::default(), ORIGIN, "tok").unwrap_err(), LiveError::Unreachable);
    }

    #[test]
    fn a_fresh_answer_is_returned_unstale_and_cached() {
        let fresh: Value = serde_json::from_str(MEMBER).unwrap();
        let (out, next) = resolve(Ok(fresh), &LeagueCache::default(), 7);
        let out = out.unwrap();
        assert_eq!(out["stale"], false);
        let next = next.unwrap();
        assert!(next.member);
        assert_eq!(next.fetched_at_ms, 7);
        assert_eq!(next.home.unwrap()["team"]["id"], 485);
    }

    /// The backend serves a member's last good answer, marked stale, while the league site is down (backend 033):
    /// it stays stale on the page, and the member is cached so the next start draws the tab.
    #[test]
    fn a_stale_server_answer_stays_stale_and_caches_the_member() {
        let (out, next) = resolve(Ok(serde_json::json!({"member": true, "stale": true, "team": {"id": 493}})), &LeagueCache::default(), 5);
        assert_eq!(out.unwrap()["stale"], true);
        let next = next.unwrap();
        assert!(next.member);
        assert_eq!(next.home.unwrap()["team"]["id"], 493);
    }

    #[test]
    fn a_non_member_answer_clears_the_cached_team() {
        let cache = LeagueCache { member: true, home: Some(serde_json::json!({"member":true})), fetched_at_ms: 1 };
        let (out, next) = resolve(Ok(serde_json::json!({"member": false})), &cache, 9);
        assert_eq!(out.unwrap()["member"], false);
        assert_eq!(next.unwrap(), LeagueCache { member: false, home: None, fetched_at_ms: 9 });
    }

    #[test]
    fn a_failure_falls_back_to_the_cached_member() {
        let cache = LeagueCache { member: true, home: Some(serde_json::from_str(MEMBER).unwrap()), fetched_at_ms: 1 };
        let (out, next) = resolve(Err(LiveError::Server(503)), &cache, 9);
        let out = out.unwrap();
        assert_eq!(out["stale"], true);
        assert_eq!(out["fetched_at_ms"], 1);
        assert!(next.is_none(), "a failure never rewrites the cache");
    }

    #[test]
    fn a_failure_with_no_cached_member_is_the_error() {
        let (out, next) = resolve(Err(LiveError::Unreachable), &LeagueCache::default(), 9);
        assert_eq!(out.unwrap_err(), LiveError::Unreachable);
        assert!(next.is_none());
    }

    #[test]
    fn a_401_is_never_covered_by_the_cache() {
        let cache = LeagueCache { member: true, home: Some(serde_json::from_str(MEMBER).unwrap()), fetched_at_ms: 1 };
        let (out, next) = resolve(Err(LiveError::Server(401)), &cache, 9);
        assert_eq!(out.unwrap_err(), LiveError::Server(401));
        assert_eq!(next.unwrap(), LeagueCache::default(), "the session is over: forget the tab");
    }

    #[test]
    fn the_cache_round_trips_and_a_bad_file_is_not_a_member() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("league.json");
        assert_eq!(load_cache(&path), LeagueCache::default());
        let c = LeagueCache { member: true, home: Some(serde_json::json!({"member":true})), fetched_at_ms: 3 };
        save_cache(&path, &c);
        assert_eq!(load_cache(&path), c);
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(load_cache(&path), LeagueCache::default());
    }

    #[test]
    fn a_leftover_tmp_file_does_not_confuse_load_and_save_replaces_it() {
        let dir = tempfile::tempdir().unwrap().keep();
        let path = dir.join("league.json");
        std::fs::write(dir.join("league.json.tmp"), b"{\"member\":tr").unwrap();
        assert_eq!(load_cache(&path), LeagueCache::default());
        let c = LeagueCache { member: true, home: Some(serde_json::json!({"member":true})), fetched_at_ms: 5 };
        save_cache(&path, &c);
        assert_eq!(load_cache(&path), c);
        assert!(!dir.join("league.json.tmp").exists());
        let nested = dir.join("a").join("league.json");
        save_cache(&nested, &c);
        assert_eq!(load_cache(&nested), c, "a missing parent dir is created");
    }

    #[test]
    fn a_cache_with_a_non_object_home_is_no_cache_and_never_panics_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("league.json");
        std::fs::write(&path, br#"{"member":true,"home":"oops","fetched_at_ms":1}"#).unwrap();
        assert_eq!(load_cache(&path), LeagueCache::default());
        let bad = LeagueCache { member: true, home: Some(serde_json::json!("oops")), fetched_at_ms: 1 };
        let (out, next) = resolve(Err(LiveError::Unreachable), &bad, 9);
        assert_eq!(out.unwrap_err(), LiveError::Unreachable);
        assert!(next.is_none());
    }
}
