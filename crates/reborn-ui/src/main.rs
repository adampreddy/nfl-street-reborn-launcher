//! The launcher window.
//!
//! # What this layer is allowed to do
//!
//! Nothing. It holds no rules of its own. Every decision, which pad to bind,
//! whether a session ended badly, whether migration is safe, already lives in
//! `reborn-core` where it is tested. This file turns those answers into JSON
//! and hands them to a web view.
//!
//! That split is deliberate and worth keeping. The two data-loss bugs found so
//! far were both found by a command line tool driving the same core, which only
//! worked because the core is where the thinking happens. A rule that lived in
//! the interface could only be exercised by clicking.
//!
//! # Why the window can be trusted not to lie
//!
//! Every command here returns either a real answer or an error string that says
//! what failed. None of them invent a plausible-looking placeholder. An
//! interface that shows a controller which is not connected, or a session that
//! did not happen, is worse than one that shows nothing.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use serde::Serialize;

use reborn_core::devices::{DeviceScanner, Enumeration};
use reborn_core::supervisor::{Startup, Supervisor};
use reborn_core::{DataDir, Platform};

/// Where the launcher keeps things, and whether that is safe.
#[derive(Debug, Serialize)]
struct Paths {
    install_root: String,
    data_root: String,
    /// True when the data root sits outside the install root, which is the
    /// property that stops an uninstall taking the player's memory cards.
    saves_survive_uninstall: bool,
    directories: Vec<DirEntry>,
}

#[derive(Debug, Serialize)]
struct DirEntry {
    relative: String,
    exists: bool,
    class: &'static str,
}

#[derive(Debug, Serialize)]
struct DeviceView {
    device_string: String,
    family: String,
    port: Option<u8>,
    /// Set when the device was found but will not be used, explaining why.
    set_aside: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeviceReport {
    pads: Vec<DeviceView>,
    other: Vec<DeviceView>,
    /// True when every backend was searched. When false an empty list means
    /// "we did not look everywhere", not "nothing is plugged in".
    complete: bool,
    unsearched: Vec<String>,
    /// How the list was obtained, so the interface never implies more certainty
    /// than it has.
    source: &'static str,
}

#[derive(Debug, Serialize)]
struct SessionView {
    id: String,
    build: String,
    minutes: f64,
    ended: String,
    needs_recovery: bool,
}

#[tauri::command]
fn paths() -> Result<Paths, String> {
    let p = Platform::detect();
    let install = p.paths.install_root().map_err(|e| e.to_string())?;
    let data = p.paths.data_root().map_err(|e| e.to_string())?;

    let directories = DataDir::ALL
        .iter()
        .map(|d| DirEntry {
            relative: d.relative().to_string(),
            exists: data.join(d.relative()).exists(),
            class: if d.is_user_data() {
                "user"
            } else if d.is_derived() {
                "derived"
            } else {
                "managed"
            },
        })
        .collect();

    Ok(Paths {
        saves_survive_uninstall: !data.starts_with(&install),
        install_root: install.display().to_string(),
        data_root: data.display().to_string(),
        directories,
    })
}

fn to_report(scan: &Enumeration, source: &'static str) -> DeviceReport {
    let pads = scan
        .playable_pads()
        .iter()
        .enumerate()
        .map(|(i, d)| DeviceView {
            device_string: d.device_string(),
            family: d.family.label().to_string(),
            port: Some((i + 1) as u8),
            set_aside: None,
        })
        .collect();

    let other = scan
        .set_aside()
        .into_iter()
        .map(|(d, why)| DeviceView {
            device_string: d.device_string(),
            family: d.family.label().to_string(),
            port: None,
            set_aside: Some(why.to_string()),
        })
        .collect();

    DeviceReport {
        pads,
        other,
        complete: scan.is_complete(),
        unsearched: scan
            .not_searched
            .iter()
            .map(|(b, why)| format!("{}: {}", b.source(), why))
            .collect(),
        source,
    }
}

#[tauri::command]
fn devices(runtime: Option<String>) -> Result<DeviceReport, String> {
    let p = Platform::detect();

    // Asking the runtime is the only authoritative answer. The built in scan
    // sees XInput and nothing else, which on a real machine misses the WGInput
    // backend entirely and would miss every PlayStation pad.
    if let Some(exe) = runtime {
        let scratch = p
            .paths
            .data_root()
            .map_err(|e| e.to_string())?
            .join(DataDir::Cache.relative());
        match reborn_core::devices::scan_via_runtime(&p, std::path::Path::new(&exe), &scratch) {
            Ok(scan) => return Ok(to_report(&scan, "runtime")),
            // Fall through rather than fail. A missing runtime should not leave
            // the page blank, but the interface is told which answer it got.
            Err(_) => return Ok(to_report(&DeviceScanner::scan(), "fallback")),
        }
    }
    Ok(to_report(&DeviceScanner::scan(), "fallback"))
}

#[tauri::command]
fn sessions() -> Result<Vec<SessionView>, String> {
    let p = Platform::detect();
    match Supervisor::acquire(&p).map_err(|e| e.to_string())? {
        Startup::AlreadyRunning { .. } => {
            Err("another launcher is running and owns the session history".to_string())
        }
        Startup::Owner(s) => {
            let _ = s.recover_unfinished().map_err(|e| e.to_string())?;
            let history = s.history().map_err(|e| e.to_string())?;
            Ok(history
                .iter()
                .rev()
                .map(|r| SessionView {
                    id: r.id.clone(),
                    build: format!("{:?}", r.build).to_lowercase(),
                    minutes: r.duration_ms().unwrap_or(0) as f64 / 60_000.0,
                    ended: r
                        .end
                        .as_ref()
                        .map(|e| format!("{e:?}"))
                        .unwrap_or_else(|| "running".to_string()),
                    needs_recovery: r.end.as_ref().is_some_and(|e| e.needs_recovery()),
                })
                .collect())
        }
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![paths, devices, sessions])
        .run(tauri::generate_context!())
        .expect("the launcher window could not start");
}
