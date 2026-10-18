//! The League tab's data commands (spec 2026-10-08 section 3): whether to draw the tab (`league_member`, from the cache
//! only) and the team home (`league_home`). The League edition's commands (install, offline, friendly, league room) are
//! Plan 3b's module; joining and cancelling a room are the existing `mm_room_join` / `mm_room_action`.
//!
//! Every failure is `{ kind, message }` (`LiveCommandError`), with `message` one of the fixed sentences below.

use std::path::Path;

use reborn_core::identity;
use reborn_core::league_home::{self, LeagueCache};
use reborn_core::live_view::LiveError;
use reborn_core::platform::Platform;

use crate::live_view::LiveCommandError;

const NOT_LOADED: &str = "League data couldn't be loaded right now.";
const BUSY: &str = "The league server is busy. Trying again in a minute.";

fn err(kind: &'static str, sentence: &str) -> LiveCommandError {
    LiveCommandError { kind, message: sentence.to_owned() }
}

fn home_error(e: LiveError) -> LiveCommandError {
    match e {
        LiveError::RateLimited => err("rate_limited", BUSY),
        _ => err("unavailable", NOT_LOADED),
    }
}

fn signed_out() -> serde_json::Value {
    serde_json::json!({ "signed_in": false })
}

fn mark_signed_in(v: &mut serde_json::Value) {
    v["signed_in"] = serde_json::Value::Bool(true);
}

/// The bearer, or `None` when signed out (no facts file, or no vault token). One vault read.
fn bearer(p: &Platform) -> Option<String> {
    identity::signed_in_token(p).ok().flatten()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// One fetch of the team home, resolved against `league.json` and written back (`league_home::resolve`). The page's
/// load and the background membership refresh both come through here.
fn home_pass(
    path: Option<&Path>,
    fetch: impl FnOnce() -> Result<serde_json::Value, LiveError>,
    now: u64,
) -> Result<serde_json::Value, LiveError> {
    let cache = path.map(league_home::load_cache).unwrap_or_else(LeagueCache::default);
    let (out, next) = league_home::resolve(fetch(), &cache, now);
    if let (Some(path), Some(next)) = (path, next) {
        league_home::save_cache(path, &next);
    }
    out
}

/// What one membership check learned from the server: whether this player is a member, and whether that came from
/// the league site itself (`fresh`) or from the backend's saved copy while the site is down (backend migration 033).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Refresh {
    member: bool,
    fresh: bool,
}

/// The background refresh's news: the server's member answer (a 401 is "not a member": the session is over), or
/// `None` for anything else. An outage, or the launcher's own cached fallback it reads as, changes nothing.
fn refresh_pass(path: Option<&Path>, fetch: impl FnOnce() -> Result<serde_json::Value, LiveError>, now: u64) -> Option<Refresh> {
    let fetched = fetch();
    let from_server = fetched.is_ok();
    let server_stale = matches!(&fetched, Ok(v) if v["stale"] == serde_json::Value::Bool(true));
    match home_pass(path, move || fetched, now) {
        Ok(v) if from_server => Some(Refresh { member: v["member"].as_bool().unwrap_or(false), fresh: !server_stale }),
        Err(LiveError::Server(401)) => Some(Refresh { member: false, fresh: true }),
        _ => None,
    }
}

/// How long to wait before each retry after an outage, in seconds: a member who signed in while the league site
/// was unreachable gets the tab as soon as it answers, not at the next restart (Adam 10-18).
const RETRY_AFTER_S: [u64; 6] = [60, 120, 300, 600, 900, 1800];

/// Check, and after an outage (or the backend's saved copy) wait and check again, until a fresh answer or the last
/// wait. `tell` hears each new membership answer once.
fn refresh_with_retries(mut pass: impl FnMut() -> Option<Refresh>, mut sleep: impl FnMut(std::time::Duration), mut tell: impl FnMut(bool), waits: &[u64]) {
    let mut told: Option<bool> = None;
    let mut waits = waits.iter();
    loop {
        if let Some(r) = pass() {
            if told != Some(r.member) {
                tell(r.member);
                told = Some(r.member);
            }
            if r.fresh {
                return;
            }
        }
        match waits.next() {
            Some(&s) => sleep(std::time::Duration::from_secs(s)),
            None => return,
        }
    }
}

/// Spec §3: membership refreshed on startup and after sign-in. Writes `league.json` (the nav's answer) on its own
/// thread, never awaited, and says nothing on failure. A fresh answer is sent to every page as `league-membership`
/// (a bool), so a new member's League link appears without a restart (shell.js). Signed out: does nothing.
pub fn refresh_membership(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        use tauri::Emitter;
        let p = Platform::detect();
        let Ok(origin) = identity::origin(&p) else { return };
        let path = league_home::cache_path(&p).ok();
        let pass = || {
            // Read the bearer each time: a sign-out while waiting ends the retries (no token, nothing to ask).
            let token = bearer(&p)?;
            refresh_pass(path.as_deref(), || league_home::fetch_home(&crate::http_client(), &origin, &token), now_ms())
        };
        refresh_with_retries(pass, std::thread::sleep, |member| { let _ = app.emit("league-membership", member); }, &RETRY_AFTER_S);
    });
}

/// Whether the nav draws the League link: the cached answer only, no network, so every page's first frame has it.
#[tauri::command]
pub async fn league_member() -> bool {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        if bearer(&p).is_none() {
            return false;
        }
        league_home::cache_path(&p).map(|path| league_home::load_cache(&path).member).unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// `GET /v1/league/me`, with the cached fallback (`league_home::resolve`). Signed out is a shape, not an error. A 503
/// (`league_unavailable`) is an outage, never "not a member": `resolve` falls back to the cached member.
#[tauri::command]
pub async fn league_home() -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        let Some(token) = bearer(&p) else { return Ok(signed_out()) };
        let origin = identity::origin(&p).map_err(|_| err("unavailable", NOT_LOADED))?;
        let path = league_home::cache_path(&p).ok();
        let fetch = || league_home::fetch_home(&crate::http_client(), &origin, &token);
        match home_pass(path.as_deref(), fetch, now_ms()) {
            Ok(mut v) => {
                mark_signed_in(&mut v);
                Ok(v)
            }
            Err(LiveError::Server(401)) => Ok(signed_out()),
            Err(e) => Err(home_error(e)),
        }
    })
    .await
    .map_err(|_| err("unavailable", NOT_LOADED))?
}

/// A day: how long a downloaded league logo is used before it is fetched again.
const LOGO_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
/// The backend's own cap on a league logo (`router.js`, 1 MiB).
const LOGO_MAX_BYTES: u64 = 1 << 20;
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// One team's logo as a `data:image/png;base64,` URL, or `None` when there is none to show.
///
/// The page may load images only from the production API's `/live-assets/` and `data:` (tauri.conf.json CSP), and the
/// backend's league logo route (`/live-assets/league/<team>.png`) is on whichever backend this launcher uses. So the
/// core fetches it from that origin, keeps it a day in `cache/league-logos/`, and hands the page the bytes. Asked once
/// per team per page visit (league.js), never with every league refresh: twelve real logos are about 4 MB.
#[tauri::command]
pub async fn league_logo(team_id: u64) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = identity::origin(&p).ok()?;
        let dir = p.paths.cache_root().ok()?.join("league-logos");
        let file = dir.join(format!("{team_id}.png"));
        let fresh = std::fs::metadata(&file)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age < LOGO_MAX_AGE);
        if !fresh {
            let _ = std::fs::create_dir_all(&dir);
            let part = dir.join(format!("{team_id}.png.part"));
            let url = format!("{}/live-assets/league/{team_id}.png", origin.trim_end_matches('/'));
            use reborn_core::net::Http;
            match crate::http_client().get_to_file(&url, &part) {
                Ok(n) if n > 0 && n <= LOGO_MAX_BYTES => {
                    let _ = std::fs::rename(&part, &file);
                }
                _ => {
                    let _ = std::fs::remove_file(&part);
                }
            }
        }
        logo_data_url(&std::fs::read(&file).ok()?)
    })
    .await
    .ok()
    .flatten()
}

/// `bytes` as a `data:` URL, only when they are a PNG no bigger than the backend allows.
fn logo_data_url(bytes: &[u8]) -> Option<String> {
    if bytes.len() as u64 > LOGO_MAX_BYTES || !bytes.starts_with(&PNG_MAGIC) {
        return None;
    }
    Some(format!("data:image/png;base64,{}", base64_encode(bytes)))
}

fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(&[0xfb, 0xff]), "+/8=");
    }

    #[test]
    fn only_a_png_within_the_cap_becomes_a_logo() {
        let mut png = PNG_MAGIC.to_vec();
        png.extend_from_slice(b"rest");
        assert!(logo_data_url(&png).unwrap().starts_with("data:image/png;base64,iVBORw0KGgpyZXN0"));
        assert_eq!(logo_data_url(b"<html>captcha</html>"), None, "an HTML answer is never drawn as a logo");
        let mut big = PNG_MAGIC.to_vec();
        big.resize(LOGO_MAX_BYTES as usize + 1, 0);
        assert_eq!(logo_data_url(&big), None);
    }

    #[test]
    fn every_home_failure_reads_as_a_league_sentence() {
        let cases = [
            (LiveError::RateLimited, "rate_limited", BUSY),
            (LiveError::Server(503), "unavailable", NOT_LOADED),
            (LiveError::Unreachable, "unavailable", NOT_LOADED),
            (LiveError::BadReply, "unavailable", NOT_LOADED),
        ];
        for (e, kind, message) in cases {
            let v = serde_json::to_value(home_error(e.clone())).unwrap();
            assert_eq!((v["kind"].as_str().unwrap(), v["message"].as_str().unwrap()), (kind, message), "{e:?}");
        }
    }

    #[test]
    fn the_home_shape_marks_signed_in() {
        assert_eq!(signed_out(), serde_json::json!({ "signed_in": false }));
        let mut v = serde_json::json!({ "member": true });
        mark_signed_in(&mut v);
        assert_eq!(v["signed_in"], true);
    }

    fn member_home() -> serde_json::Value {
        serde_json::json!({ "member": true, "team": { "id": 485, "name": "Atlanta Falcons" }, "matchup": null })
    }

    /// Spec §3 "Membership refreshed on startup and after sign-in": nothing else writes league.json before the tab
    /// exists, so without this a member could never see the League link.
    /// Adam 10-18: a member who signed in while the league site was unreachable never saw the tab until a restart.
    /// The backend's stale answer (its saved copy) is news; the launcher's own cached fallback is not.
    #[test]
    fn a_server_stale_member_answer_is_news_but_not_final() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("league.json");
        let stale = serde_json::json!({ "member": true, "stale": true, "team": { "id": 493 } });
        assert_eq!(refresh_pass(Some(&path), || Ok(stale), 7), Some(Refresh { member: true, fresh: false }));
        assert!(league_home::load_cache(&path).member, "the next start draws the tab");
        assert_eq!(refresh_pass(Some(&path), || Err(LiveError::Server(503)), 9), None, "the launcher's own fallback is not news");
    }

    /// An outage is retried on a backoff until a fresh answer arrives; each answer is told once, a change again.
    #[test]
    fn a_refresh_retries_through_an_outage_until_a_fresh_answer() {
        let mut answers = vec![None, Some(Refresh { member: true, fresh: false }), None, Some(Refresh { member: true, fresh: true })].into_iter();
        let (mut slept, mut told) = (Vec::new(), Vec::new());
        refresh_with_retries(|| answers.next().flatten(), |d| slept.push(d.as_secs()), |m| told.push(m), &[60, 120, 300, 600, 900]);
        assert_eq!(slept, vec![60, 120, 300], "three waits, then the fresh answer ends it");
        assert_eq!(told, vec![true], "told once: the fresh answer said the same");
        let (mut slept, mut told) = (Vec::new(), Vec::new());
        refresh_with_retries(|| None, |d| slept.push(d.as_secs()), |m| told.push(m), &[60, 120]);
        assert_eq!((slept, told), (vec![60, 120], vec![]), "a long outage gives up after the last wait, saying nothing");
    }

    #[test]
    fn a_refresh_with_no_cache_writes_the_member_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("league.json");
        assert_eq!(refresh_pass(Some(&path), || Ok(member_home()), 7), Some(Refresh { member: true, fresh: true }));
        let cache = league_home::load_cache(&path);
        assert!(cache.member, "the nav's next read says member");
        assert_eq!(cache.fetched_at_ms, 7);
    }

    #[test]
    fn a_refresh_for_a_non_member_never_offers_the_tab() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("league.json");
        assert_eq!(refresh_pass(Some(&path), || Ok(serde_json::json!({ "member": false })), 7), Some(Refresh { member: false, fresh: true }));
        assert!(!league_home::load_cache(&path).member);
    }

    #[test]
    fn an_outage_during_a_refresh_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("league.json");
        for e in [LiveError::Server(503), LiveError::Unreachable, LiveError::RateLimited, LiveError::BadReply] {
            assert_eq!(refresh_pass(Some(&path), || Err(e.clone()), 7), None, "{e:?}");
            assert!(!path.exists(), "no cache appears from an outage ({e:?})");
        }
        let cached = LeagueCache { member: true, home: Some(member_home()), fetched_at_ms: 3 };
        league_home::save_cache(&path, &cached);
        assert_eq!(refresh_pass(Some(&path), || Err(LiveError::Server(503)), 9), None, "a stale fallback is not news");
        assert_eq!(league_home::load_cache(&path), cached, "a member keeps the tab through an outage");
    }

    #[test]
    fn a_refresh_the_server_refuses_drops_the_tab() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("league.json");
        league_home::save_cache(&path, &LeagueCache { member: true, home: Some(member_home()), fetched_at_ms: 3 });
        assert_eq!(refresh_pass(Some(&path), || Err(LiveError::Server(401)), 9), Some(Refresh { member: false, fresh: true }));
        assert!(!league_home::load_cache(&path).member);
    }

    fn body<'a>(src: &'a str, sig: &str) -> &'a str {
        let from = src.find(sig).unwrap_or_else(|| panic!("{sig} is gone"));
        let rest = &src[from..];
        &rest[..rest.find("\n}\n").unwrap_or(rest.len())]
    }

    /// One fetch + resolve + save, shared: the page's load and the background refresh never drift apart.
    #[test]
    fn the_page_and_the_refresh_share_one_home_pass() {
        let src = include_str!("league_tab.rs");
        let production = &src[..src.find("\n#[cfg(test)]").unwrap()];
        assert_eq!(production.matches("league_home::resolve(").count(), 1, "resolve is called from home_pass only");
        assert!(body(production, "pub async fn league_home(").contains("home_pass("));
        assert!(body(production, "fn refresh_pass(").contains("home_pass("));
    }

    /// Off the main thread and never awaited: startup and sign-in answer first; a failure says nothing.
    #[test]
    fn the_refresh_runs_on_its_own_thread_and_tells_the_nav() {
        let src = include_str!("league_tab.rs");
        let refresh = body(src, "pub fn refresh_membership(");
        assert!(refresh.contains("std::thread::spawn"), "the refresh is spawned, not awaited");
        assert!(refresh.contains("\"league-membership\""), "the nav hears a fresh answer without a restart");
        let main = include_str!("main.rs");
        let main_fn = &main[main.find("\nfn main() {").unwrap()..];
        let setup = &main_fn[main_fn.find(".setup(move |app| {").unwrap()..];
        let setup = &setup[..setup.find("Ok(())").unwrap()];
        assert!(setup.contains("league_tab::refresh_membership("), "startup refreshes membership");
        let poll = &main[main.find("async fn identity_sign_in_poll(").unwrap()..];
        let poll = &poll[..poll.find("\n#[tauri::command]").unwrap()];
        let done = &poll[poll.find("save_signed_in(").unwrap()..];
        assert!(done.contains("league_tab::refresh_membership("), "a finished sign-in refreshes membership");
    }

    /// The facts file and the vault, the vault read once (identity::signed_in_token).
    #[test]
    fn the_bearer_reads_the_vault_once() {
        let src = include_str!("league_tab.rs");
        let b = body(src, "fn bearer(");
        assert!(b.contains("identity::signed_in_token(") && !b.contains("identity::status(") && !b.contains("identity::token("));
    }

    /// The same rule live_view.rs holds itself to: no command hands a page an error's Display.
    #[test]
    fn no_command_here_hands_a_page_raw_error_text() {
        let src = include_str!("league_tab.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        let compact: String = production.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(!compact.contains("e.to_string()"));
    }
}
