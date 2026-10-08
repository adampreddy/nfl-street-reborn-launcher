//! Commands for the native "Live now" view: the list of matches currently live on the web
//! game, and one match's snapshot. Both wrap `reborn_core::live_view`, which is the typed
//! client for the backend's public `GET /v1/live` and `GET /v1/live/{id}` endpoints -- no
//! sign-in, no bearer token, so these commands only need the backend origin.
//!
//! Both run on `spawn_blocking`: the client underneath is a synchronous `ureq` call, and
//! nothing here may block the async runtime the rest of the launcher's commands share.
//!
//! Every failure is a [`LiveCommandError`]: a `kind` the page can act on (a rate limit backs
//! off for a full minute) and a `message` that is always a player sentence, never a raw
//! `Display` -- matching every other command that turns a `reborn_core` error into something a
//! page shows as is (see `main.rs`'s
//! `nothing_the_online_page_can_call_turns_a_core_error_into_raw_text`).

use reborn_core::identity;
use reborn_core::live_view::LiveError;
use reborn_core::platform::Platform;

/// What a page is told when a live command fails, serialized as `{ kind, message }`. The page's
/// `reason()` reads `message`, so an error drawn as is still reads as one player sentence.
///
/// `kind` is `"rate_limited"` for the backend's 429 (the page waits at least 60 s before asking
/// again) and `"unavailable"` for everything else (the page backs off from 5 s up to 60 s).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LiveCommandError {
    pub kind: &'static str,
    pub message: String,
}

/// The line a player sees when the live request never ran at all (the blocking task failed).
const NOT_LOADED: &str = "Live games couldn't be loaded right now.";

impl LiveCommandError {
    fn unavailable(message: &str) -> Self {
        Self { kind: "unavailable", message: message.to_string() }
    }
}

impl From<LiveError> for LiveCommandError {
    fn from(e: LiveError) -> Self {
        let kind = if e == LiveError::RateLimited { "rate_limited" } else { "unavailable" };
        Self { kind, message: e.player_message().to_string() }
    }
}

/// `GET /v1/live`: every match currently live on the web game, as the backend's own JSON.
#[tauri::command]
pub async fn live_matches() -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let http = crate::http_client();
        reborn_core::live_view::list(&http, &origin).map_err(LiveCommandError::from)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}

/// `GET /v1/live/{match_id}`: one match's snapshot, or `None` once it has ended or was never
/// live. `match_id` is validated as a canonical UUID before anything is sent -- see
/// `reborn_core::live_view::snapshot` -- so a stray fragment typed into the page can never reach
/// the network as a path segment.
#[tauri::command]
pub async fn live_match(match_id: String) -> Result<Option<serde_json::Value>, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let http = crate::http_client();
        reborn_core::live_view::snapshot(&http, &origin, &match_id).map_err(LiveCommandError::from)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}

fn my_games_view(player_id: Option<&str>, games: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "signed_in": player_id.is_some(), "player_id": player_id, "games": games })
}

/// Who the Stats tab may ask the server about: the player's id and bearer token, or `None` for
/// the signed-out shape. Both are needed -- an identity whose vault token is missing (or could
/// not be read) is signed out as far as this list goes, because an anonymous request would hide
/// the player's own offline games and still read as "signed in".
fn my_games_identity(player_id: Option<String>, token: Option<String>) -> Option<(String, String)> {
    match (player_id, token) {
        (Some(id), Some(tok)) if !tok.trim().is_empty() => Some((id, tok)),
        _ => None,
    }
}

/// The signed-in player's games from the server (Stats tab). Signed out is a shape, not an error.
#[tauri::command]
pub async fn stats_my_games(limit: Option<u32>, before: Option<String>) -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let player_id = identity::status(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?.map(|f| f.player.id);
        let token = if player_id.is_some() { identity::token(&p).unwrap_or(None) } else { None };
        let Some((player_id, token)) = my_games_identity(player_id, token) else {
            return Ok(my_games_view(None, serde_json::Value::Array(vec![])));
        };
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let http = crate::http_client();
        let games = reborn_core::live_view::player_games(&http, &origin, Some(&token), &player_id, limit.unwrap_or(50), before.as_deref())
            .map_err(LiveCommandError::from)?;
        Ok(my_games_view(Some(&player_id), games))
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}

/// One game's record for the native game view, with the player's token when signed in.
#[tauri::command]
pub async fn stats_game(game_id: String) -> Result<Option<serde_json::Value>, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let token = identity::token(&p).unwrap_or(None);
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let http = crate::http_client();
        reborn_core::live_view::game_record(&http, &origin, token.as_deref(), &game_id).map_err(LiveCommandError::from)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}

/// The Stats page (dashboard). Signed out is `{ signed_in: false }`, a shape rather than an error.
#[tauri::command]
pub async fn stats_dashboard() -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let signed_in = identity::status(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?.is_some();
        let token = if signed_in { identity::token(&p).unwrap_or(None) } else { None };
        let Some(token) = token else { return Ok(serde_json::json!({ "signed_in": false })) };
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let mut v = reborn_core::live_view::dashboard(&crate::http_client(), &origin, &token).map_err(LiveCommandError::from)?;
        v["signed_in"] = serde_json::Value::Bool(true);
        Ok(v)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}

/// The Online page's Leaderboard card (spec 2026-10-02 §4): the top ten, with the player's token when signed in so the
/// server can add their own row. `None` while the server has no leaderboard. Signed out is simply anonymous.
/// The full leaderboard page (10-08) asks for `limit` rows (at most 50) from rank `offset + 1`; the card sends neither.
#[tauri::command]
pub async fn leaderboard(limit: Option<u32>, offset: Option<u32>) -> Result<Option<serde_json::Value>, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let signed_in = identity::status(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?.is_some();
        let token = if signed_in { identity::token(&p).unwrap_or(None) } else { None };
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        reborn_core::live_view::leaderboard(&crate::http_client(), &origin, token.as_deref(), limit.unwrap_or(10), offset.unwrap_or(0))
            .map_err(LiveCommandError::from)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}

/// A player's public profile: `GET /v1/players/{id}/profile`, or with `before` one more page of
/// their match log. No token -- the profile is the same for every viewer, signed in or not -- and
/// `None` once the id does not exist.
#[tauri::command]
pub async fn player_profile(player_id: String, before: Option<String>) -> Result<Option<serde_json::Value>, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let http = crate::http_client();
        reborn_core::live_view::player_profile(&http, &origin, &player_id, before.as_deref()).map_err(LiveCommandError::from)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}

/// `POST /v1/me/plays-as`: which team the signed-in player's own profile shows, or `None` for
/// "most played". Unlike `stats_dashboard`, there is no signed-out shape here -- a player cannot
/// set their own display team without an identity to set it on -- so signed out is the same
/// `unavailable` error every other command that needs a token uses.
#[tauri::command]
pub async fn set_plays_as(tgid: Option<u16>) -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let signed_in = identity::status(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?.is_some();
        let token = if signed_in { identity::token(&p).unwrap_or(None) } else { None };
        let Some(token) = token else {
            return Err(LiveCommandError::unavailable("Sign in with Discord on the Play page first."));
        };
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        reborn_core::live_view::set_plays_as(&crate::http_client(), &origin, &token, tgid).map_err(LiveCommandError::from)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn my_games_view_is_the_signed_out_shape_without_an_identity() {
        let v = my_games_view(None, serde_json::Value::Array(vec![]));
        assert_eq!(v["signed_in"], false);
        assert!(v["player_id"].is_null());
        assert_eq!(v["games"].as_array().unwrap().len(), 0);
        let v = my_games_view(Some("p1"), serde_json::json!([{"id":"g"}]));
        assert_eq!(v["signed_in"], true);
        assert_eq!(v["player_id"], "p1");
        assert_eq!(v["games"][0]["id"], "g");
    }

    #[test]
    fn my_games_needs_both_an_identity_and_a_token() {
        assert_eq!(my_games_identity(None, None), None);
        assert_eq!(my_games_identity(None, Some("tok".into())), None);
        assert_eq!(my_games_identity(Some("p1".into()), None), None, "a missing vault token is signed out, not anonymous");
        assert_eq!(my_games_identity(Some("p1".into()), Some("  ".into())), None, "a blank token is no token");
        assert_eq!(my_games_identity(Some("p1".into()), Some("tok".into())), Some(("p1".to_string(), "tok".to_string())));
    }

    #[test]
    fn a_rate_limit_reaches_the_page_as_its_own_kind() {
        let json = serde_json::to_value(LiveCommandError::from(LiveError::RateLimited)).unwrap();
        assert_eq!(json["kind"], "rate_limited");
        assert_eq!(json["message"], LiveError::RateLimited.player_message());
    }

    #[test]
    fn every_other_failure_is_unavailable_with_the_player_sentence() {
        for e in [LiveError::Unreachable, LiveError::Server(500), LiveError::BadReply, LiveError::BadId] {
            let json = serde_json::to_value(LiveCommandError::from(e)).unwrap();
            assert_eq!(json["kind"], "unavailable", "{e:?}");
            assert_eq!(json["message"], e.player_message(), "{e:?}");
        }
    }

    /// A failed blocking task (a `JoinError`) is worded for a player too, never its own
    /// `Display` ("task 12 panicked" is not something a player can act on).
    #[test]
    fn no_command_here_hands_a_page_raw_error_text() {
        let src = include_str!("live_view.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        let compact: String = production.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(!compact.contains("|e|e.to_string()"), "a command still maps an error through its Display");
        assert!(compact.contains(".map_err(|_|LiveCommandError::unavailable(NOT_LOADED))"), "a JoinError is not worded for a player");
    }

    /// The live pages build image and "Open on web" URLs against the backend origin, and the CSP
    /// pins `img-src` to it; none of them can read `identity::DEFAULT_ORIGIN` at runtime, so this
    /// holds all three to the same string. The origin lives in one shared module (live-common.js)
    /// that both pages import, and neither page spells a backend origin of its own.
    #[test]
    fn the_live_pages_and_the_csp_use_the_default_origin() {
        let origin = identity::DEFAULT_ORIGIN;
        let common = include_str!("../ui/live-common.js");
        let conf = include_str!("../tauri.conf.json");
        assert!(common.contains(&format!("'{origin}'")), "live-common.js no longer uses {origin}");
        assert!(conf.contains(&format!("{origin}/live-assets/")), "tauri.conf.json's CSP no longer allows {origin}");
        for (name, page) in [
            ("match.js", include_str!("../ui/match.js")),
            ("live-now.js", include_str!("../ui/live-now.js")),
        ] {
            assert!(page.contains("from './live-common.js'"), "{name} no longer imports the shared origin");
            assert!(!page.contains("https://api."), "{name} spells a backend origin of its own");
        }
    }
}
