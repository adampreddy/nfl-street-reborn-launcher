//! Small window and shell commands: hold the window at one size during first
//! run, open a page of the site in the browser, hand off to the uninstaller.

use std::path::PathBuf;
use tauri::Manager;

/// The only origin the launcher opens in a browser. A launcher that opens any
/// URL is a launcher that opens whatever a manifest or a mod says.
const SITE: &str = "https://nflstreetreborn.com/";

/// Whether `url` is one this launcher will hand to the shell: a page on the site, or exactly
/// `<origin>/<live|games|u>/<uuid>` on the backend origin or the page origin.
///
/// Three kinds of page -- `live` (a match in progress), `games` (a finished game) and `u` (a
/// public profile) -- each reachable on either of two origins: `identity::DEFAULT_ORIGIN` (the
/// backend, which also serves these pages) and `identity::PAGE_ORIGIN` (the web game proper).
/// Neither origin is a second literal here; both are imported so this cannot drift from the
/// constants the rest of the launcher uses.
///
/// Pure and side-effect free, so tests drive it directly without spawning a process. This is the
/// ONLY place any of these rules is decided; `open_website` calls it and does not re-derive any
/// of them. The UUID rule is the same [`reborn_core::live_view::is_canonical_uuid`] the core
/// client checks a match id against before it is ever sent to the network -- reused rather than
/// reimplemented, so the two can never drift apart on what a "real" id looks like. A profile page
/// is always opened by id, never by the player's display name: this function has no way to tell
/// an id from a name other than the UUID check, so `/u/<name>` is refused along with everything
/// else that is not a canonical UUID.
///
/// `strip_prefix` is what makes this strict: anything left over after the prefix other than
/// exactly 36 canonical-UUID characters -- `../admin`, a `?query`, a `#fragment`, a trailing
/// slash, more path -- fails [`reborn_core::live_view::is_canonical_uuid`]'s length check and is
/// refused, without this function having to name any of those shapes itself.
fn allowed(url: &str) -> bool {
    if url.starts_with(SITE) {
        return true;
    }
    use reborn_core::identity::{DEFAULT_ORIGIN, PAGE_ORIGIN};
    [DEFAULT_ORIGIN, PAGE_ORIGIN].iter().any(|origin| {
        ["live", "games", "u"].iter().any(|kind| {
            url.strip_prefix(&format!("{origin}/{kind}/"))
                .is_some_and(reborn_core::live_view::is_canonical_uuid)
        })
    })
}

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
    if !allowed(&url) {
        return Err("Only pages on nflstreetreborn.com open from here.".into());
    }
    let mut cmd = std::process::Command::new("rundll32.exe");
    cmd.args(["url.dll,FileProtocolHandler", &url]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
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
        return Err("No uninstaller was found beside the launcher. Use Windows Settings, then Apps.".into());
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

    /// A canonical UUID from `reborn_core::live_view`'s own test fixtures, so this file is
    /// never the place a second "what a valid id looks like" gets written down.
    const LIVE_ID: &str = "3f1c0f3e-0000-4000-8000-000000000001";

    #[test]
    fn a_page_on_the_site_is_still_allowed() {
        assert!(allowed("https://nflstreetreborn.com/x"));
    }

    #[test]
    fn a_canonical_live_match_link_is_allowed() {
        assert!(allowed(&format!("https://api.nflstreetreborn.com/live/{LIVE_ID}")));
    }

    #[test]
    fn a_live_link_is_refused_unless_it_is_exactly_the_bare_canonical_id() {
        // Path traversal past the prefix.
        assert!(!allowed("https://api.nflstreetreborn.com/live/../admin"));
        // A query string after an otherwise-valid id.
        assert!(!allowed(&format!("https://api.nflstreetreborn.com/live/{LIVE_ID}?x=1")));
        // Uppercase: the backend's ids are always lowercase, same rule as `is_canonical_uuid`.
        assert!(!allowed(&format!(
            "https://api.nflstreetreborn.com/live/{}",
            LIVE_ID.to_ascii_uppercase()
        )));
        // A real backend path, but not the live-match page.
        assert!(!allowed("https://api.nflstreetreborn.com/v1/live"));
        // The right path and a real id, wrong origin entirely.
        assert!(!allowed(&format!("https://evil.com/live/{LIVE_ID}")));
    }

    #[test]
    fn game_and_profile_pages_open_on_both_origins() {
        for origin in [reborn_core::identity::DEFAULT_ORIGIN, reborn_core::identity::PAGE_ORIGIN] {
            for kind in ["live", "games", "u"] {
                assert!(allowed(&format!("{origin}/{kind}/{LIVE_ID}")), "{origin}/{kind}");
                assert!(!allowed(&format!("{origin}/{kind}/{LIVE_ID}?x=1")));
                assert!(!allowed(&format!("{origin}/{kind}/../admin")));
            }
            assert!(!allowed(&format!("{origin}/v1/players/{LIVE_ID}/profile")));
            assert!(!allowed(&format!("{origin}/u/Adam")), "only an id, never a name, from the launcher");
        }
        assert!(!allowed(&format!("https://online.nflstreetreborn.com.evil.example/u/{LIVE_ID}")));
    }

    /// The CSP's `img-src` is the one directive this feature widens, and only to let a live
    /// match's crest and avatar images load. Everything else -- `default-src`, `style-src`,
    /// `script-src`, `font-src`, and in particular `connect-src`, which does not exist in this
    /// CSP today -- must be untouched: the page never fetches the backend itself, only the Rust
    /// commands do, so nothing here needed a new `connect-src` at all.
    #[test]
    fn the_csp_only_widens_img_src_for_the_backend_and_discord() {
        let conf = include_str!("../tauri.conf.json");
        let json: serde_json::Value = serde_json::from_str(conf).expect("tauri.conf.json is valid JSON");
        let csp = json["app"]["security"]["csp"]
            .as_str()
            .expect("the CSP is a single string");

        let directives: std::collections::HashMap<&str, &str> = csp
            .split(';')
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(|d| {
                let mut parts = d.splitn(2, char::is_whitespace);
                (parts.next().unwrap_or(""), parts.next().unwrap_or("").trim())
            })
            .collect();

        assert_eq!(
            directives.get("img-src").copied(),
            Some("'self' data: https://api.nflstreetreborn.com/live-assets/ https://cdn.discordapp.com/avatars/"),
            "img-src must be exactly 'self', data:, the backend's /live-assets/ path and Discord's /avatars/ path"
        );
        assert!(
            directives.get("connect-src").is_none(),
            "connect-src must stay absent: the live page renders what the Rust commands already \
             fetched, and never fetches the backend itself"
        );
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
