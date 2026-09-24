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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The live pages build image and "Watch on web" URLs against the backend origin, and the CSP
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
        for (name, page) in [("live.js", include_str!("../ui/live.js")), ("live-now.js", include_str!("../ui/live-now.js"))] {
            assert!(page.contains("from './live-common.js'"), "{name} no longer imports the shared origin");
            assert!(!page.contains("https://api."), "{name} spells a backend origin of its own");
        }
    }
}
