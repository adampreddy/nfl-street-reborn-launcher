//! Commands for the native "Live now" view: the list of matches currently live on the web
//! game, and one match's snapshot. Both wrap `reborn_core::live_view`, which is the typed
//! client for the backend's public `GET /v1/live` and `GET /v1/live/{id}` endpoints -- no
//! sign-in, no bearer token, so these commands only need the backend origin.
//!
//! Both run on `spawn_blocking`: the client underneath is a synchronous `ureq` call, and
//! nothing here may block the async runtime the rest of the launcher's commands share.
//!
//! Every failure is mapped through `player_message()`, never a raw `Display`, matching every
//! other command that turns a `reborn_core` error into a string a page can show as is (see
//! `main.rs`'s `nothing_the_online_page_can_call_turns_a_core_error_into_raw_text`, which holds
//! the Online page to the same rule this page is held to here).

use reborn_core::identity;
use reborn_core::platform::Platform;

/// `GET /v1/live`: every match currently live on the web game, as the backend's own JSON.
#[tauri::command]
pub async fn live_matches() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = identity::origin(&p).map_err(|e| e.player_message().to_string())?;
        let http = crate::http_client();
        reborn_core::live_view::list(&http, &origin).map_err(|e| e.player_message().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `GET /v1/live/{match_id}`: one match's snapshot, or `None` once it has ended or was never
/// live. `match_id` is validated as a canonical UUID before anything is sent -- see
/// `reborn_core::live_view::snapshot` -- so a stray fragment typed into the page can never reach
/// the network as a path segment.
#[tauri::command]
pub async fn live_match(match_id: String) -> Result<Option<serde_json::Value>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = identity::origin(&p).map_err(|e| e.player_message().to_string())?;
        let http = crate::http_client();
        reborn_core::live_view::snapshot(&http, &origin, &match_id).map_err(|e| e.player_message().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
