//! The Keys page: the signing keys and the vault.
//!
//! Stub for Task 1. Implemented in Task 8. Shows which public keys the
//! shipped launcher trusts (`reborn_core::signing::TRUSTED_KEYS`) and
//! whether the key file on disk matches one of them; generate refuses if
//! either key file already exists. A stored vault value is never read back
//! into the page: `vault_store` reports only whether it stored.

/// Whether `publisher.key` and `rotation.key` exist, and whether the
/// public key on disk matches one the shipped launcher trusts.
#[tauri::command]
pub fn keys_state() -> Result<serde_json::Value, String> {
    unimplemented!("Task 8")
}

/// Generate `publisher.key` and `rotation.key` together, into `dir`.
/// Refuses if either file already exists.
#[tauri::command]
pub fn keys_generate(dir: String) -> Result<serde_json::Value, String> {
    let _ = dir;
    unimplemented!("Task 8")
}

/// The three vault fields: stored or missing, never the value.
#[tauri::command]
pub fn vault_state() -> Result<serde_json::Value, String> {
    unimplemented!("Task 8")
}

/// Store one vault value. Returns only whether it stored; the value is
/// never read back into the page.
#[tauri::command]
pub fn vault_store(which: String, value: String) -> Result<serde_json::Value, String> {
    let _ = (which, value);
    unimplemented!("Task 8")
}

/// Forget one vault value.
#[tauri::command]
pub fn vault_forget(which: String) -> Result<serde_json::Value, String> {
    let _ = which;
    unimplemented!("Task 8")
}

/// Open a file or folder picker for one settings field and save the chosen
/// path into settings.
#[tauri::command]
pub fn settings_browse(which: String) -> Result<serde_json::Value, String> {
    let _ = which;
    unimplemented!("Task 8")
}
