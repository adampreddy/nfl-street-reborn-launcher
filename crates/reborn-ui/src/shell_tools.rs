//! Small window and shell commands: hold the window at one size during first
//! run, open a page of the site in the browser, hand off to the uninstaller.

use std::path::PathBuf;
use tauri::Manager;

/// The only origin the launcher opens in a browser. A launcher that opens any
/// URL is a launcher that opens whatever a manifest or a mod says.
const SITE: &str = "https://nflstreetreborn.com/";

/// One size while first run is incomplete: every setup screen is laid out for
/// 1280 by 800, and a resized window put a scrollbar on the controller step.
/// Resizable again once the launcher proper is up.
#[tauri::command]
pub fn setup_window_lock(app: tauri::AppHandle, locked: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    if locked {
        window.set_size(tauri::LogicalSize::new(1280.0, 800.0)).map_err(|e| e.to_string())?;
        let _ = window.center();
    }
    window.set_resizable(!locked).map_err(|e| e.to_string())?;
    window.set_maximizable(!locked).map_err(|e| e.to_string())?;
    Ok(())
}

/// Open a page of the site in the player's default browser.
#[tauri::command]
pub fn open_website(url: String) -> Result<(), String> {
    if !url.starts_with(SITE) {
        return Err("Only pages on nflstreetreborn.com open from here.".into());
    }
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("rundll32.exe");
        cmd.args(["url.dll,FileProtocolHandler", &url]);
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
        cmd.spawn().map_err(|e| e.to_string())?;
        Ok(())
    }
    // `xdg-open` on Linux, `open` on macOS; rundll32 exists only on Windows.
    #[cfg(not(windows))]
    {
        reborn_core::platform::imp::open_url(&reborn_core::Platform::detect(), &url)
            .map_err(|_| "The page could not be opened in your browser.".to_string())
    }
}

/// Start the uninstaller beside the launcher and quit, so it finds the
/// launcher closed. The uninstaller asks its own "are you sure"; saves live
/// on a separate branch and are not its to take.
#[tauri::command]
pub fn uninstall_launcher(app: tauri::AppHandle, confirm: bool) -> Result<(), String> {
    if !confirm {
        return Err("Uninstall was not confirmed.".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let uninstaller: PathBuf = exe
        .parent()
        .map(|d| d.join("unins000.exe"))
        .ok_or_else(|| "The launcher's own folder could not be found.".to_string())?;
    if !uninstaller.is_file() {
        return Err(if cfg!(windows) {
            "No uninstaller was found beside the launcher. Use Windows Settings, then Apps."
        } else {
            "Remove Reborn the same way you installed it."
        }.into());
    }
    std::process::Command::new(&uninstaller).spawn().map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

/// Bytes of a launcher update on disk so far: the `.partial` file the
/// download writes, in the downloads folder. The page divides by the size
/// the update check reported. Zero when nothing is downloading.
#[tauri::command]
pub fn update_progress() -> Result<u64, String> {
    let p = reborn_core::Platform::detect();
    let dir = p.data_dir(reborn_core::DataDir::Downloads).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|x| x == "partial") {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_site_opens() {
        assert!(open_website("https://example.com/".into()).is_err());
        assert!(open_website("http://nflstreetreborn.com/".into()).is_err(), "https only");
        assert!(open_website("https://nflstreetreborn.com.evil.example/".into()).is_err());
        // The site itself would launch a browser here, so it is not exercised.
    }
}

/// One-time notices the launcher owes the player, with their words. The
/// core decides both; a page only draws them and reports "Got it".
#[tauri::command]
pub fn notices_pending() -> Result<Vec<serde_json::Value>, String> {
    let p = reborn_core::Platform::detect();
    let ids = reborn_core::notices::pending(&p).map_err(|e| e.to_string())?;
    Ok(ids
        .iter()
        .filter_map(|id| {
            reborn_core::notices::text(id)
                .map(|(title, text)| serde_json::json!({"id": id, "title": title, "text": text}))
        })
        .collect())
}

#[tauri::command]
pub fn notice_dismiss(id: String) -> Result<(), String> {
    reborn_core::notices::dismiss(&reborn_core::Platform::detect(), &id).map_err(|e| e.to_string())
}
