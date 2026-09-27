//! Where releases and blobs come from over the network.
//!
//! [`crate::content`] has no network stack and must not grow one: every rule
//! it tests is about what happens to bytes after they arrive. This module is
//! the other side of that line. It knows three URLs and one header, and it
//! hands the content engine a [`TrustedRelease`] it could not have opened
//! itself and a [`BlobSource`] that streams from an offset.
//!
//! # Plain static HTTP, on purpose
//!
//! `CONTENT_AND_UPDATE_MODEL.md` section 5A: a `GET` per blob, `Range` for
//! resume, no server-side logic at all. That is what lets the whole origin be
//! a bucket behind a CDN, which is the cheapest thing that exists and the only
//! thing that scales to 20,000 players pulling 4 GB. Nothing here would need
//! to change to move between R2, S3, Bunny or a folder served by Python on a
//! laptop, and the tests use the last of those.
//!
//! # The layout the origin serves
//!
//! ```text
//!   <base>/release.json              the one signed object
//!   <base>/release.json.sig          128 hex chars, its Ed25519 signature
//!   <base>/components/<name>-<version>.json   pinned by sha256 from the release
//!   <base>/blobs/by-hash/<xx>/<sha256>        one immutable object per distinct
//!                                             file content, addressed by hash
//! ```
//!
//! Content addressing is the whole reason a texture pack update costs a few
//! megabytes instead of two gigabytes: 2,252 of the online pack's files are
//! byte-identical to the offline pack's, and under this layout they are one
//! object each, uploaded once and fetched once.
//!
//! # A host that ignores `Range` is refused, not tolerated
//!
//! [`BlobRequest::offset`] is a promise: the engine has that many good bytes
//! on disk and wants the rest. A host that answers `200` from the start would
//! have those bytes appended onto the partial file, which the hash check does
//! catch, but only after the user paid for the whole file twice and only as
//! "corrupt" rather than as the missing feature it is. So a `200` to a ranged
//! request is an error here, before a byte is written.

use std::io::Read;
use std::time::Duration;

use crate::content::{BlobRequest, BlobSource, SignatureVerifier, TrustedRelease};
use crate::manifest::Sha256Hex;
use crate::signing::decode_signature;
use crate::{Error, Result};

/// The signed release manifest, relative to the origin base.
/// Where the product's content comes from. Cloudflare R2 behind the
/// custom domain, with `blobs/*` cached at the edge for a year and
/// `release.json` never cached. See docs/RUNNING_UPDATES.md section 7.
pub const DEFAULT_ORIGIN: &str = "https://files.nflstreetreborn.com/";

pub const RELEASE_FILE: &str = "release.json";
/// Its detached signature, hex text, beside it.
pub const SIGNATURE_FILE: &str = "release.json.sig";
/// The content-addressed blob store, relative to the origin base.
pub const BLOB_DIR: &str = "blobs/by-hash";

/// A manifest larger than this is refused unread. The texture component, the
/// largest, is about 1.3 MB for 8,050 rows; this is fifty times that, and it
/// exists so a hostile or broken origin cannot stream a manifest forever.
const MANIFEST_CAP: u64 = 64 * 1024 * 1024;

/// Where a blob lives, relative to the origin base: two hex characters of
/// fan-out, then the full hash. Two characters gives 256 directories, which
/// keeps a listing of 10,000 objects readable and is what every object store
/// prefix scheme settles on.
pub fn blob_path(sha256: &Sha256Hex) -> String {
    let s = sha256.as_str();
    format!("{BLOB_DIR}/{}/{s}", &s[..2])
}

/// A release manifest that verified, and the exact bytes it verified over,
/// kept so a caller can store them beside the pointer for the next check.
#[derive(Debug)]
pub struct FetchedRelease {
    pub trusted: TrustedRelease,
    pub bytes: Vec<u8>,
}

/// One origin: a base URL and an HTTP agent with bounded timeouts.
#[derive(Debug, Clone)]
pub struct Origin {
    base: String,
    agent: ureq::Agent,
}

impl Origin {
    /// An origin at `base`, which is normalized to end in one slash so that
    /// joining a relative path never produces `//` or a missing separator.
    pub fn new(base: &str) -> Self {
        let mut base = base.trim().to_string();
        if !base.ends_with('/') {
            base.push('/');
        }
        // Connect: long enough for a phone hotspot to answer, short enough
        // that a dead origin does not hang a launch. Read: per read call, not
        // per file, so a 700 MB image downloads fine while a stalled socket
        // still gives up in a minute.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(60))
            .build();
        Self { base, agent }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// A relative path joined to the base. An absolute URL is returned as it
    /// is, so a release may pin a component manifest on another host.
    pub fn url(&self, relative: &str) -> String {
        if relative.starts_with("http://") || relative.starts_with("https://") {
            return relative.to_string();
        }
        format!("{}{}", self.base, relative.trim_start_matches('/'))
    }

    /// Fetch `release.json` and its signature, verify, and open.
    ///
    /// The signature is checked over the exact bytes the wire delivered, never
    /// a re-serialized copy: re-serializing JSON is how signature checks start
    /// passing on modified content.
    pub fn fetch_release(&self, verifier: &dyn SignatureVerifier) -> Result<FetchedRelease> {
        let bytes = self.get_bytes(&self.url(RELEASE_FILE))?;
        let sig_url = self.url(SIGNATURE_FILE);
        let sig_text = self.get_bytes(&sig_url)?;
        let sig_text = String::from_utf8_lossy(&sig_text);
        let signature = decode_signature(&sig_text).ok_or_else(|| {
            invalid(format!(
                "{sig_url} is not a signature: expected 128 hex characters, got {} \
                 byte(s). The release was not opened and nothing was changed",
                sig_text.trim().len()
            ))
        })?;
        let trusted = TrustedRelease::open(verifier, &bytes, &signature)?;
        Ok(FetchedRelease { trusted, bytes })
    }

    /// Fetch a component manifest by the URL the release pinned it at. The
    /// bytes come back unparsed: the content engine checks them against the
    /// release's hash pin before it reads a single field.
    pub fn fetch_component(&self, pinned_url: &str) -> Result<Vec<u8>> {
        self.get_bytes(&self.url(pinned_url))
    }

    /// A whole small body, capped.
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        match self.agent.get(url).call() {
            Ok(resp) => {
                let mut out = Vec::new();
                resp.into_reader()
                    .take(MANIFEST_CAP + 1)
                    .read_to_end(&mut out)
                    .map_err(|e| transport(url, e))?;
                if out.len() as u64 > MANIFEST_CAP {
                    return Err(invalid(format!(
                        "{url} is larger than {} MB, which no manifest is. Refused unread",
                        MANIFEST_CAP / (1024 * 1024)
                    )));
                }
                Ok(out)
            }
            Err(ureq::Error::Status(status, _)) => Err(not_found(format!("{url} answered {status}"))),
            Err(ureq::Error::Transport(t)) => Err(transport(url, t)),
        }
    }
}

impl BlobSource for Origin {
    fn open(&self, req: &BlobRequest<'_>) -> Result<Box<dyn Read>> {
        // A blob is immutable and named by its hash, so `?v=` costs nothing and defeats a cached
        // 404: the CDN keeps a "not found" for hours when a URL is asked for before its object is
        // uploaded, and the bare URL then fails every client's fetch (09-26, two seed files; the
        // same lever the launcher's own update URL has always carried).
        let url = format!("{}?v={}", self.url(&blob_path(&req.file.sha256)), &req.file.sha256.as_str()[..8]);
        let mut request = self.agent.get(&url);
        if req.offset > 0 {
            request = request.set("Range", &format!("bytes={}-", req.offset));
        }
        match request.call() {
            Ok(resp) => {
                if req.offset > 0 && resp.status() != 206 {
                    return Err(invalid(format!(
                        "{url} ignored the Range header and answered {} from the start. \
                         Resuming {} needs a host that honors Range; nothing was written \
                         and the {} byte(s) already downloaded are kept",
                        resp.status(),
                        req.file.path,
                        req.offset
                    )));
                }
                Ok(Box::new(resp.into_reader()))
            }
            Err(ureq::Error::Status(416, _)) => Err(invalid(format!(
                "{url}: byte {} is past the end of the blob, so the partial file for {} \
                 is longer than the published one. Nothing was written; the hash check \
                 decides what happens to the partial file",
                req.offset, req.file.path
            ))),
            Err(ureq::Error::Status(status, _)) => Err(not_found(format!(
                "{url} answered {status} for {}. Nothing was written",
                req.file.path
            ))),
            Err(ureq::Error::Transport(t)) => Err(transport(&url, t)),
        }
    }
}

fn invalid(msg: String) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

fn not_found(msg: String) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::NotFound, msg))
}

fn transport(url: &str, e: impl std::fmt::Display) -> Error {
    Error::PlainIo(std::io::Error::other(format!("{url}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ManifestFile;
    use ed25519_dalek::{Signer as _, SigningKey};
    use std::collections::BTreeMap;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A one-file-per-request HTTP server on a loopback port, so the tests
    /// exercise the real agent, real headers and real status codes without a
    /// network. `honor_range` is the switch between a proper host and one
    /// that serves the whole object whatever was asked, which is what a
    /// misconfigured CDN does.
    struct Server {
        base: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    fn serve(files: BTreeMap<String, Vec<u8>>, honor_range: bool) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                // The request target as sent (with any `?v=`), and the path the files map is keyed by.
                let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let path = target.split('?').next().unwrap_or("/").to_string();
                let range = head
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("range: bytes="))
                    .and_then(|l| l[13..].trim().trim_end_matches('-').parse::<usize>().ok());
                log.lock().expect("log").push(format!(
                    "{target}{}",
                    range.map(|r| format!(" from {r}")).unwrap_or_default()
                ));
                let (status, body): (&str, Vec<u8>) = match files.get(&path) {
                    None => ("404 Not Found", Vec::new()),
                    Some(b) => match (range, honor_range) {
                        (Some(from), true) if from > b.len() => ("416 Range Not Satisfiable", Vec::new()),
                        (Some(from), true) => ("206 Partial Content", b[from..].to_vec()),
                        _ => ("200 OK", b.clone()),
                    },
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(&body);
            }
        });
        Server { base: format!("http://{addr}/"), requests }
    }

    fn blob(bytes: &[u8]) -> (String, ManifestFile) {
        let sha = Sha256Hex::from_bytes(bytes);
        let path = format!("/{}", blob_path(&sha));
        (path, ManifestFile { path: "a/file.bin".into(), bytes: bytes.len() as u64, sha256: sha })
    }

    fn read_all(mut r: Box<dyn Read>) -> Vec<u8> {
        let mut v = Vec::new();
        r.read_to_end(&mut v).expect("read");
        v
    }

    #[test]
    fn a_blob_is_addressed_by_two_hex_characters_of_fan_out_then_its_full_hash() {
        let sha = Sha256Hex::from_bytes(b"football");
        let p = blob_path(&sha);
        assert!(p.starts_with("blobs/by-hash/"));
        let rest = &p["blobs/by-hash/".len()..];
        let (dir, name) = rest.split_once('/').expect("dir/name");
        assert_eq!(dir.len(), 2);
        assert_eq!(name, sha.as_str());
        assert_eq!(&name[..2], dir, "the directory is the start of the hash, not something else");
    }

    #[test]
    fn urls_join_cleanly_whatever_the_base_looks_like_and_absolute_ones_pass_through() {
        for base in ["http://h/x", "http://h/x/", " http://h/x/ "] {
            let o = Origin::new(base);
            assert_eq!(o.url("release.json"), "http://h/x/release.json", "base {base:?}");
            assert_eq!(o.url("/release.json"), "http://h/x/release.json", "base {base:?}");
        }
        let o = Origin::new("http://h/x");
        assert_eq!(o.url("https://elsewhere/c.json"), "https://elsewhere/c.json");
    }

    #[test]
    fn a_fresh_download_asks_from_zero_and_gets_the_whole_blob() {
        let data = b"0123456789".to_vec();
        let (path, file) = blob(&data);
        let srv = serve(BTreeMap::from([(path, data.clone())]), true);
        let o = Origin::new(&srv.base);

        let got = read_all(
            o.open(&BlobRequest { component: "textures", version: "1", file: &file, offset: 0 })
                .expect("open"),
        );
        assert_eq!(got, data);
        let log = srv.requests.lock().expect("log").clone();
        assert_eq!(log.len(), 1);
        assert!(!log[0].contains("from"), "no Range header on a fresh fetch: {log:?}");
        // The cache-buster: a CDN that cached a 404 for the bare URL must never answer a blob fetch (09-26).
        assert!(log[0].contains(&format!("?v={}", &file.sha256.as_str()[..8])), "blob fetched with ?v=<sha8>: {log:?}");
    }

    #[test]
    fn a_resume_sends_range_and_a_proper_host_serves_only_the_tail() {
        let data = b"0123456789".to_vec();
        let (path, file) = blob(&data);
        let srv = serve(BTreeMap::from([(path, data.clone())]), true);
        let o = Origin::new(&srv.base);

        let got = read_all(
            o.open(&BlobRequest { component: "textures", version: "1", file: &file, offset: 7 })
                .expect("open"),
        );
        assert_eq!(got, b"789", "only the bytes after offset 7");
        let log = srv.requests.lock().expect("log").clone();
        assert!(log[0].ends_with(" from 7"), "the Range header carried the offset: {log:?}");
    }

    #[test]
    fn a_host_that_ignores_range_is_refused_before_a_byte_is_written() {
        // Same server, honor_range off: it answers 200 with everything.
        let data = b"0123456789".to_vec();
        let (path, file) = blob(&data);
        let srv = serve(BTreeMap::from([(path, data)]), false);
        let o = Origin::new(&srv.base);

        let outcome = o.open(&BlobRequest { component: "textures", version: "1", file: &file, offset: 7 });
        let msg = outcome.err().expect("a 200 to a ranged request is a refusal").to_string();
        assert!(msg.contains("ignored the Range header"), "{msg}");
        assert!(msg.contains("7 byte(s) already downloaded are kept"), "says what remains safe: {msg}");
    }

    #[test]
    fn a_missing_blob_names_the_file_and_the_status() {
        let (_, file) = blob(b"never uploaded");
        let srv = serve(BTreeMap::new(), true);
        let o = Origin::new(&srv.base);
        let msg = o
            .open(&BlobRequest { component: "textures", version: "1", file: &file, offset: 0 })
            .err()
            .expect("404")
            .to_string();
        assert!(msg.contains("answered 404"), "{msg}");
        assert!(msg.contains("a/file.bin"), "{msg}");
    }

    // -- the release --------------------------------------------------------

    fn keypair(seed: u8) -> (SigningKey, [u8; 32]) {
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let pk = sk.verifying_key().to_bytes();
        (sk, pk)
    }

    const RELEASE: &[u8] =
        br#"{"schema":"nsr.release.v1","version":"2.4.0","sequence":12,"components":{}}"#;

    fn origin_with_release(sig_text: &str) -> Server {
        serve(
            BTreeMap::from([
                ("/release.json".to_string(), RELEASE.to_vec()),
                ("/release.json.sig".to_string(), sig_text.as_bytes().to_vec()),
            ]),
            true,
        )
    }

    #[test]
    fn a_signed_release_opens_and_the_bytes_it_verified_over_come_back_unchanged() {
        let (sk, pk) = keypair(9);
        let sig = crate::signing::encode_signature(&sk.sign(RELEASE).to_bytes());
        let srv = origin_with_release(&format!("{sig}\n"));
        let verifier = crate::signing::Ed25519Verifier::with_keys(&[pk]).expect("key");

        let fetched = Origin::new(&srv.base).fetch_release(&verifier).expect("opens");
        assert_eq!(fetched.trusted.release().version, "2.4.0");
        assert_eq!(fetched.trusted.release().sequence, 12);
        assert_eq!(fetched.bytes, RELEASE, "kept exactly as served, for caching");
    }

    #[test]
    fn a_release_signed_by_the_wrong_key_does_not_open() {
        let (stranger, _) = keypair(1);
        let (_, ours) = keypair(2);
        let sig = crate::signing::encode_signature(&stranger.sign(RELEASE).to_bytes());
        let srv = origin_with_release(&sig);
        let verifier = crate::signing::Ed25519Verifier::with_keys(&[ours]).expect("key");

        let msg = Origin::new(&srv.base).fetch_release(&verifier).expect_err("refused").to_string();
        assert!(msg.contains("did not verify"), "{msg}");
        assert!(msg.contains("nothing was changed"), "{msg}");
    }

    #[test]
    fn a_signature_file_that_is_not_a_signature_is_named_and_the_release_is_not_opened() {
        let (_, pk) = keypair(3);
        let srv = origin_with_release("this is not hex\n");
        let verifier = crate::signing::Ed25519Verifier::with_keys(&[pk]).expect("key");
        let msg = Origin::new(&srv.base).fetch_release(&verifier).expect_err("refused").to_string();
        assert!(msg.contains("release.json.sig is not a signature"), "{msg}");
    }

    #[test]
    fn a_missing_signature_file_is_an_outage_not_an_update() {
        let (_, pk) = keypair(4);
        let srv = serve(BTreeMap::from([("/release.json".to_string(), RELEASE.to_vec())]), true);
        let verifier = crate::signing::Ed25519Verifier::with_keys(&[pk]).expect("key");
        let msg = Origin::new(&srv.base).fetch_release(&verifier).expect_err("refused").to_string();
        assert!(msg.contains("release.json.sig answered 404"), "{msg}");
    }

    #[test]
    fn a_component_manifest_is_fetched_raw_so_the_engine_can_check_the_pin_first() {
        let body = b"{\"schema\":\"nsr.component.v1\"}".to_vec();
        let srv = serve(BTreeMap::from([("/components/textures-1.json".to_string(), body.clone())]), true);
        let o = Origin::new(&srv.base);
        assert_eq!(o.fetch_component("components/textures-1.json").expect("bytes"), body);
        assert_eq!(o.fetch_component("/components/textures-1.json").expect("bytes"), body);
    }
}
