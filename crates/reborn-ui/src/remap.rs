//! The controller remap wizard.
//!
//! # What it is for
//!
//! Most pads never need this. An Xbox pad on XInput and a PlayStation pad on
//! WGInput both report their controls by NAME, so the launcher reads the names
//! and maps them with nothing asked of the player.
//!
//! A pad that reports by NUMBER cannot be handled that way. DirectInput and SDL
//! both do it: `Button 0` through `Button 14`, with nothing to say which is
//! which, in an order that is not stable across pads or across SDL versions.
//! There is no table that fixes that and no guess worth making, because a wrong
//! guess silently puts a control somewhere the player did not choose.
//!
//! So the launcher asks, and the RUNTIME watches. Detection happens inside the
//! same binary that will later resolve the name, which is the one guarantee the
//! launcher cannot give itself. Getting that guarantee wrong is the whole
//! reason PlayStation pads were unusable.
//!
//! # Shape
//!
//! One capture session serves the whole wizard and lives in a static, because
//! the interface drives it one control at a time across many calls. It is shut
//! down by `remap_stop`, by starting another session, or by the process ending,
//! and `Capture`'s own `Drop` is what actually does it, so no path can leak a
//! runtime holding the pad open.

use std::path::Path;
use std::sync::Mutex;

use reborn_core::capture::Capture;
use reborn_core::controller::Mapping;
use reborn_core::platform::Platform;

/// The live session. `None` when the wizard is not open.
static SESSION: Mutex<Option<Capture>> = Mutex::new(None);

fn scratch() -> Result<std::path::PathBuf, String> {
    let p = Platform::detect();
    let dir = p
        .paths
        .data_root()
        .map_err(|e| e.to_string())?
        .join("cache")
        .join("remap");
    Ok(dir)
}

/// Open the wizard: start the runtime and report what it can see.
///
/// For every pad it also reports which controls map from the names alone and
/// which have to be pressed, so the interface can say "your controller is
/// ready" for most players instead of walking all twenty regardless.
#[tauri::command]
pub async fn remap_start(runtime: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let dir = scratch()?;
        let capture = Capture::start(&p, Path::new(&runtime), &dir)?;

        let pads: Vec<serde_json::Value> = capture
            .devices()
            .iter()
            // Keyboard and mouse is not something a player holds.
            .filter(|d| !d.device.starts_with("DInput/") || !d.device.contains("Keyboard"))
            .map(|d| {
                let available = d.all_names();
                let (_, needs_pressing) = Mapping::auto_from_with(&available, d.sdl_layout().as_ref());
                serde_json::json!({
                    "device": d.device,
                    "automatic": needs_pressing.is_empty(),
                    "needs_pressing": needs_pressing
                        .iter()
                        .map(|i| serde_json::json!({"key": i.ini_key(), "label": i.label()}))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();

        *SESSION.lock().map_err(|_| "the wizard is in a bad state")? = Some(capture);
        Ok(serde_json::json!({ "pads": pads }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Wait for the player to press something on this pad.
///
/// `binding` is null when nothing was pressed in time, which is how skipping a
/// control is expressed. A pad with no Z button must still be finishable.
#[tauri::command]
pub async fn remap_detect(device: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let guard = SESSION.lock().map_err(|_| "the wizard is in a bad state")?;
        let capture = guard
            .as_ref()
            .ok_or("The controller wizard is not open. Start it again.")?;
        let pressed = capture.detect(&device)?;
        Ok(serde_json::json!({ "binding": pressed }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Close the wizard and stop the runtime.
///
/// Safe to call when nothing is open, because the interface calls it on the way
/// out of the page whether or not a session was ever started.
#[tauri::command]
pub async fn remap_stop() -> Result<(), String> {
    // Dropping the Capture writes the quit file.
    let mut guard = SESSION.lock().map_err(|_| "the wizard is in a bad state")?;
    *guard = None;
    Ok(())
}
