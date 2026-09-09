//! The Content page: textures, runtime and mods components, and the
//! runtime lint that keeps a started runtime from being published.
//!
//! Stub for Task 1. Implemented in Task 6. The runtime lint runs before
//! anything is written and refuses a tree carrying `ReShade.log*`,
//! `ReShade.ini.launcher-original`, a non-empty `ReShadePreset.ini`, a
//! `User` folder or a `Dump` folder: the marks of a runtime the launcher
//! has already started. Runtimes 1.0.0 and 1.0.1 were both published that
//! way and both offered every player a pointless update.

/// Plan: what publishing this component, folder and version would do.
/// Version is refused when the origin already holds it.
#[tauri::command]
pub fn content_plan(
    component: String,
    folder: String,
    version: String,
) -> Result<serde_json::Value, String> {
    let _ = (component, folder, version);
    unimplemented!("Task 6")
}

/// Run: publish the component, then a release carrying it forward.
#[tauri::command]
pub fn content_run(
    component: String,
    folder: String,
    version: String,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    let _ = (component, folder, version, dry_run);
    unimplemented!("Task 6")
}

/// Verify: read the bucket back for this component.
#[tauri::command]
pub fn content_verify(component: String) -> Result<serde_json::Value, String> {
    let _ = component;
    unimplemented!("Task 6")
}

/// The runtime lint, run on its own so a page can check a folder before
/// offering Plan at all.
#[tauri::command]
pub fn content_lint_runtime(folder: String) -> Result<serde_json::Value, String> {
    let _ = folder;
    unimplemented!("Task 6")
}
