//! The Launcher page: build and publish the launcher-only installer and its
//! update manifest.
//!
//! Stub for Task 1. Implemented in Task 4, driving
//! `installer\windows\build.ps1 -LauncherOnly -Version <v>` through the
//! core's `ProcessHost`, then uploading the exe and `latest.json`.

/// Plan: what the bucket serves now, the version to build, and a warning
/// when they are equal (no launcher would be offered anything).
#[tauri::command]
pub fn launcher_plan(version: String) -> Result<serde_json::Value, String> {
    let _ = version;
    unimplemented!("Task 4")
}

/// Run: build, hash the produced exe, compare with `latest.json`, upload
/// the exe then the manifest.
#[tauri::command]
pub fn launcher_run(version: String, dry_run: bool) -> Result<serde_json::Value, String> {
    let _ = (version, dry_run);
    unimplemented!("Task 4")
}

/// Verify: read both back over HTTPS and ask the core's update check, as a
/// launcher still on the previously hosted version, that the new version
/// is offered.
#[tauri::command]
pub fn launcher_verify(version: String) -> Result<serde_json::Value, String> {
    let _ = version;
    unimplemented!("Task 4")
}
