//! The Game page: the Reborn Builder's ISO work, and the game patch chain.
//!
//! Stub for Task 1. Implemented in Task 5, two sections on one page. Build
//! ISO drives `reborn-builder-cli.exe`; the game patch section hashes the
//! new build, refuses a base that is not the chain's tip, then encodes,
//! decode-verifies, publishes, re-signs, uploads and verifies.

/// Build ISO, plan: what the Builder would do with this base image, roster
/// and output path, and its warnings.
#[tauri::command]
pub fn game_build_plan(base: String, roster: String, out: String) -> Result<serde_json::Value, String> {
    let _ = (base, roster, out);
    unimplemented!("Task 5")
}

/// Build ISO, run: build it, and hash the result.
#[tauri::command]
pub fn game_build_run(
    base: String,
    roster: String,
    out: String,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    let _ = (base, roster, out, dry_run);
    unimplemented!("Task 5")
}

/// Dump an ISO's contents through the Builder, for inspection.
#[tauri::command]
pub fn game_dump(iso: String, out: String) -> Result<serde_json::Value, String> {
    let _ = (iso, out);
    unimplemented!("Task 5")
}

/// Sync a memory card's gear and faces into a roster file through the
/// Builder.
#[tauri::command]
pub fn game_cardsync(card: String, roster: String) -> Result<serde_json::Value, String> {
    let _ = (card, roster);
    unimplemented!("Task 5")
}

/// Game patch, plan: hash the new build, read the chain's head, and refuse
/// a base that is not the chain's tip, with both hashes shown.
#[tauri::command]
pub fn game_patch_plan(to: String) -> Result<serde_json::Value, String> {
    let _ = to;
    unimplemented!("Task 5")
}

/// Game patch, run: encode, decode-verify, publish, re-sign, upload.
#[tauri::command]
pub fn game_patch_run(to: String, version: String, dry_run: bool) -> Result<serde_json::Value, String> {
    let _ = (to, version, dry_run);
    unimplemented!("Task 5")
}

/// Game patch, verify.
#[tauri::command]
pub fn game_patch_verify() -> Result<serde_json::Value, String> {
    unimplemented!("Task 5")
}
