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

/// The credential vault account a retired uploader stored its bearer token under.
///
/// `league.rs` was the launcher-side port of Dolphin's `NflsStatUpload`, posting the raw
/// capture to `ingest.php` with this token. `backend.rs` replaced it and the module is gone,
/// but a token nothing reads is still a token sitting in the Windows credential vault, and
/// this is one this project has already had to rotate. Named here so [`forget_retired_league_token`]
/// can take it off the machine.
pub const RETIRED_LEAGUE_TOKEN_ACCOUNT: &str = "nsr.league.ingest";

/// Take the retired league credential off this machine.
///
/// Best effort by design, and that is why it returns nothing. There is no version of this a
/// player should ever be told about: they do not know the credential exists, they did not put
/// it there, and there is nothing for them to do about it either way. A vault that refuses is
/// logged for a developer and otherwise ignored; the next start tries again.
///
/// Called at every start of the launcher, not once behind a flag. An installation that was
/// never opened after the change would otherwise keep the credential forever, and "we removed
/// it, unless they had already upgraded" is not removal.
pub fn forget_retired_league_token(platform: &Platform) {
    if let Err(e) = platform.credentials.delete(RETIRED_LEAGUE_TOKEN_ACCOUNT) {
        tracing::warn!("the retired league credential could not be removed: {e}");
    }
}
pub const DEFAULT_ORIGIN: &str = "https://api.nflstreetreborn.com";
pub const PAGE_ORIGIN: &str = "https://online.nflstreetreborn.com";
const IDENTITY_FILE: &str = "identity.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerFacts {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar_hash: Option<String>,
    #[serde(default)]
    pub show_offline: bool,
    /// The name the player chose to be shown by online (backend migration 026). `None` = unset, or an older server,
    /// or an identity file written before nicknames: the Discord name (`display_name`) is used then.
    #[serde(default)]
    pub nickname: Option<String>,
    /// When the server will take the next nickname change (ISO time), or `None` = now.
    #[serde(default)]
    pub nickname_next_change_at: Option<String>,
}

/// The non-secret half of a sign-in, kept beside settings so pages can say who is signed in
/// without asking the vault or the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityFile {
    pub player: PlayerFacts,
    pub signed_in_at_ms: u64,
}

#[derive(Clone, Deserialize)]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verify_url: String,
    pub expires_in: u64,
    pub interval: u64,
}

impl std::fmt::Debug for DeviceStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // device_code is a live pre-authorization secret for its whole expiry window --
        // anyone holding it can poll and take the token the moment the player approves.
        // verify_url embeds the user code too, and a hostile origin could put anything in
        // it. Neither belongs in a Debug print.
        f.debug_struct("DeviceStart")
            .field("user_code", &self.user_code)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
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

// Every refusal in this module is a finished sentence for the player, so all
// of them are built with `Error::player` and one of the named constants below.
//
// They used to be `Error::PlainIo`, which is transparent over an operating
// system error and therefore indistinguishable from one. Anything routing an
// identity failure through `Error::player_message` would have replaced "The
// sign-in service could not be reached. Check your internet connection and try
// again." with "Something unexpected happened. Try again.", which says less
// and is not true.
//
// There was a `plain(msg: String)` helper here that wrapped the constructor.
// It took any string at all and handed it to a screen unchanged, so one
// `format!` written into one of these call sites would have put a path or an
// operating system error number in front of a player with nothing to stop it.
// Every caller passed a constant, which is why nothing was broken and why it
// was worth removing before something was.

fn origin_url(origin: &str, path: &str) -> String {
    format!("{}{}", origin.trim_end_matches('/'), path)
}

/// Shown to the player when the sign-in service could not be reached at all: no route, DNS
/// failure, timeout, connection refused. The one thing a player can do about any of those is
/// check their own connection and try again, so that is the whole message.
const SIGN_IN_SERVICE_UNREACHABLE: &str =
    "The sign-in service could not be reached. Check your internet connection and try again.";

/// Shown to the player when the sign-in service answered but did not behave: a failure status,
/// a reply that could not be parsed, or a reply missing what it needed. A player cannot tell
/// these apart and there is nothing different for them to do about any of them, so they all
/// collapse to this one sentence. The technical detail that would distinguish them goes to
/// `tracing::warn!` instead, the same seam `mods.rs` and `content.rs` already use for detail
/// that is useful to a developer but not to a player.
const SIGN_IN_SERVICE_MISBEHAVED: &str = "The sign-in service is not responding properly right now. Try again in a moment.";

/// Shown to the player when a preference change (such as show-offline) could not be sent to
/// the server because it could not be reached at all. A near-duplicate of `SIGN_IN_SERVICE_UNREACHABLE`
/// on purpose rather than a shared constant: the player is not signing in here, they are
/// toggling a setting while already signed in, and a message that talks about "sign-in" in
/// that moment would be confusing even though it is, under the hood, the same backend.
const SAVE_UNREACHABLE: &str =
    "This could not be saved because the server could not be reached. Check your internet connection and try again.";

/// Shown to the player when a preference change reached the server but the server did not
/// behave: a failure status, or a reply that could not be understood. Same reasoning as
/// `SIGN_IN_SERVICE_MISBEHAVED`, worded for a save instead of a sign-in.
const SAVE_UNAVAILABLE: &str = "This could not be saved right now. Try again in a moment.";

/// Shown when `clear_token` cannot finish: the vault still reports the credential present
/// after a delete. This is reached from `sign_out`, which the player triggered on purpose by
/// asking to sign out, so the message says plainly that it did not fully happen and what to
/// try -- never "token" or "vault", which are our vocabulary, not the player's.
const SIGN_OUT_FAILED: &str = "You are still signed in. Try signing out again.";

/// Shown when a player who is not signed in tries to change a preference such as show-offline
/// (the show-offline checkbox is meant to be hidden while signed out, but `set_show_offline`
/// still guards itself). States plainly that they are signed out and what to do about it.
const MUST_BE_SIGNED_IN: &str = "You are signed out. Sign in to change this setting.";

/// Shown when `open_in_browser` refuses to open a non-http(s) url. Not reachable today -- its
/// one caller discards the error via `.is_ok()` -- fixed anyway so a future caller that does
/// propagate it cannot put a fragment, let alone the url itself, on screen.
const PAGE_NOT_OPENED: &str = "That page could not be opened automatically. Please open it yourself.";

/// The 401 case in `set_show_offline`: the server itself ended the session. This is the
/// wording every other message in this file is held to: short, calm, plain sentences that say
/// what happened and what to do next.
const SESSION_EXPIRED: &str = "Signed out. Sign in again.";

/// Shown when the launcher cannot tell whether anybody is signed in: the facts file will not
/// read, or the credential vault refused. The Play page renders this in the account card, and
/// it used to render the raw error there instead, so a locked Windows credential store put
/// the vault's own wording on the Play screen.
const SIGN_IN_STATE_UNREADABLE: &str =
    "Whether you are signed in could not be read just now. Try again in a moment.";

/// Shown when the launcher cannot work out where the sign-in service is, because its own
/// settings file will not read. Nothing about that is the player's doing and nothing about it
/// is theirs to fix, so the message says what they can see (sign-in is not working) and what
/// to do (wait a moment).
///
/// Named for what happened rather than for what the player is told. This and
/// [`SIGN_IN_SERVICE_MISBEHAVED`] used to be `SIGN_IN_UNAVAILABLE` and
/// `SIGNIN_UNAVAILABLE`: one underscore apart, different sentences, and
/// picking the wrong one at a call site is a one-character mistake that
/// compiles and shows a player a sentence about somebody else's problem.
pub const SIGN_IN_SERVICE_NOT_LOCATED: &str = "Sign-in is not available right now. Try again in a moment.";

/// Shown when the browser step finished and the result could not be written to this PC. The
/// token is already dropped by [`save_signed_in`] in that case, so signing in again is
/// genuinely the right instruction rather than a hopeful one.
const SIGN_IN_NOT_SAVED: &str =
    "You were signed in, but it could not be saved on this PC. Try signing in again.";

pub fn start(http: &dyn Http, origin: &str, label: &str) -> Result<DeviceStart> {
    let body = serde_json::json!({ "label": label });
    let url = origin_url(origin, "/v1/auth/device");
    let bytes = serde_json::to_vec(&body).expect("a serializable request body");
    let r = http.post_json(&url, &bytes, None).map_err(|e| {
        tracing::warn!("sign-in: could not reach {url}: {e}");
        Error::player(SIGN_IN_SERVICE_UNREACHABLE)
    })?;
    if !r.is_success() {
        tracing::warn!("sign-in: {url} answered {}", r.status);
        return Err(Error::player(SIGN_IN_SERVICE_MISBEHAVED));
    }
    serde_json::from_str(&r.body).map_err(|e| {
        tracing::warn!("sign-in: the device-start reply was not understood: {e}");
        Error::player(SIGN_IN_SERVICE_MISBEHAVED)
    })
}

pub fn poll(http: &dyn Http, origin: &str, device_code: &str) -> Result<Poll> {
    #[derive(Deserialize)]
    struct Reply { token: Option<String>, player: Option<PlayerFacts> }
    let body = serde_json::json!({ "device_code": device_code });
    let url = origin_url(origin, "/v1/auth/device/poll");
    let bytes = serde_json::to_vec(&body).expect("a serializable request body");
    let r = http.post_json(&url, &bytes, None).map_err(|e| {
        tracing::warn!("sign-in: could not reach {url}: {e}");
        Error::player(SIGN_IN_SERVICE_UNREACHABLE)
    })?;
    match r.status {
        428 => Ok(Poll::Pending),
        // 410 alone means the code expired. A 404 means the origin or path is wrong; mapping
        // it to Expired would send the player round a device-code loop that can never succeed.
        410 => Ok(Poll::Expired),
        s if (200..300).contains(&s) => {
            let reply: Reply = serde_json::from_str(&r.body).map_err(|e| {
                tracing::warn!("sign-in: the poll reply was not understood: {e}");
                Error::player(SIGN_IN_SERVICE_MISBEHAVED)
            })?;
            match (reply.token, reply.player) {
                (Some(token), Some(player)) => Ok(Poll::Done { token, player }),
                _ => {
                    tracing::warn!("sign-in: the poll reply had no token or player");
                    Err(Error::player(SIGN_IN_SERVICE_MISBEHAVED))
                }
            }
        }
        s => {
            tracing::warn!("sign-in: {url} answered {s}");
            Err(Error::player(SIGN_IN_SERVICE_MISBEHAVED))
        }
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
///
/// Neither half's failure is described to the player in its own words. A vault that refuses
/// and a facts file that will not read are the same thing from where they are sitting -- the
/// launcher cannot tell -- and there is nothing different to do about either.
pub fn status(platform: &Platform) -> Result<Option<IdentityFile>> {
    read_status(platform).map_err(|e| {
        tracing::warn!("the sign-in state could not be read: {e}");
        Error::player(SIGN_IN_STATE_UNREADABLE)
    })
}

fn read_status(platform: &Platform) -> Result<Option<IdentityFile>> {
    let Some(file) = read_identity(&identity_path(platform)?)? else { return Ok(None) };
    Ok(token(platform)?.map(|_| file))
}

/// Where the backend is: the setting, or the product origin.
///
/// Lives here rather than in the window because its one failure -- the launcher's own settings
/// file will not read -- has to become a sentence, and the window has no logger to put the
/// detail in.
pub fn origin(platform: &Platform) -> Result<String> {
    let read = || -> Result<String> {
        let file = crate::settings::settings_path(platform)?;
        let settings = crate::settings::Settings::load(&file)?;
        Ok(settings
            .backend_origin
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_ORIGIN.to_string()))
    };
    read().map_err(|e| {
        tracing::warn!("the backend origin could not be read from settings: {e}");
        Error::player(SIGN_IN_SERVICE_NOT_LOCATED)
    })
}

pub fn token(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(TOKEN_ACCOUNT)
}

pub fn save_signed_in(platform: &Platform, token: &str, player: PlayerFacts, now_ms: u64) -> Result<()> {
    platform.credentials.store(TOKEN_ACCOUNT, token).map_err(|e| {
        tracing::warn!("the sign-in could not be stored in the credential vault: {e}");
        Error::player(SIGN_IN_NOT_SAVED)
    })?;
    let path = identity_path(platform).map_err(|e| {
        tracing::warn!("the identity file path could not be resolved: {e}");
        Error::player(SIGN_IN_NOT_SAVED)
    })?;
    let file = IdentityFile { player, signed_in_at_ms: now_ms };
    let bytes = serde_json::to_vec_pretty(&file).expect("a serializable identity file");
    if let Err(source) = std::fs::write(&path, bytes) {
        // The token is already live in the vault. Leaving it there while the facts file
        // never lands would give the player a working token while every page reads them
        // as signed out, with no way back in through the UI.
        let _ = platform.credentials.delete(TOKEN_ACCOUNT);
        // The path is inside the launcher's own config folder and the wording is the
        // operating system's. Neither belongs on the sign-in card.
        tracing::warn!("the identity file could not be written at {}: {source}", path.display());
        return Err(Error::player(SIGN_IN_NOT_SAVED));
    }
    Ok(())
}

/// The server said 401. Drop the token; keep nothing that would make a page claim a sign-in.
pub fn clear_token(platform: &Platform) -> Result<()> {
    let _ = platform.credentials.delete(TOKEN_ACCOUNT);
    // `delete` alone cannot distinguish a real vault failure from an already-absent entry.
    // Re-read the vault and only report success once it is actually gone; a token that
    // silently survives a "successful" sign-out is the worst outcome this function could
    // produce.
    let still_there = platform.credentials.retrieve(TOKEN_ACCOUNT).map_err(|e| {
        tracing::warn!("the credential vault could not be re-read after a sign-out: {e}");
        Error::player(SIGN_OUT_FAILED)
    })?;
    if still_there.is_some() {
        return Err(Error::player(SIGN_OUT_FAILED));
    }
    // The League tab's last answer belongs to this sign-in: the next player on this PC must not see the tab.
    if let Ok(league) = crate::league_home::cache_path(platform) {
        let _ = std::fs::remove_file(league);
    }
    let path = identity_path(platform).map_err(|e| {
        tracing::warn!("the identity file path could not be resolved: {e}");
        Error::player(SIGN_OUT_FAILED)
    })?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => {
            tracing::warn!("the identity file could not be removed at {}: {source}", path.display());
            Err(Error::player(SIGN_OUT_FAILED))
        }
    }
}

/// Revoke this device on the server (best effort), then forget locally.
pub fn sign_out(platform: &Platform, http: &dyn Http, origin: &str) -> Result<()> {
    if let Some(tok) = token(platform)? {
        let _ = http.delete(&origin_url(origin, "/v1/auth/device"), Some(&tok));
    }
    clear_token(platform)
}

/// The network half of `set_show_offline`: send the preference and get a raw response back.
/// Split out so the one path that can be unreachable -- the server cannot be reached at all --
/// is testable with just a fake `Http`, no `Platform` or vault involved, the same way `start`
/// and `poll` are.
fn post_show_offline(http: &dyn Http, url: &str, tok: &str, show: bool) -> Result<crate::net::HttpResponse> {
    // PATCH is not on the Http trait; the backend accepts the same body on POST /v1/me/show_offline.
    let body = serde_json::to_vec(&serde_json::json!({ "show_offline": show })).expect("a serializable request body");
    http.post_json(url, &body, Some(tok)).map_err(|e| {
        tracing::warn!("show-offline: could not reach {url}: {e}");
        Error::player(SAVE_UNREACHABLE)
    })
}

/// The other half: a non-401 status or a reply body that turned out not to be the expected
/// shape. Also independently testable with a canned [`crate::net::HttpResponse`].
fn parse_show_offline_reply(r: &crate::net::HttpResponse, url: &str) -> Result<PlayerFacts> {
    #[derive(Deserialize)]
    struct Reply { player: PlayerFacts }
    if !r.is_success() {
        tracing::warn!("show-offline: {url} answered {}", r.status);
        return Err(Error::player(SAVE_UNAVAILABLE));
    }
    let reply: Reply = serde_json::from_str(&r.body).map_err(|e| {
        tracing::warn!("show-offline: the reply was not understood: {e}");
        Error::player(SAVE_UNAVAILABLE)
    })?;
    Ok(reply.player)
}

pub fn set_show_offline(platform: &Platform, http: &dyn Http, origin: &str, show: bool) -> Result<PlayerFacts> {
    let vault = token(platform).map_err(|e| {
        tracing::warn!("the credential vault could not be read: {e}");
        Error::player(SAVE_UNAVAILABLE)
    })?;
    let tok = vault.ok_or_else(|| Error::player(MUST_BE_SIGNED_IN))?;
    let url = origin_url(origin, "/v1/me/show_offline");
    let r = post_show_offline(http, &url, &tok, show)?;
    if r.status == 401 { clear_token(platform)?; return Err(Error::player(SESSION_EXPIRED)); }
    let player = parse_show_offline_reply(&r, &url)?;
    let path = identity_path(platform).map_err(|e| {
        tracing::warn!("the identity file path could not be resolved: {e}");
        Error::player(SAVE_UNAVAILABLE)
    })?;
    let existing = read_identity(&path).map_err(|e| {
        tracing::warn!("the identity file could not be read at {}: {e}", path.display());
        Error::player(SAVE_UNAVAILABLE)
    })?;
    let file = match existing {
        Some(mut existing) => { existing.player = player.clone(); existing }
        None => {
            // No facts file to update -- either this is the first write, or the existing one
            // was unreadable. Either way, write a fresh one rather than silently dropping the
            // player's own preference change.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            IdentityFile { player: player.clone(), signed_in_at_ms: now_ms }
        }
    };
    std::fs::write(&path, serde_json::to_vec_pretty(&file).expect("a serializable identity file")).map_err(|source| {
        tracing::warn!("the identity file could not be written at {}: {source}", path.display());
        Error::player(SAVE_UNAVAILABLE)
    })?;
    Ok(player)
}

/// Why a nickname was not saved.
///
/// Not an [`Error::PlayerMessage`]: the server's refusal names a rule the player's own text broke ("That nickname is
/// taken."), so it is a sentence the SERVER wrote for the player and is passed through verbatim, which is exactly
/// what `Error::player`'s `&'static str` rule exists to keep out of that variant. Its own type keeps the two apart.
#[derive(Debug)]
pub enum NicknameError {
    /// The server refused the name (400 invalid, 409 taken, 429 too soon): its `error` sentence, unchanged.
    Refused(String),
    /// Anything else: unreachable, signed out, a 5xx, a reply not understood. This file's own wording.
    Failed(Error),
}

impl std::fmt::Display for NicknameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NicknameError::Refused(m) => f.write_str(m),
            NicknameError::Failed(e) => write!(f, "{e}"),
        }
    }
}

impl From<Error> for NicknameError {
    fn from(e: Error) -> Self { NicknameError::Failed(e) }
}

/// Set (or, with "", clear) the player's nickname: POST /v1/me/nickname (the Http trait has no PUT).
///
/// The server's refusal (`{"ok":false,"error":"..."}` on 400 invalid, 409 taken, 429 too soon) is returned VERBATIM
/// as [`NicknameError::Refused`], so the player reads exactly which rule the name broke.
pub fn set_nickname(platform: &Platform, http: &dyn Http, origin: &str, nickname: &str) -> std::result::Result<PlayerFacts, NicknameError> {
    let vault = token(platform).map_err(|e| {
        tracing::warn!("the credential vault could not be read: {e}");
        Error::player(SAVE_UNAVAILABLE)
    })?;
    let tok = vault.ok_or_else(|| Error::player(MUST_BE_SIGNED_IN))?;
    let url = origin_url(origin, "/v1/me/nickname");
    let r = post_nickname(http, &url, &tok, nickname)?;
    if r.status == 401 { clear_token(platform)?; return Err(Error::player(SESSION_EXPIRED).into()); }
    let player = parse_nickname_reply(&r, &url)?;
    // The facts file follows the server's answer. A file that will not read or write only costs the Settings field
    // its pre-filled value, never the change the server already made.
    match identity_path(platform).map(|path| (read_identity(&path), path)) {
        Ok((Ok(Some(mut file)), path)) => {
            file.player = player.clone();
            if let Err(e) = std::fs::write(&path, serde_json::to_vec_pretty(&file).expect("a serializable identity file")) {
                tracing::warn!("the identity file could not be written at {}: {e}", path.display());
            }
        }
        _ => tracing::warn!("nickname: the identity file was not updated"),
    }
    Ok(player)
}

fn post_nickname(http: &dyn Http, url: &str, tok: &str, nickname: &str) -> Result<crate::net::HttpResponse> {
    let body = serde_json::to_vec(&serde_json::json!({ "nickname": nickname })).expect("a serializable request body");
    http.post_json(url, &body, Some(tok)).map_err(|e| {
        tracing::warn!("nickname: could not reach {url}: {e}");
        Error::player(SAVE_UNREACHABLE)
    })
}

/// The longest server sentence passed through; a longer one is not a message written for a player.
const SERVER_MESSAGE_MAX: usize = 240;

fn parse_nickname_reply(r: &crate::net::HttpResponse, url: &str) -> std::result::Result<PlayerFacts, NicknameError> {
    #[derive(Deserialize)]
    struct Reply { #[serde(default)] player: Option<PlayerFacts>, #[serde(default)] error: Option<String> }
    let reply: Option<Reply> = serde_json::from_str(&r.body).ok();
    if !r.is_success() {
        tracing::warn!("nickname: {url} answered {}", r.status);
        if (400..500).contains(&r.status) {
            if let Some(msg) = reply.and_then(|x| x.error).map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty() && m.chars().count() <= SERVER_MESSAGE_MAX) {
                return Err(NicknameError::Refused(msg));
            }
        }
        return Err(Error::player(SAVE_UNAVAILABLE).into());
    }
    reply.and_then(|x| x.player).ok_or_else(|| {
        tracing::warn!("nickname: the reply was not understood");
        Error::player(SAVE_UNAVAILABLE).into()
    })
}

/// Open a URL in the player's default browser, through the operating system's own shell.
///
/// This used to run `explorer.exe <url>`, and it never worked for a sign-in link. Explorer
/// re-parses its own command line rather than using the argument it was handed, and reads the
/// `?` that begins a query string as a path wildcard, so it decides there is no url and opens
/// the default folder instead: Documents. Every verify_url carries `?code=`, so pressing
/// "Sign in with Discord" opened a Documents window. [`crate::platform::imp::open_url`] asks
/// the shell directly, which takes the url as one string and has no command line to misread.
///
/// A failure is one plain sentence. The url is never logged, never returned, and never put in
/// an error: a verify_url carries the player's own sign-in code.
pub fn open_in_browser(platform: &Platform, url: &str) -> Result<()> {
    // Lowercase a copy before comparing so "HTTPS://..." is accepted rather than refused by
    // accident; the original-case url is still what gets opened.
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        // The url itself is never logged or returned: for a device-start verify_url it can
        // carry the player's own sign-in code, and a hostile origin could put anything in it --
        // the same reason `DeviceStart`'s Debug impl excludes verify_url.
        tracing::warn!("refused to open a link that was not http or https");
        return Err(Error::player(PAGE_NOT_OPENED));
    }
    crate::platform::imp::open_url(platform, url).map_err(|source| {
        // `source` names the shell, or the program that was to be started, and never the url:
        // `ProcessHost::spawn` puts the *program* path in its error, and the Windows arm builds
        // its own fixed sentence. So this line stays inside the rule stated above.
        tracing::warn!("the operating system would not open a link: {source}");
        Error::player(PAGE_NOT_OPENED)
    })
}

/// The device label shown on the backend: the machine name, or "this PC".
pub fn machine_label() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "this PC".to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn signing_out_forgets_the_league_tab() {
        let src = include_str!("identity.rs");
        let body = &src[src.find("pub fn clear_token").unwrap()..src.find("pub fn sign_out").unwrap()];
        assert!(body.contains("league_home::cache_path"), "clear_token no longer removes league.json");
    }

    #[test]
    fn a_nickname_refusal_reaches_the_player_verbatim_and_a_server_fault_does_not() {
        use crate::net::HttpResponse;
        let url = "https://b/v1/me/nickname";
        let taken = HttpResponse { status: 409, body: r#"{"ok":false,"error":"That nickname is taken."}"#.into() };
        assert_eq!(parse_nickname_reply(&taken, url).unwrap_err().to_string(), "That nickname is taken.");
        let soon = HttpResponse { status: 429, body: r#"{"ok":false,"error":"You can change your nickname again in 23 hours.","retry_at":"2026-10-07T12:00:00.000Z"}"#.into() };
        assert!(matches!(parse_nickname_reply(&soon, url), Err(NicknameError::Refused(m)) if m.starts_with("You can change")));
        let down = HttpResponse { status: 500, body: r#"{"ok":false,"error":"stack trace here"}"#.into() };
        assert_eq!(parse_nickname_reply(&down, url).unwrap_err().to_string(), SAVE_UNAVAILABLE);
        let ok = HttpResponse { status: 200, body: r#"{"ok":true,"player":{"id":"p1","display_name":"adam_discord","nickname":"Slick","name":"Slick","show_offline":false,"nickname_next_change_at":"2026-10-07T12:00:00.000Z"}}"#.into() };
        let p = parse_nickname_reply(&ok, url).unwrap();
        assert_eq!((p.display_name.as_str(), p.nickname.as_deref()), ("adam_discord", Some("Slick")));
        assert_eq!(p.nickname_next_change_at.as_deref(), Some("2026-10-07T12:00:00.000Z"));
    }

    #[test]
    fn player_facts_from_an_older_server_or_file_have_no_nickname() {
        let p: PlayerFacts = serde_json::from_str(r#"{"id":"p1","display_name":"Adam","avatar_hash":null,"show_offline":false}"#).unwrap();
        assert_eq!(p.nickname, None);
        assert_eq!(p.nickname_next_change_at, None);
    }
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
    fn poll_maps_404_to_an_error_not_expired() {
        let wrong_path = FakeHttp::default().reply("https://b/v1/auth/device/poll", 404, "");
        // A 404 must show up as an error, never as Expired -- Expired would send the player
        // round a device-code loop that can never succeed. (Message wording is covered by
        // `every_identity_error_reads_as_a_plain_sentence` below.)
        assert!(poll(&wrong_path, "https://b", "dc").is_err());
    }

    /// A player can see the message from every failing path this file has that a `reborn-ui`
    /// command can reach: `start`, `poll`, the two network halves of `set_show_offline`, and
    /// `open_in_browser`'s link check. Renamed from `every_sign_in_error_...` (task 10b round 1)
    /// because it now covers more than sign-in: `set_show_offline` is a settings toggle, not a
    /// sign-in step, but it is reachable from the same screen through the same kind of network
    /// call and was found to leak the same way.
    ///
    /// Every message is checked against five things, mechanically rather than as a list of
    /// expected strings, so a future edit cannot slip a bad one past this test just by not
    /// being on a hand-maintained list:
    /// - no scheme word ("http")
    /// - no URL ("://")
    /// - no three-or-more-digit run (a status code)
    /// - starts with a capital letter
    /// - ends with a full stop, a question mark, or an exclamation mark (a sentence, not a
    ///   fragment -- task 10b round 3, after `"not signed in"` and
    ///   `"could not remove the stored sign-in token; try again"` reached this test as
    ///   fragments and were rewritten rather than the check being loosened to let them through)
    #[test]
    fn every_identity_error_reads_as_a_plain_sentence() {
        // The five rules moved to `crate::player_sentence` in fix wave 2, when three more
        // modules needed exactly the same check. A sixth rule came with them: no `\` and no
        // `/`, because the leaks found in `stats.rs` and here were file paths rather than
        // status codes.
        use crate::player_sentence::assert_player_safe;

        // start: the server cannot be reached at all (no canned reply -> a transport failure).
        let start_unreachable = FakeHttp::default();
        assert_player_safe(&start(&start_unreachable, "https://b", "ADAM-PC").unwrap_err().to_string());

        // start: the server answers, but with a failure status.
        let start_failure = FakeHttp::default().reply("https://b/v1/auth/device", 500, "internal error");
        assert_player_safe(&start(&start_failure, "https://b", "ADAM-PC").unwrap_err().to_string());

        // start: a 200 whose body is not the expected shape.
        let start_garbage = FakeHttp::default().reply("https://b/v1/auth/device", 200, "not json");
        assert_player_safe(&start(&start_garbage, "https://b", "ADAM-PC").unwrap_err().to_string());

        // poll: the server cannot be reached at all.
        let poll_unreachable = FakeHttp::default();
        assert_player_safe(&poll(&poll_unreachable, "https://b", "dc").unwrap_err().to_string());

        // poll: a failure status that is neither 428 pending nor 410 expired.
        let poll_failure = FakeHttp::default().reply("https://b/v1/auth/device/poll", 500, "internal error");
        assert_player_safe(&poll(&poll_failure, "https://b", "dc").unwrap_err().to_string());

        // poll: a 200 whose body is not the expected shape.
        let poll_garbage = FakeHttp::default().reply("https://b/v1/auth/device/poll", 200, "not json");
        assert_player_safe(&poll(&poll_garbage, "https://b", "dc").unwrap_err().to_string());

        // poll: a 200 with valid JSON that is missing the token and player.
        let poll_incomplete = FakeHttp::default().reply("https://b/v1/auth/device/poll", 200, r#"{"ok":true}"#);
        assert_player_safe(&poll(&poll_incomplete, "https://b", "dc").unwrap_err().to_string());

        // poll: the 404-is-not-Expired case must also read as a plain sentence.
        let poll_wrong_path = FakeHttp::default().reply("https://b/v1/auth/device/poll", 404, "");
        assert_player_safe(&poll(&poll_wrong_path, "https://b", "dc").unwrap_err().to_string());

        // set_show_offline (via its network halves): the server cannot be reached at all.
        let show_unreachable = FakeHttp::default();
        assert_player_safe(&post_show_offline(&show_unreachable, "https://b/v1/me/show_offline", "tok", true).unwrap_err().to_string());

        // set_show_offline: the server answers, but with a failure status.
        let show_failure = FakeHttp::default().reply("https://b/v1/me/show_offline", 500, "internal error");
        let r = post_show_offline(&show_failure, "https://b/v1/me/show_offline", "tok", true).unwrap();
        assert_player_safe(&parse_show_offline_reply(&r, "https://b/v1/me/show_offline").unwrap_err().to_string());

        // set_show_offline: a 200 whose body is not the expected shape.
        let show_garbage = FakeHttp::default().reply("https://b/v1/me/show_offline", 200, "not json");
        let r = post_show_offline(&show_garbage, "https://b/v1/me/show_offline", "tok", true).unwrap();
        assert_player_safe(&parse_show_offline_reply(&r, "https://b/v1/me/show_offline").unwrap_err().to_string());

        // open_in_browser: a non-http url is refused, and its error must not repeat the url back.
        let platform = crate::platform::imp::platform();
        assert_player_safe(&open_in_browser(&platform, "file:///C:/secrets.txt").unwrap_err().to_string());
        assert_player_safe(&open_in_browser(&platform, "javascript:alert(1)").unwrap_err().to_string());

        // clear_token's "the vault still has it" message and set_show_offline's "you are
        // signed out" guard are not reachable through a fake Http or a fake vault (this crate
        // has no in-memory CredentialVault, and using the real OS vault under the real
        // TOKEN_ACCOUNT would risk clobbering a signed-in session on whatever machine runs this
        // test). Their wording is still locked down here directly, so a future edit that turns
        // either back into a fragment fails this test even without a live vault.
        assert_player_safe(SIGN_OUT_FAILED);
        assert_player_safe(MUST_BE_SIGNED_IN);
        assert_player_safe(SESSION_EXPIRED);

        // Fix wave 2. Four more paths a window command can reach, all of which used to hand
        // the player the raw error: `status` (the account card's own read, which rendered a
        // locked credential vault's wording on the Play screen), `origin` (the launcher's own
        // settings file), `save_signed_in` (the config folder) and the vault re-read in
        // `clear_token`. None of the four is reachable with a fake vault -- this crate has no
        // in-memory `CredentialVault`, and using the real one under the real `TOKEN_ACCOUNT`
        // would risk clobbering a signed-in session on whatever machine runs this test -- so
        // their wording is locked down directly, exactly as the three above are.
        assert_player_safe(SIGN_IN_STATE_UNREADABLE);
        assert_player_safe(SIGN_IN_SERVICE_NOT_LOCATED);
        assert_player_safe(SIGN_IN_NOT_SAVED);
    }

    /// No two message constants in this module have names that differ only by
    /// punctuation.
    ///
    /// `SIGN_IN_SERVICE_MISBEHAVED` and `SIGN_IN_SERVICE_NOT_LOCATED` both existed, with
    /// different wording, one underscore apart: "The sign-in service is not
    /// responding properly right now." for a server that answered badly, and
    /// "Sign-in is not available right now." for a launcher that could not
    /// work out where the server is. Picking the wrong one at a call site is
    /// a one-character mistake that compiles, passes every test, and shows a
    /// player a sentence about somebody else's problem.
    ///
    /// # How this is bounded so it cannot find itself
    ///
    /// It reads this file down to the `#[cfg(test)]` that begins this module,
    /// so the constant names quoted in this test are never part of what is
    /// scanned.
    #[test]
    fn no_two_message_constants_here_are_one_underscore_apart() {
        let src = include_str!("identity.rs");
        let production = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];

        let mut seen: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        for line in production.lines() {
            let line = line.trim_start();
            let rest = line
                .strip_prefix("pub const ")
                .or_else(|| line.strip_prefix("const "));
            let Some(rest) = rest else { continue };
            let Some(name) = rest.split(':').next().map(str::trim) else { continue };
            if name.is_empty() {
                continue;
            }
            let flattened: String = name
                .chars()
                .filter(|c| *c != '_')
                .map(|c| c.to_ascii_lowercase())
                .collect();
            if let Some(first) = seen.insert(flattened.clone(), name.to_string()) {
                panic!(
                    "{first} and {name} are the same name with different punctuation, and \
                     they hold different sentences: picking the wrong one at a call site is \
                     a one-character mistake that compiles"
                );
            }
        }
        assert!(
            seen.len() > 10,
            "only {} constants were found, so this scan is not reading what it thinks",
            seen.len()
        );
    }

    #[test]
    fn a_debug_print_of_a_poll_result_never_shows_the_token() {
        let p = Poll::Done { token: "s3cr3t".into(), player: PlayerFacts { id: "p".into(), display_name: "A".into(), avatar_hash: None, show_offline: false, nickname: None, nickname_next_change_at: None } };
        assert!(!format!("{p:?}").contains("s3cr3t"));
    }

    #[test]
    fn a_debug_print_of_a_device_start_never_shows_the_device_code() {
        let d = DeviceStart {
            device_code: "d3vic3-c0d3-s3cr3t".into(),
            user_code: "ABCD-EFGH".into(),
            verify_url: "https://b/link?code=ABCD-EFGH".into(),
            expires_in: 600,
            interval: 3,
        };
        let printed = format!("{d:?}");
        assert!(!printed.contains("d3vic3-c0d3-s3cr3t"));
        assert!(!printed.contains("verify_url"));
        assert!(!printed.contains("https://"));
        assert!(printed.contains("ABCD-EFGH"));
    }

    #[test]
    fn open_in_browser_refuses_a_non_http_url() {
        let platform = crate::platform::imp::platform();
        assert!(open_in_browser(&platform, "file:///C:/secrets.txt").is_err());
        assert!(open_in_browser(&platform, "javascript:alert(1)").is_err());
        assert!(open_in_browser(&platform, "not-a-url").is_err());

        // Added with the shell fix, because the shell will open far more than a browser will.
        // A bare path and a UNC share are folders, and a folder is exactly what the broken
        // version used to open; `data:` and `vbscript:` are the other two schemes that run
        // code; an empty string is what an origin that answered without a verify_url leaves;
        // and a leading space is the one way a url that starts with "https://" once it is
        // trimmed can still reach here untrimmed.
        assert!(open_in_browser(&platform, "").is_err());
        assert!(open_in_browser(&platform, r"C:\Users\Adam\Documents").is_err());
        assert!(open_in_browser(&platform, r"\\server\share").is_err());
        assert!(open_in_browser(&platform, "data:text/html,<script>alert(1)</script>").is_err());
        assert!(open_in_browser(&platform, "vbscript:msgbox(1)").is_err());
        assert!(open_in_browser(&platform, " https://example.com/").is_err());
        assert!(open_in_browser(&platform, "ftp://example.com/").is_err());

        // Every refusal is the same sentence and says nothing about the link, whatever the
        // link was. `every_identity_error_reads_as_a_plain_sentence` holds the wording itself
        // to the six rules; this holds the refusal to that one wording.
        for bad in [
            "file:///C:/secrets.txt",
            "data:text/html,x",
            r"C:\Users\Adam\Documents",
            "vbscript:msgbox(1)",
        ] {
            let message = open_in_browser(&platform, bad).unwrap_err().to_string();
            assert_eq!(message, PAGE_NOT_OPENED);
            assert!(!message.contains("secrets"));
            assert!(!message.contains("Documents"));
        }
    }

    /// The one thing a passing `Result` cannot tell you.
    ///
    /// Not run by `cargo test`: it opens a real browser window, which no automated run should
    /// do. Run it by hand, watch the screen, and close what it opens:
    ///
    /// ```text
    /// cargo test -p reborn-core --lib -- --ignored --nocapture the_shell_really_opens
    /// ```
    ///
    /// The url has a query string on purpose. That is the whole bug: `explorer.exe` handed
    /// `https://example.com/?probe=1` opened a Documents window and returned success, so a
    /// test that asserted only `is_ok()` passed while the feature was broken in front of a
    /// player. `is_ok()` here still is not the proof. A browser on screen, and no file
    /// manager, is.
    #[test]
    #[ignore = "opens a real browser window; run it by hand and watch the screen"]
    fn the_shell_really_opens_a_link_with_a_query_string() {
        let platform = crate::platform::imp::platform();
        open_in_browser(&platform, "https://example.com/?probe=1")
            .expect("the shell accepted the link");
    }
}
