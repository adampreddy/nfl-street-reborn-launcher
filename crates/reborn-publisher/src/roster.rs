//! The Roster page: fetch the roster the site published, or a file, and
//! push it as a component plus a release carrying every other pin forward.
//!
//! Stub for Task 1. Implemented in Task 3, over
//! `reborn_core::publish::roster::publish_roster` through
//! [`crate::ops::run_operation`], then plan 2's `upload`. `source` is
//! either `{"kind": "site"}` (fetch with the vaulted token from the
//! export URL the CLI uses) or `{"kind": "file", "path": ...}`.

/// Plan: the roster's player and team counts, the sequence and pins before
/// and after, and one sentence on what a player will see. Writes nothing.
#[tauri::command]
pub fn roster_plan(source: serde_json::Value, version: String) -> Result<serde_json::Value, String> {
    let _ = (source, version);
    unimplemented!("Task 3")
}

/// Run: publish the roster component, then upload it and the release.
#[tauri::command]
pub fn roster_run(
    source: serde_json::Value,
    version: String,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    let _ = (source, version, dry_run);
    unimplemented!("Task 3")
}

/// Verify: read the bucket back, then a seeded scratch-root check.
#[tauri::command]
pub fn roster_verify() -> Result<serde_json::Value, String> {
    unimplemented!("Task 3")
}
