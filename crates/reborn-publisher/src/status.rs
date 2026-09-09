//! The Status page: what the bucket serves, what sits in the local origin,
//! and what this machine has ready.
//!
//! Stub for Task 1. Implemented in Task 2, from
//! `reborn_core::publish::status::read_status`, `journal::read_last`, and a
//! check of every tool and input this app needs. The three signatures below
//! are fixed by the plan and registered in `main.rs`; Task 2 fills in their
//! bodies and nothing else in this file's shape should need to change.

/// `reborn_core::publish::status::read_status`, serialized.
#[tauri::command]
pub fn publisher_status() -> serde_json::Value {
    unimplemented!("Task 2")
}

/// Every tool and input this app needs: found or missing, with the path.
#[tauri::command]
pub fn publisher_environment() -> serde_json::Value {
    unimplemented!("Task 2")
}

/// `journal::read_last`, serialized.
#[tauri::command]
pub fn publisher_recent(n: usize) -> serde_json::Value {
    let _ = n;
    unimplemented!("Task 2")
}
