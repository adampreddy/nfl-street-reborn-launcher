//! The League tab's data commands (spec 2026-10-08 section 3): whether to draw the tab (`league_member`, from the cache
//! only) and the team home (`league_home`). The League edition's commands (install, offline, friendly, league room) are
//! Plan 3b's module; joining and cancelling a room are the existing `mm_room_join` / `mm_room_action`.
//!
//! Every failure is `{ kind, message }` (`LiveCommandError`), with `message` one of the fixed sentences below.

use reborn_core::identity;
use reborn_core::league_home::{self, LeagueCache};
use reborn_core::live_view::LiveError;
use reborn_core::platform::Platform;

use crate::live_view::LiveCommandError;

const NOT_LOADED: &str = "League data couldn't be loaded right now.";
const BUSY: &str = "The league server is busy. Trying again in a minute.";

fn err(kind: &'static str, sentence: &str) -> LiveCommandError {
    LiveCommandError { kind, message: sentence.to_owned() }
}

fn home_error(e: LiveError) -> LiveCommandError {
    match e {
        LiveError::RateLimited => err("rate_limited", BUSY),
        _ => err("unavailable", NOT_LOADED),
    }
}

fn signed_out() -> serde_json::Value {
    serde_json::json!({ "signed_in": false })
}

fn mark_signed_in(v: &mut serde_json::Value) {
    v["signed_in"] = serde_json::Value::Bool(true);
}

/// The bearer, or `None` when signed out (no facts file, or no vault token).
fn bearer(p: &Platform) -> Option<String> {
    identity::status(p).ok().flatten()?;
    identity::token(p).ok().flatten().filter(|t| !t.trim().is_empty())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Whether the nav draws the League link: the cached answer only, no network, so every page's first frame has it.
#[tauri::command]
pub async fn league_member() -> bool {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        if bearer(&p).is_none() {
            return false;
        }
        league_home::cache_path(&p).map(|path| league_home::load_cache(&path).member).unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// `GET /v1/league/me`, with the cached fallback (`league_home::resolve`). Signed out is a shape, not an error. A 503
/// (`league_unavailable`) is an outage, never "not a member": `resolve` falls back to the cached member.
#[tauri::command]
pub async fn league_home() -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        let Some(token) = bearer(&p) else { return Ok(signed_out()) };
        let origin = identity::origin(&p).map_err(|_| err("unavailable", NOT_LOADED))?;
        let path = league_home::cache_path(&p).ok();
        let cache = path.as_deref().map(league_home::load_cache).unwrap_or_else(LeagueCache::default);
        let fetched = league_home::fetch_home(&crate::http_client(), &origin, &token);
        let (out, next) = league_home::resolve(fetched, &cache, now_ms());
        if let (Some(path), Some(next)) = (path.as_deref(), next) {
            league_home::save_cache(path, &next);
        }
        match out {
            Ok(mut v) => {
                mark_signed_in(&mut v);
                Ok(v)
            }
            Err(LiveError::Server(401)) => Ok(signed_out()),
            Err(e) => Err(home_error(e)),
        }
    })
    .await
    .map_err(|_| err("unavailable", NOT_LOADED))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_home_failure_reads_as_a_league_sentence() {
        let cases = [
            (LiveError::RateLimited, "rate_limited", BUSY),
            (LiveError::Server(503), "unavailable", NOT_LOADED),
            (LiveError::Unreachable, "unavailable", NOT_LOADED),
            (LiveError::BadReply, "unavailable", NOT_LOADED),
        ];
        for (e, kind, message) in cases {
            let v = serde_json::to_value(home_error(e.clone())).unwrap();
            assert_eq!((v["kind"].as_str().unwrap(), v["message"].as_str().unwrap()), (kind, message), "{e:?}");
        }
    }

    #[test]
    fn the_home_shape_marks_signed_in() {
        assert_eq!(signed_out(), serde_json::json!({ "signed_in": false }));
        let mut v = serde_json::json!({ "member": true });
        mark_signed_in(&mut v);
        assert_eq!(v["signed_in"], true);
    }

    /// The same rule live_view.rs holds itself to: no command hands a page an error's Display.
    #[test]
    fn no_command_here_hands_a_page_raw_error_text() {
        let src = include_str!("league_tab.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        let compact: String = production.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(!compact.contains("e.to_string()"));
    }
}
