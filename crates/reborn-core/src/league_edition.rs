//! The League edition: the league's own build of the game (its own ISO, texture tree and game ID), installed from the
//! League tab by members only (spec 2026-10-08 v2 §4).
//!
//! The on-disk layout is Plan 4's (the publisher writes it, this reads it) and is fixed here, never taken from
//! `league.json`: the file only confirms which build is installed. A manifest that put the disc anywhere else would be
//! a different component, and composing textures under a game ID this launcher does not boot would leave the league
//! game with none.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use crate::content::pipeline::ReleaseSource;
use crate::content::{BlobRequest, BlobSource, SignatureVerifier};
use crate::net::Http;
use crate::origin::{FetchedRelease, Origin};
use crate::platform::DataDir;

/// The disc ID the league build carries (its first six bytes). Dolphin files the league's textures and per-game
/// settings under it. Named by Adam 10-09 (contracts file). A stable installer build refuses if this is ever the
/// placeholder (`league_game_id_is_named_before_a_stable_release`).
pub const LEAGUE_GAME_ID: &str = "NSRS04";
/// The pre-naming value. Kept so the stable release gate can tell a named ID from an unnamed one.
pub const LEAGUE_GAME_ID_PLACEHOLDER: &str = "LGXXXX";
/// The retail Reborn disc, whose mod payloads the league build borrows (one shared mod list, spec v2).
pub const RETAIL_GAME_ID: &str = "GN7E69";
pub const LEAGUE_INFO_SCHEMA: &str = "nsr.league-game.v1";

const INFO_FILE: &str = "league.json";
const ISO_PATH: &str = "game/league.iso";
const SEED_PATH: &str = "seed/league.s01";

pub fn league_dir(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::LibraryLeague.relative())
}

pub fn league_image_path(data_root: &Path) -> PathBuf {
    let mut p = league_dir(data_root);
    for seg in ISO_PATH.split('/') {
        p = p.join(seg);
    }
    p
}

/// The pristine league texture tree the composed `Load/Textures/<LEAGUE_GAME_ID>` is built from.
pub fn league_textures_root(data_root: &Path) -> PathBuf {
    league_dir(data_root).join("textures").join(LEAGUE_GAME_ID)
}

pub fn league_info_path(data_root: &Path) -> PathBuf {
    league_dir(data_root).join(INFO_FILE)
}

pub fn league_seed_path(data_root: &Path) -> PathBuf {
    let mut p = data_root.join(DataDir::LibraryLeagueContent.relative());
    for seg in SEED_PATH.split('/') {
        p = p.join(seg);
    }
    p
}

/// `league.json`, as Plan 4's publisher stages it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LeagueInfo {
    pub schema: String,
    pub game_id: String,
    pub iso: String,
    pub textures: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeagueInfoError {
    /// No league build installed.
    Missing,
    /// There, but not a `nsr.league-game.v1` file in Plan 4's layout.
    Unreadable,
    /// A league build for another game ID: this launcher would compose and boot the wrong thing.
    WrongGameId(String),
}

/// The installed build's `league.json`, accepted only when it is exactly the layout this launcher boots.
pub fn read_league_info(data_root: &Path) -> Result<LeagueInfo, LeagueInfoError> {
    let bytes = match std::fs::read(league_info_path(data_root)) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(LeagueInfoError::Missing),
        Err(_) => return Err(LeagueInfoError::Unreadable),
    };
    let info: LeagueInfo = serde_json::from_slice(&bytes).map_err(|_| LeagueInfoError::Unreadable)?;
    if info.schema != LEAGUE_INFO_SCHEMA
        || info.iso != ISO_PATH
        || info.textures != format!("textures/{}", info.game_id)
    {
        return Err(LeagueInfoError::Unreadable);
    }
    if info.game_id != LEAGUE_GAME_ID {
        return Err(LeagueInfoError::WrongGameId(info.game_id));
    }
    Ok(info)
}

/// Why a league backend call did not give what was asked. Never shown as is: reborn-ui turns each into a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeagueError {
    SignedOut,
    NotMember,
    UnknownVersion,
    Unavailable,
    RateLimited,
    Unreachable,
    BadReply,
}

fn league_url(origin: &str, path: &str) -> String {
    format!("{}{}", origin.trim_end_matches('/'), path)
}

/// A refusal by status. The backend's `error` code is the same fact the status already states (`not_member` is the
/// only 403, `unknown_version` the only 404), so the status decides.
fn status_error(status: u16) -> LeagueError {
    match status {
        401 => LeagueError::SignedOut,
        403 => LeagueError::NotMember,
        404 => LeagueError::UnknownVersion,
        429 => LeagueError::RateLimited,
        _ => LeagueError::Unavailable,
    }
}

/// One file of a members-only component, with the signed link the backend issued for it (6 h).
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct GatedFile {
    pub path: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl std::fmt::Debug for GatedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the link: it is a credential for 6 h.
        f.debug_struct("GatedFile")
            .field("path", &self.path)
            .field("sha256", &self.sha256)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct DownloadReply {
    ok: bool,
    component: String,
    version: String,
    files: Vec<GatedFile>,
}

/// `POST {origin}/v1/league/download {component, version}` (Plan 2). Members only: the backend asks the league site
/// first, then signs one short-lived link per blob.
pub fn request_download(
    http: &dyn Http,
    origin: &str,
    token: &str,
    component: &str,
    version: &str,
) -> std::result::Result<Vec<GatedFile>, LeagueError> {
    let body = serde_json::to_vec(&serde_json::json!({ "component": component, "version": version }))
        .map_err(|_| LeagueError::BadReply)?;
    let reply = http
        .post_json(&league_url(origin, "/v1/league/download"), &body, Some(token))
        .map_err(|_| LeagueError::Unreachable)?;
    if !(200..300).contains(&reply.status) {
        return Err(status_error(reply.status));
    }
    let parsed: DownloadReply = serde_json::from_str(&reply.body).map_err(|_| LeagueError::BadReply)?;
    // Links for another component or version would put another build's blobs under this manifest; the hash check
    // would refuse them, but only after downloading. Refused here, before a byte is fetched.
    if !parsed.ok || parsed.component != component || parsed.version != version {
        return Err(LeagueError::BadReply);
    }
    Ok(parsed.files)
}

/// The content pipeline's source for a members-only component: the release and the component manifests come from the
/// public, signed origin exactly as for every other component; the blobs come only from the backend's signed links.
/// A blob with no link is refused, never fetched from the public CDN (where it does not exist).
///
/// `ContentService::download` fetches several files at once through one shared `&LeagueSource`; every link is read
/// from the table, and an expired one fails that file (the `.part` file is kept). The caller then asks the backend
/// again, [`LeagueSource::refresh`]es, and downloads again, which resumes each `.part` file by `Range`.
#[derive(Clone)]
pub struct LeagueSource {
    origin: Origin,
    links: BTreeMap<String, String>,
    on_bytes: Option<Arc<dyn Fn(u64) + Send + Sync>>,
}

impl std::fmt::Debug for LeagueSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the links themselves: they are credentials for 6 h.
        f.debug_struct("LeagueSource").field("origin", &self.origin.base()).field("links", &self.links.len()).finish()
    }
}

impl LeagueSource {
    pub fn new(origin: Origin, files: &[GatedFile]) -> Self {
        let mut s = Self { origin, links: BTreeMap::new(), on_bytes: None };
        s.refresh(files);
        s
    }

    /// Put the backend's links in the table, replacing any older link for the same blob (the links expired, or
    /// another component's were added).
    pub fn refresh(&mut self, files: &[GatedFile]) {
        for f in files {
            self.links.insert(f.sha256.to_ascii_lowercase(), f.url.clone());
        }
    }

    /// Called with the number of bytes each read delivered, for the install progress bar. Called from several
    /// download threads at once.
    pub fn with_progress(mut self, on_bytes: Arc<dyn Fn(u64) + Send + Sync>) -> Self {
        self.on_bytes = Some(on_bytes);
        self
    }
}

struct Counting<R> {
    inner: R,
    on_bytes: Arc<dyn Fn(u64) + Send + Sync>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        (self.on_bytes)(n as u64);
        Ok(n)
    }
}

/// The marker inside the error for a signed link the blob store refused (401/403): typed, so it survives the message
/// being rewritten or replaced, and [`link_expired`] never reads wording.
#[derive(Debug)]
struct ExpiredLink(String);

impl std::fmt::Display for ExpiredLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExpiredLink {}

/// An error from opening a signed link, with the link's query (the signature) taken out of the message, since the
/// message reaches logs. If the signature still shows in any form, only a fixed sentence is kept. A refusal
/// (`PermissionDenied`, which `open_blob_url` gives a 401/403) is marked [`ExpiredLink`].
fn without_signature(err: crate::Error, url: &str, path: &str) -> crate::Error {
    let crate::Error::PlainIo(io) = err else { return err };
    let bare = url.split('?').next().unwrap_or("");
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut msg = io.to_string().replace(url, bare);
    if !query.is_empty() {
        msg = msg.replace(query, "");
    }
    if msg.contains("X-Amz-") || msg.contains("Signature=") {
        msg = format!("the league download link for {path} failed");
    }
    if io.kind() == std::io::ErrorKind::PermissionDenied {
        return crate::Error::PlainIo(std::io::Error::new(io.kind(), ExpiredLink(msg)));
    }
    crate::Error::PlainIo(std::io::Error::new(io.kind(), msg))
}

impl BlobSource for LeagueSource {
    fn open(&self, req: &BlobRequest<'_>) -> crate::Result<Box<dyn Read>> {
        let Some(url) = self.links.get(&req.file.sha256.as_str().to_ascii_lowercase()) else {
            return Err(crate::Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("no download link for {} from the league server", req.file.path),
            )));
        };
        let reader = crate::origin::open_blob_url(self.origin.agent(), url, req)
            .map_err(|e| without_signature(e, url, &req.file.path))?;
        Ok(match &self.on_bytes {
            Some(cb) => Box::new(Counting { inner: reader, on_bytes: cb.clone() }),
            None => reader,
        })
    }
}

impl ReleaseSource for LeagueSource {
    fn fetch_release(&self, verifier: &dyn SignatureVerifier) -> crate::Result<FetchedRelease> {
        self.origin.fetch_release(verifier)
    }

    fn fetch_component(&self, pinned_url: &str) -> crate::Result<Vec<u8>> {
        self.origin.fetch_component(pinned_url)
    }

    fn describe(&self) -> String {
        self.origin.base().to_string()
    }
}

/// A signed link past its 6 h: the blob store answers 403. Asked again of the backend once, then the fetch resumes.
/// Typed: only an error `LeagueSource` marked, never a message that happens to say 403.
pub fn link_expired(err: &crate::Error) -> bool {
    matches!(err, crate::Error::PlainIo(io) if io.get_ref().is_some_and(|inner| inner.is::<ExpiredLink>()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn write_info(root: &std::path::Path, json: &str) {
        let p = league_info_path(root);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, json).unwrap();
    }

    #[test]
    fn the_layout_is_plan_4s() {
        let root = std::path::Path::new("/d");
        assert_eq!(league_dir(root), root.join("library/league"));
        assert_eq!(league_image_path(root), root.join("library/league").join("game").join("league.iso"));
        assert_eq!(league_textures_root(root), root.join("library/league").join("textures").join(LEAGUE_GAME_ID));
        assert_eq!(league_seed_path(root), root.join("library/league-content").join("seed").join("league.s01"));
    }

    #[test]
    fn league_info_must_name_this_launchers_game_id() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Missing));
        write_info(dir.path(), "not json");
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Unreadable));
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v1","game_id":"ZZZZ99","iso":"game/league.iso","textures":"textures/ZZZZ99"}}"#));
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::WrongGameId("ZZZZ99".into())));
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v2","game_id":"{LEAGUE_GAME_ID}","iso":"game/league.iso","textures":"textures/{LEAGUE_GAME_ID}"}}"#));
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Unreadable));
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v1","game_id":"{LEAGUE_GAME_ID}","iso":"elsewhere.iso","textures":"textures/{LEAGUE_GAME_ID}"}}"#));
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Unreadable), "the layout is fixed, never taken from the file");
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v1","game_id":"{LEAGUE_GAME_ID}","iso":"game/league.iso","textures":"textures/{LEAGUE_GAME_ID}"}}"#));
        assert_eq!(read_league_info(dir.path()).unwrap().game_id, LEAGUE_GAME_ID);
    }

    /// Release gate. `installer/windows/build.ps1 -Channel stable` runs this with `--ignored`; it fails if
    /// `LEAGUE_GAME_ID` is ever the placeholder again. A beta or a dev build is not held by it.
    #[test]
    #[ignore = "release gate: run by build.ps1 for the stable channel"]
    fn league_game_id_is_named_before_a_stable_release() {
        assert_ne!(LEAGUE_GAME_ID, LEAGUE_GAME_ID_PLACEHOLDER, "set LEAGUE_GAME_ID in league_edition.rs before a stable release");
        assert!(crate::mods::GameId::parse(LEAGUE_GAME_ID).is_ok());
    }

    #[test]
    fn the_league_game_id_is_a_valid_game_id() {
        assert!(crate::mods::GameId::parse(LEAGUE_GAME_ID).is_ok(), "the tree is filed under this name");
        assert_eq!(LEAGUE_GAME_ID.len(), 6);
        assert_ne!(LEAGUE_GAME_ID, RETAIL_GAME_ID);
    }

    use crate::net::fake::FakeHttp;

    const ORIGIN: &str = "https://api.test";
    const DL: &str = "https://api.test/v1/league/download";
    const SHA: &str = "aa00000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn a_download_request_names_the_component_and_version_with_the_bearer() {
        let body = format!(r#"{{"ok":true,"files":[{{"path":"game/league.iso","url":"https://r2.test/x?sig=1","sha256":"{SHA}","size":6}}],
            "expires_at":"2026-10-08T20:00:00Z","component":"league-game","version":"2026.10.08"}}"#);
        let http = FakeHttp::default().reply(DL, 200, &body);
        let files = request_download(&http, ORIGIN, "tok", "league-game", "2026.10.08").unwrap();
        assert_eq!(files, vec![GatedFile { path: "game/league.iso".into(), url: "https://r2.test/x?sig=1".into(), sha256: SHA.into(), size: 6 }]);
        let posts = http.posts.lock().unwrap();
        assert_eq!((posts[0].0.as_str(), posts[0].2.as_deref()), (DL, Some("tok")));
        let sent: serde_json::Value = serde_json::from_slice(&posts[0].1).unwrap();
        assert_eq!(sent, serde_json::json!({"component":"league-game","version":"2026.10.08"}));
    }

    #[test]
    fn download_refusals_are_named() {
        for (status, body, want) in [
            (401u16, "{}", LeagueError::SignedOut),
            (403, r#"{"ok":false,"error":"not_member"}"#, LeagueError::NotMember),
            (404, r#"{"ok":false,"error":"unknown_version"}"#, LeagueError::UnknownVersion),
            (429, "{}", LeagueError::RateLimited),
            (503, r#"{"ok":false,"error":"league_unavailable"}"#, LeagueError::Unavailable),
            (200, r#"{"files":"nope"}"#, LeagueError::BadReply),
            (200, "<b>Warning</b>", LeagueError::BadReply),
        ] {
            let http = FakeHttp::default().reply(DL, status, body);
            assert_eq!(request_download(&http, ORIGIN, "tok", "league-game", "1").unwrap_err(), want, "{status} {body}");
        }
        assert_eq!(request_download(&FakeHttp::default(), ORIGIN, "tok", "league-game", "1").unwrap_err(), LeagueError::Unreachable);
    }

    #[test]
    fn a_link_never_comes_from_anywhere_but_the_backend() {
        let src = LeagueSource::new(crate::origin::Origin::new("https://cdn.test/"), &[]);
        let file = crate::manifest::ManifestFile {
            path: "game/league.iso".into(),
            bytes: 6,
            sha256: crate::manifest::Sha256Hex::parse(SHA).unwrap(),
        };
        let req = crate::content::BlobRequest { component: "league-game", version: "1", file: &file, offset: 0 };
        let err = crate::content::BlobSource::open(&src, &req).err().expect("no link, no fetch");
        assert!(err.to_string().contains("no download link"), "{err}");
    }

    /// A 200 that does not say `ok`, or answers for another component or version, is not the links that were asked
    /// for: installing from it would put another build's blobs under this one's manifest.
    #[test]
    fn a_download_reply_must_answer_what_was_asked() {
        let reply = |ok: &str, component: &str, version: &str| {
            format!(r#"{{"ok":{ok},"component":"{component}","version":"{version}","expires_at":"2026-10-08T20:00:00Z",
                "files":[{{"path":"game/league.iso","url":"https://r2.test/x?sig=1","sha256":"{SHA}","size":6}}]}}"#)
        };
        for (body, why) in [
            (reply("true", "league-content", "1"), "wrong component"),
            (reply("true", "league-game", "2"), "wrong version"),
            (reply("false", "league-game", "1"), "ok:false"),
            (r#"{"component":"league-game","version":"1","files":[]}"#.to_string(), "no ok"),
        ] {
            let http = FakeHttp::default().reply(DL, 200, &body);
            assert_eq!(request_download(&http, ORIGIN, "tok", "league-game", "1").unwrap_err(), LeagueError::BadReply, "{why}");
        }
        let http = FakeHttp::default().reply(DL, 200, reply("true", "league-game", "1"));
        assert_eq!(request_download(&http, ORIGIN, "tok", "league-game", "1").unwrap().len(), 1);
    }

    #[test]
    fn expired_links_are_requested_again_once() {
        // The signed links last 6 h; the blob store answers 403 for an expired one. Only that is "expired", and it is
        // a typed signal: wording alone (or a 404) never counts.
        assert!(!link_expired(&crate::origin::test_not_found("https://r2.test/x answered 403 for game/league.iso")));
        let (base, _log) = blob_store(std::collections::BTreeMap::new());
        let file = league_iso();
        let req = crate::content::BlobRequest { component: "league-game", version: "1", file: &file, offset: 0 };
        let expired = LeagueSource::new(crate::origin::Origin::new("https://cdn.test/"), &[gated(format!("{base}/blob?sig=old"))]);
        assert!(link_expired(&crate::content::BlobSource::open(&expired, &req).err().expect("403")));
        let missing = LeagueSource::new(crate::origin::Origin::new("https://cdn.test/"), &[]);
        assert!(!link_expired(&crate::content::BlobSource::open(&missing, &req).err().expect("no link")));
    }

    /// When the redaction cannot keep the URL out by removing the query, it keeps only a fixed sentence; the
    /// expired-link signal survives that.
    #[test]
    fn an_expired_link_is_still_named_when_the_message_is_replaced() {
        let (base, _log) = blob_store(std::collections::BTreeMap::new());
        let src = LeagueSource::new(crate::origin::Origin::new("https://cdn.test/"), &[gated(format!("{base}/X-Amz-blob?sig=old"))]);
        let file = league_iso();
        let req = crate::content::BlobRequest { component: "league-game", version: "1", file: &file, offset: 0 };
        let err = crate::content::BlobSource::open(&src, &req).err().expect("expired");
        assert_eq!(err.to_string(), "the league download link for game/league.iso failed");
        assert!(link_expired(&err), "{err}");
    }

    /// A loopback blob store: `/<name>?sig=good` serves `files[name]` honoring `Range`; any other signature (or
    /// name) answers 403, which is what R2 says to an expired presigned link. Every request target and Range offset
    /// is recorded. Shared with the content pipeline's league download test.
    pub(crate) fn blob_store(
        files: std::collections::BTreeMap<String, Vec<u8>>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = log.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let range = head
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(|r| r.trim().trim_end_matches('-').to_string()))
                    .and_then(|r| r.parse::<usize>().ok());
                seen.lock().unwrap().push(format!("{target} range={range:?}"));
                let body = target
                    .strip_prefix('/')
                    .and_then(|t| t.strip_suffix("?sig=good"))
                    .and_then(|name| files.get(name));
                let reply = if let (Some(body), Some(from)) = (body, range) {
                    let mut r = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{}/{}\r\nConnection: close\r\n\r\n",
                        body.len() - from,
                        body.len() - 1,
                        body.len()
                    )
                    .into_bytes();
                    r.extend_from_slice(&body[from..]);
                    r
                } else if let Some(body) = body {
                    let mut r = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
                    r.extend_from_slice(body);
                    r
                } else {
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                };
                let _ = s.write_all(&reply);
            }
        });
        (base, log)
    }

    fn gated(url: String) -> GatedFile {
        // Upper case on purpose: the link table matches a manifest hash whatever the case the backend used.
        GatedFile { path: "game/league.iso".into(), url, sha256: SHA.to_ascii_uppercase(), size: 6 }
    }

    fn league_iso() -> crate::manifest::ManifestFile {
        crate::manifest::ManifestFile {
            path: "game/league.iso".into(),
            bytes: 6,
            sha256: crate::manifest::Sha256Hex::parse(SHA).unwrap(),
        }
    }

    /// Resume through a signed link: the link is opened exactly as issued (no `?v=`, no CDN path) with a `Range`
    /// from the bytes already on disk, and every byte read is counted for the progress bar.
    #[test]
    fn a_signed_link_resumes_from_the_part_file_and_counts_bytes() {
        let (base, log) = blob_store([("blob".to_string(), b"league".to_vec())].into());
        let counted = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let c = counted.clone();
        let src = LeagueSource::new(crate::origin::Origin::new("https://cdn.test/"), &[gated(format!("{base}/blob?sig=good"))])
            .with_progress(std::sync::Arc::new(move |n| {
                c.fetch_add(n, std::sync::atomic::Ordering::SeqCst);
            }));
        let file = league_iso();
        let req = crate::content::BlobRequest { component: "league-game", version: "1", file: &file, offset: 2 };
        let mut out = Vec::new();
        crate::content::BlobSource::open(&src, &req).unwrap().read_to_end(&mut out).unwrap();
        assert_eq!(out, b"ague");
        assert_eq!(counted.load(std::sync::atomic::Ordering::SeqCst), 4);
        assert_eq!(*log.lock().unwrap(), vec!["/blob?sig=good range=Some(2)".to_string()]);
        assert!(!format!("{src:?}").contains("sig="), "Debug never shows a link: {src:?}");
    }

    /// An expired link is a 403 that `link_expired` names, the error never carries the signature, and after
    /// `refresh` with the backend's new link the same source opens the file.
    #[test]
    fn an_expired_link_is_named_without_the_signature_and_refresh_replaces_it() {
        let (base, _log) = blob_store([("blob".to_string(), b"league".to_vec())].into());
        let mut src = LeagueSource::new(crate::origin::Origin::new("https://cdn.test/"), &[gated(format!("{base}/blob?sig=old"))]);
        let file = league_iso();
        let req = crate::content::BlobRequest { component: "league-game", version: "1", file: &file, offset: 0 };
        let err = crate::content::BlobSource::open(&src, &req).err().expect("expired");
        assert!(link_expired(&err), "{err}");
        assert!(!err.to_string().contains("sig=old"), "a signed link is a credential; never in a message: {err}");
        src.refresh(&[gated(format!("{base}/blob?sig=good"))]);
        let mut out = Vec::new();
        crate::content::BlobSource::open(&src, &req).unwrap().read_to_end(&mut out).unwrap();
        assert_eq!(out, b"league");
    }
}
