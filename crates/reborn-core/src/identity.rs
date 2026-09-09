//! Who the player is: Discord, through the backend, with a per-device token in the OS vault.
//!
//! Device-code sign-in: the launcher asks the backend for a code, opens the verify
//! page in the system browser, and polls until the browser step is done. No port is
//! opened here and no Discord token ever reaches this process. The only secret the
//! launcher holds is the backend's own device token, stored in the vault under
//! [`TOKEN_ACCOUNT`] and never written to disk or logged.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::net::Http;
use crate::platform::{DataDir, Error, Platform, Result};

pub const TOKEN_ACCOUNT: &str = "nsr.reborn.player";
pub const DEFAULT_ORIGIN: &str = "https://api.nflstreetreborn.com";
const IDENTITY_FILE: &str = "identity.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerFacts {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar_hash: Option<String>,
    #[serde(default)]
    pub show_offline: bool,
}

/// The non-secret half of a sign-in, kept beside settings so pages can say who is signed in
/// without asking the vault or the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityFile {
    pub player: PlayerFacts,
    pub signed_in_at_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verify_url: String,
    pub expires_in: u64,
    pub interval: u64,
}

pub enum Poll {
    Pending,
    Expired,
    Done { token: String, player: PlayerFacts },
}

impl std::fmt::Debug for Poll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Poll::Pending => f.write_str("Pending"),
            Poll::Expired => f.write_str("Expired"),
            // The token is deliberately absent.
            Poll::Done { player, .. } => f.debug_struct("Done").field("player", player).finish(),
        }
    }
}

fn plain(msg: String) -> Error {
    Error::PlainIo(std::io::Error::other(msg))
}

fn origin_url(origin: &str, path: &str) -> String {
    format!("{}{}", origin.trim_end_matches('/'), path)
}

pub fn start(http: &dyn Http, origin: &str, label: &str) -> Result<DeviceStart> {
    let body = serde_json::json!({ "label": label });
    let url = origin_url(origin, "/v1/auth/device");
    let r = http.post_json(&url, &serde_json::to_vec(&body).unwrap_or_default(), None)?;
    if !r.is_success() {
        return Err(plain(format!("{url} answered {}: {}", r.status, r.body.chars().take(200).collect::<String>())));
    }
    serde_json::from_str(&r.body).map_err(|e| plain(format!("the sign-in reply was not understood: {e}")))
}

pub fn poll(http: &dyn Http, origin: &str, device_code: &str) -> Result<Poll> {
    #[derive(Deserialize)]
    struct Reply { token: Option<String>, player: Option<PlayerFacts> }
    let body = serde_json::json!({ "device_code": device_code });
    let url = origin_url(origin, "/v1/auth/device/poll");
    let r = http.post_json(&url, &serde_json::to_vec(&body).unwrap_or_default(), None)?;
    match r.status {
        428 => Ok(Poll::Pending),
        410 | 404 => Ok(Poll::Expired),
        s if (200..300).contains(&s) => {
            let reply: Reply = serde_json::from_str(&r.body).map_err(|e| plain(format!("the poll reply was not understood: {e}")))?;
            match (reply.token, reply.player) {
                (Some(token), Some(player)) => Ok(Poll::Done { token, player }),
                _ => Err(plain("the poll reply had no token".to_string())),
            }
        }
        s => Err(plain(format!("{url} answered {s}"))),
    }
}

pub fn identity_path(platform: &Platform) -> Result<PathBuf> {
    Ok(platform.data_dir(DataDir::Config)?.join(IDENTITY_FILE))
}

fn read_identity(path: &Path) -> Result<Option<IdentityFile>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path: path.to_path_buf(), source }),
    }
}

/// Signed in means BOTH halves exist: the facts file and a token in the vault.
pub fn status(platform: &Platform) -> Result<Option<IdentityFile>> {
    let Some(file) = read_identity(&identity_path(platform)?)? else { return Ok(None) };
    Ok(token(platform)?.map(|_| file))
}

pub fn token(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(TOKEN_ACCOUNT)
}

pub fn save_signed_in(platform: &Platform, token: &str, player: PlayerFacts, now_ms: u64) -> Result<()> {
    platform.credentials.store(TOKEN_ACCOUNT, token)?;
    let path = identity_path(platform)?;
    let file = IdentityFile { player, signed_in_at_ms: now_ms };
    std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap_or_default()).map_err(|source| Error::Io { path, source })
}

/// The server said 401. Drop the token; keep nothing that would make a page claim a sign-in.
pub fn clear_token(platform: &Platform) -> Result<()> {
    let _ = platform.credentials.delete(TOKEN_ACCOUNT);
    let path = identity_path(platform)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|source| Error::Io { path, source })?;
    }
    Ok(())
}

/// Revoke this device on the server (best effort), then forget locally.
pub fn sign_out(platform: &Platform, http: &dyn Http, origin: &str) -> Result<()> {
    if let Some(tok) = token(platform)? {
        let _ = http.delete(&origin_url(origin, "/v1/auth/device"), Some(&tok));
    }
    clear_token(platform)
}

pub fn set_show_offline(platform: &Platform, http: &dyn Http, origin: &str, show: bool) -> Result<PlayerFacts> {
    #[derive(Deserialize)]
    struct Reply { player: PlayerFacts }
    let tok = token(platform)?.ok_or_else(|| plain("not signed in".to_string()))?;
    // PATCH is not on the Http trait; the backend accepts the same body on POST /v1/me/show_offline.
    let url = origin_url(origin, "/v1/me/show_offline");
    let r = http.post_json(&url, &serde_json::to_vec(&serde_json::json!({ "show_offline": show })).unwrap_or_default(), Some(&tok))?;
    if r.status == 401 { clear_token(platform)?; return Err(plain("Signed out. Sign in again.".to_string())); }
    if !r.is_success() { return Err(plain(format!("{url} answered {}", r.status))); }
    let reply: Reply = serde_json::from_str(&r.body).map_err(|e| plain(format!("reply not understood: {e}")))?;
    if let Some(mut file) = read_identity(&identity_path(platform)?)? {
        file.player = reply.player.clone();
        let path = identity_path(platform)?;
        std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap_or_default()).map_err(|source| Error::Io { path, source })?;
    }
    Ok(reply.player)
}

/// Open a URL in the player's default browser through the platform process host.
/// On Windows `explorer.exe <url>` opens the default browser; it exits non-zero even on success,
/// so the handle is dropped, exactly as the folder button does.
pub fn open_in_browser(platform: &Platform, url: &str) -> Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(plain(format!("refusing to open a non-http URL: {url}")));
    }
    let program: &str = if cfg!(windows) { "explorer.exe" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let args = vec![std::ffi::OsString::from(url)];
    platform.processes.spawn(Path::new(program), &args, None, &std::collections::BTreeMap::new()).map(|_| ())
}

/// The device label shown on the backend: the machine name, or "this PC".
pub fn machine_label() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "this PC".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;

    #[test]
    fn start_posts_the_label_and_reads_the_device_facts() {
        let http = FakeHttp::default().reply(
            "https://b/v1/auth/device", 200,
            r#"{"device_code":"dc","user_code":"ABCD-EFGH","verify_url":"https://b/link?code=ABCD-EFGH","expires_in":600,"interval":3}"#,
        );
        let s = start(&http, "https://b", "ADAM-PC").unwrap();
        assert_eq!(s.user_code, "ABCD-EFGH");
        assert_eq!(s.interval, 3);
        let posts = http.posts.lock().unwrap();
        assert_eq!(posts[0].0, "https://b/v1/auth/device");
        assert_eq!(posts[0].1, br#"{"label":"ADAM-PC"}"#.to_vec());
        assert_eq!(posts[0].2, None);
    }

    #[test]
    fn poll_maps_428_410_and_200() {
        let pending = FakeHttp::default().reply("https://b/v1/auth/device/poll", 428, r#"{"ok":false,"status":"pending"}"#);
        assert!(matches!(poll(&pending, "https://b", "dc").unwrap(), Poll::Pending));
        let expired = FakeHttp::default().reply("https://b/v1/auth/device/poll", 410, "");
        assert!(matches!(poll(&expired, "https://b", "dc").unwrap(), Poll::Expired));
        let done = FakeHttp::default().reply(
            "https://b/v1/auth/device/poll", 200,
            r#"{"ok":true,"token":"tok","player":{"id":"p1","display_name":"Adam","avatar_hash":null,"show_offline":false}}"#,
        );
        match poll(&done, "https://b", "dc").unwrap() {
            Poll::Done { token, player } => { assert_eq!(token, "tok"); assert_eq!(player.display_name, "Adam"); }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_debug_print_of_a_poll_result_never_shows_the_token() {
        let p = Poll::Done { token: "s3cr3t".into(), player: PlayerFacts { id: "p".into(), display_name: "A".into(), avatar_hash: None, show_offline: false } };
        assert!(!format!("{p:?}").contains("s3cr3t"));
    }
}
