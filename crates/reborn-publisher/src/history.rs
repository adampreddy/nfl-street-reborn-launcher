//! The History page: the journal as a table, and rollback to an earlier
//! version of a component.
//!
//! Stub for Task 1. Implemented in Task 7. Rollback shows the current pins
//! against the proposed pins and the bytes a player would fetch, then
//! signs a release with the next sequence and uploads; it needs its own
//! click after the before-and-after is shown.

/// The last `limit` journal records, newest first.
#[tauri::command]
pub fn history_list(limit: usize) -> Result<serde_json::Value, String> {
    let _ = limit;
    unimplemented!("Task 7")
}

/// Every version of one component the origin still holds a manifest for.
#[tauri::command]
pub fn history_versions(component: String) -> Result<serde_json::Value, String> {
    let _ = component;
    unimplemented!("Task 7")
}

/// Plan: the current pins against the proposed pins, and the bytes a player
/// would fetch.
#[tauri::command]
pub fn rollback_plan(component: String, version: String) -> Result<serde_json::Value, String> {
    let _ = (component, version);
    unimplemented!("Task 7")
}

/// Run: sign a release at the next sequence pinning the earlier version,
/// and upload it.
#[tauri::command]
pub fn rollback_run(
    component: String,
    version: String,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    let _ = (component, version, dry_run);
    unimplemented!("Task 7")
}

/// Verify: read the release back and confirm the rolled-back pin.
#[tauri::command]
pub fn rollback_verify() -> Result<serde_json::Value, String> {
    unimplemented!("Task 7")
}
