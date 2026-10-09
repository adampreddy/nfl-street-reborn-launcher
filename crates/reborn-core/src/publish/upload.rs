//! AWS Signature Version 4 request signing, service `s3`, for the R2 (an
//! S3-compatible object store) upload; the transport that sends a signed
//! request over the wire; and the ordered plan of what an origin directory
//! needs uploaded. A later task adds the actual upload loop (retries,
//! multipart, read-back) on top of this.
//!
//! The reference is the AWS published test suite. `sign` reproduces its
//! canonical "GET Object" example exactly (see the tests below); everything
//! here follows from that one worked case: URI-encode the object key
//! segment by segment, sort the query by name, sign `host`,
//! `x-amz-content-sha256`, `x-amz-date` and whatever the caller adds, and
//! derive the signing key through the four-step HMAC chain the spec
//! describes.
//!
//! # Path-style addressing
//!
//! The AWS vector above is virtual-hosted style: the bucket lives in the
//! hostname (`examplebucket.s3.amazonaws.com`), and the canonical URI is the
//! key alone. R2 uses path-style addressing instead: the bucket is a path
//! segment (`<endpoint>/<bucket>/<key>`). `sign` stays the low-level
//! primitive it was for Task 1 -- it signs whatever path its `key` argument
//! turns into, with no opinion about a bucket -- and [`object_path`] is what
//! a caller building an R2 request passes as that argument, so the exact
//! same string becomes both the canonical URI that gets signed and, through
//! `sign`'s own URL assembly, the request URL's path. A signature computed
//! over a different path than the one the request actually sends fails as a
//! signature mismatch with no indication of what disagreed, so keeping them
//! the same string by construction is the whole reason this exists.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::manifest::Sha256Hex;
use crate::net::Http;
use crate::origin::{blob_path, BLOB_DIR, PRIVATE_BLOB_DIR, RELEASE_FILE, SIGNATURE_FILE};
use crate::publish::journal::JOURNAL_FILE;
use crate::publish::{err, io_err, out, refuse, thousands, Event, Outcome, Sink, COMPONENTS_DIR};
use crate::{Error, Result};

type HmacSha256 = Hmac<Sha256>;

/// The bucket's address and credentials.
///
/// `endpoint` is host-only (scheme + authority, no path), for example
/// `"https://<account>.r2.cloudflarestorage.com"`. [`sign`] builds the
/// request path from the object key alone; nothing here inserts `bucket`
/// into it (a later task's caller decides whether the key it passes already
/// carries a bucket path segment).
pub struct Bucket {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// Hand-written so the secret access key never prints: `<set>` or `<unset>`,
/// never the value. The access key id is not a secret and prints in full,
/// same as every other field. See `publish::roster::RosterRequest` for the
/// same pattern applied to a bearer token.
impl std::fmt::Debug for Bucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bucket")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", if self.secret_access_key.is_empty() { &"<unset>" } else { &"<set>" })
            .finish()
    }
}

/// One request to sign: the method, the full URL (host, path and sorted
/// query already assembled), and the headers to send, including
/// `Authorization`. Holds no secret, so the derived `Debug` is safe to log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// The payload hash to send when the body is not hashed up front, for
/// example a streamed multipart part. SigV4 still requires the header; this
/// is the value that tells S3 the signature does not cover the body.
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// Characters SigV4's URI-encoding step leaves alone: the unreserved set,
/// RFC 3986 section 2.3. Everything else, space included, percent-encodes
/// (space becomes `%20`, never `+`).
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

fn uri_encode(s: &str) -> String {
    utf8_percent_encode(s, UNRESERVED).to_string()
}

/// The request path: `key` encoded segment by segment so `/` stays a
/// separator instead of becoming `%2F`.
///
/// A leading slash on `key` is stripped before splitting, not treated as an
/// empty first segment: [`object_path`] returns a path already rooted at
/// `/`, and passing that straight through as `sign`'s `key` argument must
/// not produce a doubled leading slash.
fn canonical_uri(key: &str) -> String {
    let mut out = String::from("/");
    for (i, segment) in key.trim_start_matches('/').split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&uri_encode(segment));
    }
    out
}

/// The query string used both for signing and for the request URL: every
/// name and value URI-encoded, then sorted by name (ties broken by value,
/// which the spec does not require but never hurts).
fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = query.iter().map(|(k, v)| (uri_encode(k), uri_encode(v))).collect();
    pairs.sort();
    pairs.into_iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")
}

/// The canonical headers block (`name:value\n` per line, lowercase name,
/// sorted) and the matching `SignedHeaders` list. `host`,
/// `x-amz-content-sha256` and `x-amz-date` are always included; whatever
/// the caller passed in `extra_headers` joins them.
///
/// `extra_headers` must never name one of those three (case-insensitive).
/// The headers are sorted and then deduplicated by name below, so a
/// collision would let whichever of the two values happens to sort first
/// silently win, which is whichever the ASCII ordering of the value
/// prefers, not whichever the caller meant to send. Nothing here currently
/// passes a colliding name -- the multipart requests in this module only
/// ever call `sign` with an empty `extra_headers` -- so this is a guard
/// against a future caller getting it wrong, checked with a debug
/// assertion rather than a `Result`: every caller of `sign` in this crate
/// already treats it as infallible, and a header collision here is a
/// programming error to catch in testing, not a runtime condition for a
/// publisher to recover from.
fn canonical_headers_and_signed(host: &str, amz_date: &str, payload_sha256: &str, extra_headers: &[(String, String)]) -> (String, String) {
    for (name, _) in extra_headers {
        debug_assert!(
            !matches!(name.to_ascii_lowercase().as_str(), "host" | "x-amz-content-sha256" | "x-amz-date"),
            "extra header {name:?} collides with a header sign() always sets itself; one of the \
             two values would be silently dropped instead of both being sent"
        );
    }

    let mut headers: Vec<(String, String)> = vec![
        ("host".to_string(), host.to_string()),
        ("x-amz-content-sha256".to_string(), payload_sha256.to_string()),
        ("x-amz-date".to_string(), amz_date.to_string()),
    ];
    for (name, value) in extra_headers {
        headers.push((name.to_lowercase(), value.trim().to_string()));
    }
    headers.sort();
    headers.dedup_by(|a, b| a.0 == b.0);

    let mut canonical = String::new();
    for (name, value) in &headers {
        canonical.push_str(name);
        canonical.push(':');
        canonical.push_str(value);
        canonical.push('\n');
    }
    let signed = headers.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(";");
    (canonical, signed)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    // A key of any length is valid input to HMAC (RFC 2104 pads or hashes it
    // as needed), so `new_from_slice` cannot fail here.
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts a key of any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// `HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), "s3"), "aws4_request")`.
fn signing_key(secret: &str, date_stamp: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    hmac_sha256(&k_service, b"aws4_request")
}

/// `now` as `yyyymmddTHHMMSSZ`, UTC. Hand-rolled instead of pulling in a
/// date/time crate for one timestamp format: seconds since the epoch split
/// into a day count and a time of day, then the day count turned into a
/// civil date with Howard Hinnant's `civil_from_days`
/// (<https://howardhinnant.github.io/date_algorithms.html#civil_from_days>).
fn format_amz_date(now: std::time::SystemTime) -> String {
    let secs = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (year, month, day) = crate::publish::civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// Sign one request. `now` is a parameter, never read from the clock here,
/// so a caller's test is deterministic.
pub fn sign(
    bucket: &Bucket,
    method: &'static str,
    key: &str,
    query: &[(&str, &str)],
    extra_headers: &[(String, String)],
    payload_sha256: &str,
    now: std::time::SystemTime,
) -> SignedRequest {
    let amz_date = format_amz_date(now);
    let date_stamp = amz_date[..8].to_string();

    let host = bucket.endpoint.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string();

    let uri = canonical_uri(key);
    let query_string = canonical_query(query);
    let (canonical_headers, signed_headers) = canonical_headers_and_signed(&host, &amz_date, payload_sha256, extra_headers);

    let canonical_request = format!("{method}\n{uri}\n{query_string}\n{canonical_headers}\n{signed_headers}\n{payload_sha256}");
    let hashed_canonical_request = hex::encode(Sha256::digest(canonical_request.as_bytes()));

    let credential_scope = format!("{date_stamp}/{}/s3/aws4_request", bucket.region);
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{hashed_canonical_request}");

    let key_bytes = signing_key(&bucket.secret_access_key, &date_stamp, &bucket.region);
    let signature = hex::encode(hmac_sha256(&key_bytes, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope},SignedHeaders={signed_headers},Signature={signature}",
        bucket.access_key_id
    );

    let mut headers = vec![
        ("host".to_string(), host),
        ("x-amz-content-sha256".to_string(), payload_sha256.to_string()),
        ("x-amz-date".to_string(), amz_date),
    ];
    headers.extend(extra_headers.iter().cloned());
    headers.push(("Authorization".to_string(), authorization));

    let mut url = format!("{}{}", bucket.endpoint.trim_end_matches('/'), uri);
    if !query_string.is_empty() {
        url.push('?');
        url.push_str(&query_string);
    }

    SignedRequest { method, url, headers }
}

/// The request path R2's path-style addressing uses: `/<bucket>/<key>`. See
/// the module doc comment for why this is safe to pass straight through as
/// `sign`'s `key` argument.
pub fn object_path(bucket: &Bucket, key: &str) -> String {
    format!("/{}/{key}", bucket.bucket)
}

/// A refusal reported as an error because the origin holds something that
/// cannot be planned around: a blob file name that is not a sha-256 digest.
fn invalid(message: impl Into<String>) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()))
}

fn transport_failure(url: &str, e: impl std::fmt::Display) -> Error {
    Error::PlainIo(std::io::Error::new(
        std::io::ErrorKind::NotConnected,
        format!("{url} could not be reached: {e}"),
    ))
}

/// One HTTP exchange. Narrow on purpose: a test stands in for it rather than
/// a real socket, the same shape as [`crate::net::Http`] elsewhere in this
/// crate.
pub trait Transport: Send + Sync + std::fmt::Debug {
    fn send(&self, req: &SignedRequest, body: Body<'_>) -> Result<Reply>;
}

/// A request body. `File` streams from an offset for a bounded length
/// without ever holding the whole file in memory, which is the point of it
/// existing: a multipart upload sends a 100 MB part out of a file that may
/// be gigabytes long.
///
/// `Clone, Copy`: every variant is a reference or a plain number, nothing
/// owned, so retrying a failed send costs nothing to repeat -- `Body` is
/// just handed to [`Transport::send`] again rather than needing to be
/// rebuilt or cloned deeply.
#[derive(Debug, Clone, Copy)]
pub enum Body<'a> {
    Empty,
    Bytes(&'a [u8]),
    File { path: &'a Path, offset: u64, len: u64 },
}

/// What the server sent back, whatever its status. A transport failure (no
/// route, DNS, timeout, refused) is an `Err`; a reply the server actually
/// sent is `Ok` no matter the status code, because the caller decides what a
/// 409 or a 5xx means and whether to retry it. Matches the rule
/// [`crate::net::Http`] already follows for the download side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// The real transport: a blocking `ureq` agent with a timeout on every call.
/// Same reasoning as [`crate::net::UreqHttp`]: a publish that hangs on a
/// dead server is indistinguishable from one that crashed.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl std::fmt::Debug for UreqTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UreqTransport").finish()
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl UreqTransport {
    /// Connect: long enough for a slow link to answer. Overall: long enough
    /// for a multipart part (Task 3 defaults to 100 MiB) to finish over a
    /// publisher's ordinary upload link, which is far short of a datacenter's.
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(300))
            .build();
        Self { agent }
    }

    fn to_reply(resp: ureq::Response) -> Result<Reply> {
        let status = resp.status();
        // Header values, not names, are read while `resp` is still the
        // header-only response; `into_reader` below consumes it for the body.
        let headers = resp
            .headers_names()
            .into_iter()
            .filter_map(|name| resp.header(&name).map(|value| (name.clone(), value.to_string())))
            .collect();
        let mut body = Vec::new();
        resp.into_reader().read_to_end(&mut body).map_err(|e| transport_failure("<response body>", e))?;
        Ok(Reply { status, headers, body })
    }
}

impl Transport for UreqTransport {
    fn send(&self, req: &SignedRequest, body: Body<'_>) -> Result<Reply> {
        let mut request = self.agent.request(req.method, &req.url);
        for (name, value) in &req.headers {
            request = request.set(name, value);
        }

        let outcome = match body {
            Body::Empty => request.call(),
            Body::Bytes(bytes) => request.send_bytes(bytes),
            Body::File { path, offset, len } => {
                let mut file = std::fs::File::open(path).map_err(|e| io_err(path, e))?;
                file.seek(SeekFrom::Start(offset)).map_err(|e| io_err(path, e))?;
                // A fixed Content-Length up front, so `ureq` sends exactly
                // `len` bytes with ordinary framing instead of falling back
                // to chunked transfer encoding for a reader of unknown
                // size, which a plainly-signed S3-compatible PUT does not
                // expect.
                request = request.set("Content-Length", &len.to_string());
                request.send(file.take(len))
            }
        };

        match outcome {
            Ok(resp) => Self::to_reply(resp),
            Err(ureq::Error::Status(_, resp)) => Self::to_reply(resp),
            Err(ureq::Error::Transport(t)) => Err(transport_failure(&req.url, t)),
        }
    }
}

/// One object to upload: where it goes, where it comes from, how big it is,
/// its hash for a future integrity check, and which of the three groups
/// [`UploadPlan::items`] must yield it in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadItem {
    pub key: String,
    pub from: PathBuf,
    pub bytes: u64,
    pub sha256: Sha256Hex,
    pub group: Group,
}

/// Which of the three upload groups an item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Blob,
    Manifest,
    Release,
}

/// What to upload, in the only order that is safe: every blob (immutable
/// content nothing else names until a manifest is live), then every
/// component manifest (named by a release nothing serves yet), then the two
/// release files (the pointer that makes both of the above reachable).
///
/// The three groups are private. A caller adds to a group by name --
/// [`UploadPlan::blob`], [`UploadPlan::manifest`] or
/// [`UploadPlan::release_file`] -- and [`UploadPlan::items`] always yields
/// them in the order above, whatever order they were added in. A caller
/// cannot express "release first", which would publish a pointer to objects
/// the bucket does not have yet.
#[derive(Debug, Default)]
pub struct UploadPlan {
    blobs: Vec<UploadItem>,
    manifests: Vec<UploadItem>,
    release_files: Vec<UploadItem>,
}

impl UploadPlan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn blob(&mut self, key: String, from: PathBuf, bytes: u64, sha256: Sha256Hex) {
        self.blobs.push(UploadItem { key, from, bytes, sha256, group: Group::Blob });
    }

    pub fn manifest(&mut self, key: String, from: PathBuf, bytes: u64, sha256: Sha256Hex) {
        self.manifests.push(UploadItem { key, from, bytes, sha256, group: Group::Manifest });
    }

    pub fn release_file(&mut self, key: String, from: PathBuf, bytes: u64, sha256: Sha256Hex) {
        self.release_files.push(UploadItem { key, from, bytes, sha256, group: Group::Release });
    }

    /// Every item, blobs then manifests then release files, each group in
    /// the order it was added.
    pub fn items(&self) -> impl Iterator<Item = &UploadItem> {
        self.blobs.iter().chain(self.manifests.iter()).chain(self.release_files.iter())
    }

    pub fn total_bytes(&self) -> u64 {
        self.items().map(|item| item.bytes).sum()
    }

    pub fn len(&self) -> usize {
        self.blobs.len() + self.manifests.len() + self.release_files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `dir`'s direct children, as full paths, sorted by name so a plan's order
/// does not depend on the filesystem's own directory-listing order (which on
/// most platforms is arbitrary).
fn sorted_dir_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| io_err(dir, e))?.filter_map(|entry| entry.ok()).map(|entry| entry.path()).collect();
    entries.sort();
    Ok(entries)
}

/// Names that must never enter a plan, no matter which directory a future
/// change makes this function walk. `plan_origin` today only ever reads
/// `blobs/**`, `components/*.json` and the two release files named
/// explicitly below, which already keeps [`JOURNAL_FILE`] (it lives at the
/// origin root, under a name no glob here matches, and holds local paths
/// and a version history) off the bucket -- but only as a side effect of
/// which directories happen to get scanned. This list turns that into an
/// explicit check at each scan site instead, so a later change to widen a
/// scan (say, to pick up a stray file dropped into `components/` by a bad
/// copy) cannot quietly start shipping the journal to a public CDN.
const NEVER_UPLOADED: &[&str] = &[JOURNAL_FILE];

/// Build the plan for an origin directory: every blob and manifest the
/// bucket does not already hold at the right length, then the release
/// files.
///
/// `already(key)` reports the length the bucket already has that key at, or
/// `None` if it does not have the key at all. A length that disagrees with
/// the local file counts as not-yet-uploaded rather than skipped: a
/// mismatch is exactly the case an upload exists to fix, not a reason to
/// leave it alone.
///
/// `release.json` and `release.json.sig` are never checked against
/// `already`: they change on every publish (a new sequence, a new
/// signature) and must always be re-sent, or a publish that only added a
/// blob would leave the public pointer on the previous release.
///
/// Both release files must be present together, or neither. An origin that
/// has never published anything (no blobs, no manifests, and no release
/// files at all) is a legitimate empty plan, not a refusal: nothing has
/// been signed yet because nothing exists yet to sign. But an origin that
/// already holds published content and is missing one or both release
/// files is refused by name: a publish that quietly skipped an absent
/// `release.json` would upload real content and report success while never
/// making it reachable, which is the exact failure this whole plan/upload
/// split exists to rule out.
pub fn plan_origin(origin: &Path, already: &dyn Fn(&str) -> Option<u64>) -> Result<UploadPlan> {
    let mut plan = UploadPlan::new();

    let mut blobs: Vec<(String, PathBuf, u64, Sha256Hex)> = Vec::new();
    let blobs_dir = origin.join(BLOB_DIR);
    if blobs_dir.is_dir() {
        for prefix in sorted_dir_entries(&blobs_dir)? {
            if !prefix.is_dir() {
                continue;
            }
            for file in sorted_dir_entries(&prefix)? {
                if !file.is_file() {
                    continue;
                }
                let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                if NEVER_UPLOADED.contains(&name.as_str()) {
                    continue;
                }
                let sha256 = Sha256Hex::parse(&name)
                    .map_err(|_| invalid(format!("{} is not a sha-256 file name; refusing to plan its upload", file.display())))?;
                let bytes = std::fs::metadata(&file).map_err(|e| io_err(&file, e))?.len();
                let key = blob_path(&sha256);
                blobs.push((key, file, bytes, sha256));
            }
        }
    }
    let any_blob_files = !blobs.is_empty();
    blobs.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, from, bytes, sha256) in blobs {
        if already(&key) != Some(bytes) {
            plan.blob(key, from, bytes, sha256);
        }
    }

    let mut manifests: Vec<(String, PathBuf, u64, Sha256Hex)> = Vec::new();
    let components_dir = origin.join(COMPONENTS_DIR);
    if components_dir.is_dir() {
        for file in sorted_dir_entries(&components_dir)? {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if !file.is_file() || !name.ends_with(".json") || NEVER_UPLOADED.contains(&name.as_str()) {
                continue;
            }
            let contents = std::fs::read(&file).map_err(|e| io_err(&file, e))?;
            let bytes = contents.len() as u64;
            let sha256 = Sha256Hex::from_bytes(&contents);
            let key = format!("{COMPONENTS_DIR}/{name}");
            manifests.push((key, file, bytes, sha256));
        }
    }
    let any_manifest_files = !manifests.is_empty();
    manifests.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, from, bytes, sha256) in manifests {
        if already(&key) != Some(bytes) {
            plan.manifest(key, from, bytes, sha256);
        }
    }

    let release_present = origin.join(RELEASE_FILE).is_file();
    let signature_present = origin.join(SIGNATURE_FILE).is_file();
    let origin_has_published_before = any_blob_files || any_manifest_files || release_present || signature_present;

    match (release_present, signature_present) {
        (true, true) => {
            for name in [RELEASE_FILE, SIGNATURE_FILE] {
                let from = origin.join(name);
                let contents = std::fs::read(&from).map_err(|e| io_err(&from, e))?;
                let bytes = contents.len() as u64;
                let sha256 = Sha256Hex::from_bytes(&contents);
                plan.release_file(name.to_string(), from, bytes, sha256);
            }
        }
        (false, false) if !origin_has_published_before => {
            // A genuinely fresh origin: nothing has ever been published
            // here, so there is nothing to sign yet either. An empty plan,
            // not a refusal.
        }
        (false, false) => {
            return Err(invalid(format!(
                "{RELEASE_FILE} and {SIGNATURE_FILE} are both missing from {}, but this origin \
                 already holds published content (a blob, a manifest, or both). Refusing to plan \
                 an upload with no release to point at any of it: sign a release first.",
                origin.display()
            )));
        }
        (true, false) => {
            return Err(invalid(format!(
                "{SIGNATURE_FILE} is missing from {}, but {RELEASE_FILE} is present there. \
                 Refusing to plan an upload that would publish a release with no signature for \
                 it: sign it, or remove {RELEASE_FILE} if it should not exist yet.",
                origin.display()
            )));
        }
        (false, true) => {
            return Err(invalid(format!(
                "{RELEASE_FILE} is missing from {}, but {SIGNATURE_FILE} is present there. \
                 Refusing to plan an upload that would publish a signature with no release for \
                 it to sign: it cannot be included in a plan when the file it signs is not on \
                 disk.",
                origin.display()
            )));
        }
    }

    Ok(plan)
}

/// The members-only blobs, for the PRIVATE bucket (`reborn-league`): every
/// file under `<origin>/private/blobs/by-hash` the bucket does not already
/// hold at the right length, keyed by the ordinary `blobs/by-hash/<xx>/<sha>`
/// layout. Blobs only. The manifests and release files that name them are
/// public and go up through [`plan_origin`] to the public bucket, after this
/// plan has finished.
pub fn plan_private(origin: &Path, already: &dyn Fn(&str) -> Option<u64>) -> Result<UploadPlan> {
    let mut plan = UploadPlan::new();
    let dir = origin.join(PRIVATE_BLOB_DIR);
    if !dir.is_dir() {
        return Ok(plan);
    }
    let mut blobs: Vec<(String, PathBuf, u64, Sha256Hex)> = Vec::new();
    for prefix in sorted_dir_entries(&dir)? {
        if !prefix.is_dir() {
            continue;
        }
        for file in sorted_dir_entries(&prefix)? {
            if !file.is_file() {
                continue;
            }
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let sha256 = Sha256Hex::parse(&name)
                .map_err(|_| invalid(format!("{} is not a sha-256 file name; refusing to plan its upload", file.display())))?;
            let bytes = std::fs::metadata(&file).map_err(|e| io_err(&file, e))?.len();
            blobs.push((blob_path(&sha256), file, bytes, sha256));
        }
    }
    blobs.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, from, bytes, sha256) in blobs {
        if already(&key) != Some(bytes) {
            plan.blob(key, from, bytes, sha256);
        }
    }
    Ok(plan)
}

// ============================================================================
// Task 3: uploading the plan, and reading back what the edge serves
// ============================================================================

/// Objects at or above this size go multipart. 256 MiB: comfortably above
/// every component manifest and most textures, so only the few genuinely
/// large blobs (game images, big archives) ever pay for the extra round
/// trips a multipart upload costs.
pub const DEFAULT_MULTIPART_THRESHOLD: u64 = 256 * 1024 * 1024;

/// The size of every part but the last in a multipart upload. 100 MiB: small
/// enough that one failed part is a small retry, large enough that a
/// multi-gigabyte object does not need thousands of them.
pub const DEFAULT_PART_SIZE: u64 = 100 * 1024 * 1024;

/// Attempts per object or per part before giving up.
pub const DEFAULT_ATTEMPTS: u32 = 3;

/// sha256 of the empty byte string, the payload hash for a request with no
/// body: `CreateMultipartUpload` and `AbortMultipartUpload`.
/// `CompleteMultipartUpload` has a real body (the XML listing the parts) and
/// signs its own hash instead.
const EMPTY_PAYLOAD_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// One upload run: what to send, in what sizes, with what patience, and
/// whether to actually send it.
#[derive(Debug)]
pub struct UploadRequest<'a> {
    pub bucket: &'a Bucket,
    pub plan: &'a UploadPlan,
    /// Objects at or above this size go multipart. Default
    /// [`DEFAULT_MULTIPART_THRESHOLD`].
    pub multipart_threshold: u64,
    /// Multipart chunk size. Default [`DEFAULT_PART_SIZE`].
    pub part_size: u64,
    /// Attempts per object or per part before giving up. Default
    /// [`DEFAULT_ATTEMPTS`].
    pub attempts: u32,
    /// The public host the read-back uses, for example
    /// "https://files.nflstreetreborn.com/". Read-back is skipped when
    /// `None`.
    pub public_base: Option<&'a str>,
    pub apply: bool,
}

/// What an upload run actually sent, or would send under `apply: false`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Uploaded {
    pub objects: u64,
    pub bytes: u64,
    /// Always 0 today. `plan_origin` already excludes anything the bucket
    /// holds at the right length before an [`UploadPlan`] is built, so every
    /// item that reaches [`upload`] gets sent; this field exists so the
    /// shape here matches a caller (a future status/resume feature) that
    /// builds a plan by hand and may include entries it already knows are
    /// good.
    pub skipped: u64,
    /// One line per successful read-back check, empty when `public_base`
    /// was `None` or the plan had nothing a read-back applies to.
    pub verified: Vec<String>,
}

/// Sign a request against `bucket`'s object `key`, always over
/// [`object_path`], so the exact string that gets hashed into the canonical
/// request is the exact string the request URL is built from.
///
/// Every request this module sends -- the plain PUT and all four multipart
/// requests (`CreateMultipartUpload`, a part `PUT`, `CompleteMultipartUpload`
/// and `AbortMultipartUpload`) -- goes through this one function rather than
/// calling [`sign`] directly. That makes "the signed path and the sent path
/// disagree" a mistake this module cannot make in five different places,
/// only in this one place it is easy to check (see the module's tests for
/// exactly that check against a request carrying query parameters, the
/// shape most likely to be built inconsistently).
fn signed(bucket: &Bucket, method: &'static str, key: &str, query: &[(&str, &str)], payload_sha256: &str, now: std::time::SystemTime) -> SignedRequest {
    let path = object_path(bucket, key);
    sign(bucket, method, &path, query, &[], payload_sha256, now)
}

/// The clock, checked before it is trusted to sign a request.
///
/// [`format_amz_date`] calls `duration_since(UNIX_EPOCH).unwrap_or_default()`,
/// so a clock reading before 1970 would silently sign every request in this
/// run with `19700101T000000Z`. That date does not match what the server's
/// own clock will compute, so the request fails as a signature mismatch --
/// the least diagnosable error this API returns -- with nothing here to say
/// the actual cause was the local clock. Checking it once, here, before any
/// request is built, turns that into a message that names the real problem
/// instead of a mismatched hash.
fn now_for_signing() -> Result<std::time::SystemTime> {
    let now = std::time::SystemTime::now();
    if now.duration_since(std::time::UNIX_EPOCH).is_err() {
        return Err(invalid(
            "the system clock reads a time before 1970-01-01. Every request an upload signs is \
             timestamped from this clock, and a request signed with an impossible date is \
             rejected by the server as a signature mismatch that gives no hint the clock is the \
             problem. Fix the system clock and run the upload again."
                .to_string(),
        ));
    }
    Ok(now)
}

/// A short pause between one attempt and the next, scaled by attempt number
/// so a server under load sees a little more room each time rather than
/// three attempts back to back.
fn backoff(attempt: u32) {
    std::thread::sleep(std::time::Duration::from_millis(20 * u64::from(attempt)));
}

/// Send `req` with `body`, retrying a transport failure or a `429`/`5xx`
/// reply up to `attempts` times with a short [`backoff`] between tries.
///
/// A `4xx` other than `429` is returned on the first attempt and never
/// retried: a signature error, a bad request or a permissions failure would
/// fail exactly the same way every time, so retrying it would only make the
/// operator wait `attempts` times as long to see the one attempt's answer.
fn send_with_retry(transport: &dyn Transport, req: &SignedRequest, body: Body<'_>, attempts: u32, sink: Sink<'_>, what: &str) -> Result<Reply> {
    let attempts = attempts.max(1);
    let mut attempt = 1u32;
    loop {
        match transport.send(req, body) {
            Ok(reply) => {
                let retryable = reply.status == 429 || (500..600).contains(&reply.status);
                if retryable && attempt < attempts {
                    err!(sink, "  {what} answered {} on attempt {attempt} of {attempts}; retrying.", reply.status);
                    backoff(attempt);
                    attempt += 1;
                    continue;
                }
                return Ok(reply);
            }
            Err(e) => {
                if attempt < attempts {
                    err!(sink, "  {what} could not be reached on attempt {attempt} of {attempts} ({e}); retrying.");
                    backoff(attempt);
                    attempt += 1;
                    continue;
                }
                return Err(e);
            }
        }
    }
}

/// What one object's upload came to. Kept separate from [`Outcome`] because
/// this is an internal, per-object result: the caller ([`upload`]) is the
/// one place that turns "one object was refused" into the whole run's
/// [`Outcome::Refused`].
enum PutOutcome {
    Done,
    Refused(Vec<String>),
}

/// A reply outside 2xx, turned into the refusal lines the operator reads.
/// Never a header value, per this module's rule: only the status and the
/// object key.
fn refused_status(bucket: &Bucket, key: &str, reply: &Reply) -> PutOutcome {
    PutOutcome::Refused(vec![format!(
        "{} answered {} while uploading {key} to bucket {}. Nothing more from this plan was \
         uploaded; whatever came before {key} in the plan's order may already be on the bucket. \
         Fix the cause and run the upload again -- it only sends what the bucket does not \
         already have.",
        bucket.endpoint, reply.status, bucket.bucket
    )])
}

/// One PUT, no multipart, for an object under `multipart_threshold`.
///
/// Signs the object's real content hash, already known from the plan as
/// `item.sha256`, rather than [`UNSIGNED_PAYLOAD`] -- and streams the body
/// straight from disk through [`Body::File`], so this never reads the file
/// into memory before sending it.
fn put_single(transport: &dyn Transport, bucket: &Bucket, item: &UploadItem, attempts: u32, sink: Sink<'_>) -> Result<PutOutcome> {
    let now = now_for_signing()?;
    let req = signed(bucket, "PUT", &item.key, &[], item.sha256.as_str(), now);
    let body = Body::File { path: &item.from, offset: 0, len: item.bytes };
    let reply = send_with_retry(transport, &req, body, attempts, sink, &format!("PUT {}", item.key))?;
    Ok(if (200..300).contains(&reply.status) { PutOutcome::Done } else { refused_status(bucket, &item.key, &reply) })
}

/// `(offset, len)` for every part of an object `total_bytes` long, each
/// `part_size` except the last, which takes the remainder. A zero-byte
/// object still gets exactly one (empty) part: `CompleteMultipartUpload`
/// needs at least one part number to name.
fn part_ranges(total_bytes: u64, part_size: u64) -> Vec<(u64, u64)> {
    let part_size = part_size.max(1);
    let mut ranges = Vec::new();
    let mut offset = 0u64;
    while offset < total_bytes {
        let len = part_size.min(total_bytes - offset);
        ranges.push((offset, len));
        offset += len;
    }
    if ranges.is_empty() {
        ranges.push((0, 0));
    }
    ranges
}

/// `<UploadId>...</UploadId>` (or any other single tag) out of an XML reply
/// body. Hand-rolled rather than pulling in an XML crate for the one field
/// this module ever reads out of one, the same reasoning as the hand-rolled
/// date math above: a small, closed job, not a reason to add a dependency.
fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let rest = &xml[start..];
    let end = rest.find(&close)?;
    Some(rest[..end].to_string())
}

/// A header by name, case-insensitively: S3-compatible servers are not
/// consistent about `ETag` vs `Etag`, and [`Transport`] implementations
/// (real or fake) should not have to agree on a casing either.
fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// The minimal escaping a part's `ETag` needs before it goes inside an XML
/// element: S3 ETags are always quoted hex today, so none of these
/// characters can actually appear, but a body built by hand should not
/// assume that stays true forever.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The `CompleteMultipartUpload` request body: every part, in order, with
/// the ETag its `PUT` reply carried.
fn complete_body(parts: &[(u32, String)]) -> String {
    let mut xml = String::from("<CompleteMultipartUpload>");
    for (part_number, etag) in parts {
        xml.push_str(&format!("<Part><PartNumber>{part_number}</PartNumber><ETag>{}</ETag></Part>", xml_escape(etag)));
    }
    xml.push_str("</CompleteMultipartUpload>");
    xml
}

/// `CreateMultipartUpload`, a `PUT` per part, then `CompleteMultipartUpload`,
/// for an object at or above `multipart_threshold`.
///
/// # The abort-on-failure guarantee
///
/// Everything between a successful create and a successful complete --
/// every part's `PUT`, and the complete call itself -- runs inside the
/// closure below rather than inline in this function, and every fallible
/// step in it uses `?` exactly as naturally as it would if it were written
/// inline. The difference is where a `?` sends control: inline, a `?` here
/// would return out of `put_multipart` itself, straight past the abort call
/// beneath the closure, and leave the upload id dangling on the bucket with
/// however many parts had already landed. Inside the closure, the same `?`
/// only returns out of the closure, producing an `Err` as the closure's
/// `Result` -- `put_multipart` itself never returns until after the `match`
/// below has looked at that result and decided whether to abort. So the one
/// rule that keeps the abort from being skipped is structural, not a
/// discipline of remembering to check every early return by hand: nothing
/// between "the upload id exists" and "the match below runs" can leave this
/// function without going through the closure boundary first.
fn put_multipart(transport: &dyn Transport, bucket: &Bucket, item: &UploadItem, part_size: u64, attempts: u32, done_before: u64, total: u64, sink: Sink<'_>) -> Result<PutOutcome> {
    let ranges = part_ranges(item.bytes, part_size);

    let now = now_for_signing()?;
    let create_req = signed(bucket, "POST", &item.key, &[("uploads", "")], EMPTY_PAYLOAD_SHA256, now);
    let create_reply = send_with_retry(transport, &create_req, Body::Empty, attempts, sink, &format!("CreateMultipartUpload {}", item.key))?;
    if !(200..300).contains(&create_reply.status) {
        return Ok(refused_status(bucket, &item.key, &create_reply));
    }
    let create_body = String::from_utf8_lossy(&create_reply.body).to_string();
    let Some(upload_id) = extract_tag(&create_body, "UploadId") else {
        return Ok(PutOutcome::Refused(vec![format!(
            "CreateMultipartUpload for {} on bucket {} answered a 2xx with no <UploadId> in the \
             body. Nothing was uploaded for it.",
            item.key, bucket.bucket
        )]));
    };
    out!(sink, "  {} : multipart upload {upload_id}, {} part(s)", item.key, ranges.len());

    // See the doc comment above: everything fallible between here and the
    // `match` below is captured into `result` rather than propagated with a
    // bare `?`, which is what makes the abort in the `match`'s other arm
    // unconditional rather than one more early-return path to remember.
    let result: Result<PutOutcome> = (|| {
        let mut parts: Vec<(u32, String)> = Vec::with_capacity(ranges.len());
        for (i, (offset, len)) in ranges.iter().enumerate() {
            let part_number = (i + 1) as u32;
            let now = now_for_signing()?;
            let part_number_text = part_number.to_string();
            let query: [(&str, &str); 2] = [("partNumber", &part_number_text), ("uploadId", &upload_id)];
            let req = signed(bucket, "PUT", &item.key, &query, UNSIGNED_PAYLOAD, now);
            let body = Body::File { path: &item.from, offset: *offset, len: *len };
            let reply = send_with_retry(transport, &req, body, attempts, sink, &format!("UploadPart {part_number} of {}", item.key))?;
            if !(200..300).contains(&reply.status) {
                return Ok(refused_status(bucket, &item.key, &reply));
            }
            let Some(etag) = find_header(&reply.headers, "etag") else {
                return Ok(PutOutcome::Refused(vec![format!(
                    "Part {part_number} of {} on bucket {} answered a 2xx with no ETag header. \
                     Nothing more was uploaded for it.",
                    item.key, bucket.bucket
                )]));
            };
            // `done_before` + this object's bytes so far: the bar keeps
            // moving through a large object instead of jumping at its end.
            sink(
                Event::err(format!("    part {part_number} of {} uploaded, {} byte(s)", ranges.len(), thousands(*len)))
                    .with_progress(done_before + offset + len, total, format!("Uploading part {part_number} of {}", ranges.len())),
            );
            parts.push((part_number, etag.to_string()));
        }

        let now = now_for_signing()?;
        let xml = complete_body(&parts);
        let xml_sha256 = hex::encode(Sha256::digest(xml.as_bytes()));
        let req = signed(bucket, "POST", &item.key, &[("uploadId", &upload_id)], &xml_sha256, now);
        let reply = send_with_retry(transport, &req, Body::Bytes(xml.as_bytes()), attempts, sink, &format!("CompleteMultipartUpload {}", item.key))?;
        Ok(if (200..300).contains(&reply.status) { PutOutcome::Done } else { refused_status(bucket, &item.key, &reply) })
    })();

    match &result {
        Ok(PutOutcome::Done) => {}
        _ => {
            // Best-effort: a failure aborting the abort is not something a
            // second failure message would help the operator act on. The
            // failure already in `result` is the one that gets reported;
            // this only tries to stop the bucket from billing and listing
            // parts nobody will ever complete.
            let _ = abort_multipart(transport, bucket, &item.key, &upload_id, attempts, sink);
        }
    }
    result
}

/// `AbortMultipartUpload`. Called from [`put_multipart`] on any failure
/// after a successful create, so no part lingers half-uploaded on the
/// bucket, billed and listed by `ListMultipartUploads`, with nothing ever
/// going on to complete it.
fn abort_multipart(transport: &dyn Transport, bucket: &Bucket, key: &str, upload_id: &str, attempts: u32, sink: Sink<'_>) -> Result<()> {
    let now = now_for_signing()?;
    let req = signed(bucket, "DELETE", key, &[("uploadId", upload_id)], EMPTY_PAYLOAD_SHA256, now);
    let reply = send_with_retry(transport, &req, Body::Empty, attempts, sink, &format!("AbortMultipartUpload {key}"))?;
    if (200..300).contains(&reply.status) {
        err!(sink, "  {key} : aborted the multipart upload; no parts of it are left on the bucket.");
    } else {
        err!(
            sink,
            "  {key} : AbortMultipartUpload answered {}. The upload id may still be listed by \
             ListMultipartUploads; clean it up by hand.",
            reply.status
        );
    }
    Ok(())
}

/// A relative key joined to the public host, without ever producing a
/// doubled or missing `/` regardless of whether `base` ends in one or `key`
/// starts with one.
fn public_url(base: &str, key: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), key.trim_start_matches('/'))
}

/// What the post-upload read-back found.
enum ReadBack {
    /// The plan carried nothing a read-back applies to: no `release.json`
    /// group item and nothing named `latest.json` either. Not every publish
    /// moves a pointer a player's launcher reads, so this is not a failure.
    Skipped,
    /// The public host served exactly the bytes just uploaded. Carries the
    /// one line [`Uploaded::verified`] records.
    Matched(String),
    /// The public host could not be read, answered something other than
    /// 2xx, or served bytes that do not match. Any of the three means the
    /// same thing to an operator: the upload finished, but what players
    /// actually get has not been confirmed to be it.
    Mismatch(Vec<String>),
}

/// After every object in the plan is uploaded, confirm the edge is actually
/// serving what this run just wrote rather than a stale, cached, or
/// otherwise different copy.
///
/// Checks `release.json` when the plan carries one in its release group;
/// otherwise `latest.json`, for a plan that never touches the release
/// pointer (publishing a component's manifest and blobs on their own, or a
/// caller such as the launcher's own update channel that names its pointer
/// file `latest.json` instead). When the plan has neither, there is nothing
/// this check can confirm, and that is not treated as a failure.
fn verify_read_back(http: &dyn Http, public_base: &str, plan: &UploadPlan) -> Result<ReadBack> {
    let check = plan
        .items()
        .find(|i| i.group == Group::Release && i.key == RELEASE_FILE)
        .or_else(|| plan.items().find(|i| i.key == "latest.json"));
    let Some(item) = check else {
        return Ok(ReadBack::Skipped);
    };

    let local = std::fs::read(&item.from).map_err(|e| io_err(&item.from, e))?;
    let url = public_url(public_base, &item.key);

    let reply = match http.get_text(&url) {
        Ok(r) => r,
        Err(e) => {
            return Ok(ReadBack::Mismatch(vec![format!(
                "The upload finished, but {url} could not be read back to confirm it ({e}). The \
                 edge's state is unknown; check it by hand before telling players to update."
            )]));
        }
    };
    if !(200..300).contains(&reply.status) {
        return Ok(ReadBack::Mismatch(vec![format!(
            "The upload finished, but {url} answered {} on the read-back. The edge's state is \
             unknown; check it by hand before telling players to update.",
            reply.status
        )]));
    }
    if reply.body.as_bytes() != local.as_slice() {
        return Ok(ReadBack::Mismatch(vec![format!(
            "The upload finished, but {url} is serving {} byte(s) that do not match the {} \
             byte(s) just uploaded from {}. The edge is serving something else -- a cache, a \
             stale replica, or a race with another publish -- and players would get that, not \
             what this run sent. Check it by hand before telling players to update.",
            reply.body.len(),
            local.len(),
            item.from.display()
        )]));
    }
    Ok(ReadBack::Matched(format!("{} read back from {url} and verified byte for byte.", item.key)))
}

/// Upload everything [`UploadRequest::plan`] names, in its order, and read
/// back what the public edge serves once it is all up.
///
/// Below `multipart_threshold`, one `PUT` per object. At or above it,
/// `CreateMultipartUpload`, a `PUT` per `part_size` chunk, then
/// `CompleteMultipartUpload`; any failure after the create sends
/// `AbortMultipartUpload` (see [`put_multipart`]'s doc comment for how that
/// is guaranteed regardless of which step failed). A transport failure or a
/// `429`/`5xx` is retried up to `attempts` times; any other non-2xx status
/// stops the run at that object and is reported as a refusal naming it, not
/// as an [`Error`] -- the bucket answered, plainly, and the operator needs
/// the plain answer, not this crate's generic I/O error shape.
///
/// `apply: false` describes every object and its size and sends nothing.
pub fn upload(transport: &dyn Transport, http: &dyn Http, req: &UploadRequest<'_>, sink: Sink<'_>) -> Result<Outcome<Uploaded>> {
    let plan = req.plan;
    out!(sink, "Bucket        {}", req.bucket.bucket);
    out!(sink, "Objects       {} ({} byte(s))", thousands(plan.len() as u64), thousands(plan.total_bytes()));
    out!(sink);

    if plan.is_empty() {
        out!(sink, "Nothing to upload.");
        return Ok(if req.apply { Outcome::Done(Uploaded::default()) } else { Outcome::Described });
    }

    if !req.apply {
        for item in plan.items() {
            out!(sink, "  {}  ({} byte(s))", item.key, thousands(item.bytes));
        }
        out!(sink);
        out!(
            sink,
            "Nothing was uploaded. Re-run with --apply to upload {} object(s), {} byte(s).",
            thousands(plan.len() as u64),
            thousands(plan.total_bytes())
        );
        return Ok(Outcome::Described);
    }

    let total = plan.total_bytes();
    let mut uploaded_objects = 0u64;
    let mut uploaded_bytes = 0u64;
    // The bar starts at zero before the first byte moves, so a window shows
    // "0 of N" at once instead of nothing until the first object lands.
    sink(Event::out(format!("Uploading     {} byte(s) in {} object(s)", thousands(total), thousands(plan.len() as u64))).with_progress(0, total, "Uploading"));
    for item in plan.items() {
        let outcome = if item.bytes < req.multipart_threshold {
            put_single(transport, req.bucket, item, req.attempts, sink)?
        } else {
            put_multipart(transport, req.bucket, item, req.part_size, req.attempts, uploaded_bytes, total, sink)?
        };
        match outcome {
            PutOutcome::Done => {
                uploaded_objects += 1;
                uploaded_bytes += item.bytes;
                sink(
                    Event::out(format!("  {}  ({} byte(s))", item.key, thousands(item.bytes)))
                        .with_progress(uploaded_bytes, total, "Uploading"),
                );
            }
            PutOutcome::Refused(lines) => return Ok(refuse(sink, &lines)),
        }
    }

    let mut verified = Vec::new();
    if let Some(base) = req.public_base {
        match verify_read_back(http, base, plan)? {
            ReadBack::Skipped => {}
            ReadBack::Matched(line) => {
                out!(sink, "{line}");
                verified.push(line);
            }
            ReadBack::Mismatch(lines) => return Ok(refuse(sink, &lines)),
        }
    }

    out!(sink);
    out!(sink, "Uploaded {} object(s), {} byte(s).", thousands(uploaded_objects), thousands(uploaded_bytes));

    Ok(Outcome::Done(Uploaded { objects: uploaded_objects, bytes: uploaded_bytes, skipped: 0, verified }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The AWS test suite's bucket and credentials, used by every test
    /// below unless a test needs its own.
    fn test_bucket() -> Bucket {
        Bucket {
            endpoint: "https://examplebucket.s3.amazonaws.com".to_string(),
            bucket: "examplebucket".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
        }
    }

    /// 2013-05-24T00:00:00Z, the AWS test suite's fixed timestamp.
    fn fixed_now() -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_369_353_600)
    }

    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn header<'a>(signed: &'a SignedRequest, name: &str) -> Option<&'a str> {
        signed.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// The AWS published SigV4 test suite's "GET Object" example, byte for
    /// byte. This is the proof the algorithm is right: everything else
    /// below is a property test around it.
    #[test]
    fn aws_published_vector_get_object_with_range() {
        let bucket = test_bucket();
        let extra = vec![("range".to_string(), "bytes=0-9".to_string())];

        let signed = sign(&bucket, "GET", "test.txt", &[], &extra, EMPTY_SHA256, fixed_now());

        assert_eq!(signed.url, "https://examplebucket.s3.amazonaws.com/test.txt");
        assert_eq!(
            header(&signed, "Authorization"),
            Some(
                "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request,\
                 SignedHeaders=host;range;x-amz-content-sha256;x-amz-date,\
                 Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
            )
        );
    }

    #[test]
    fn key_with_a_space_encodes_as_percent_20_not_plus() {
        let signed = sign(&test_bucket(), "GET", "a b.txt", &[], &[], UNSIGNED_PAYLOAD, fixed_now());

        assert_eq!(signed.url, "https://examplebucket.s3.amazonaws.com/a%20b.txt");
        assert!(!signed.url.contains('+'));

        // The encoded form is what actually got signed, not a display-only
        // detail: changing the key changes the signature.
        let unspaced = sign(&test_bucket(), "GET", "a_b.txt", &[], &[], UNSIGNED_PAYLOAD, fixed_now());
        assert_ne!(header(&signed, "Authorization"), header(&unspaced, "Authorization"));
    }

    #[test]
    fn key_with_a_slash_keeps_it_as_a_path_separator() {
        let signed = sign(&test_bucket(), "GET", "a/b.txt", &[], &[], UNSIGNED_PAYLOAD, fixed_now());

        assert_eq!(signed.url, "https://examplebucket.s3.amazonaws.com/a/b.txt");
        assert!(!signed.url.contains("%2F") && !signed.url.contains("%2f"));
    }

    #[test]
    fn query_parameters_sort_by_name_no_matter_the_call_order() {
        let bucket = test_bucket();
        let forward = sign(&bucket, "GET", "test.txt", &[("uploadId", "abc"), ("partNumber", "2")], &[], UNSIGNED_PAYLOAD, fixed_now());
        let reversed = sign(&bucket, "GET", "test.txt", &[("partNumber", "2"), ("uploadId", "abc")], &[], UNSIGNED_PAYLOAD, fixed_now());

        assert_eq!(forward.url, reversed.url);
        assert!(forward.url.ends_with("?partNumber=2&uploadId=abc"));
        // Same signature too: sorting happened before signing, not after.
        assert_eq!(header(&forward, "Authorization"), header(&reversed, "Authorization"));
    }

    #[test]
    fn secret_never_appears_in_signed_request_or_its_headers() {
        let bucket = test_bucket();
        let extra = vec![("range".to_string(), "bytes=0-9".to_string())];
        let signed = sign(&bucket, "GET", "test.txt", &[], &extra, EMPTY_SHA256, fixed_now());

        let debug = format!("{signed:?}");
        assert!(!debug.contains(bucket.secret_access_key.as_str()));
        for (_, value) in &signed.headers {
            assert!(!value.contains(bucket.secret_access_key.as_str()));
        }
    }

    #[test]
    fn bucket_debug_hides_the_secret_but_shows_the_access_key_id() {
        let bucket = test_bucket();
        let debug = format!("{bucket:?}");

        assert!(debug.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(debug.contains("<set>"));
        assert!(!debug.contains(bucket.secret_access_key.as_str()));
    }

    #[test]
    fn bucket_debug_shows_unset_for_an_empty_secret() {
        let bucket = Bucket { secret_access_key: String::new(), ..test_bucket() };
        let debug = format!("{bucket:?}");

        assert!(debug.contains("<unset>"));
        assert!(!debug.contains("<set>"));
    }

    // --- Task 2: object_path (the bucket segment) --------------------------

    #[test]
    fn object_path_prefixes_the_bucket_and_the_signed_path_matches_the_request_url() {
        let bucket = test_bucket();
        let key = "blobs/by-hash/de/ad beef.bin"; // a subdirectory and a space

        let path = object_path(&bucket, key);
        assert_eq!(path, "/examplebucket/blobs/by-hash/de/ad beef.bin");

        let signed = sign(&bucket, "PUT", &path, &[], &[], UNSIGNED_PAYLOAD, fixed_now());

        // The request URL is exactly the endpoint plus the canonical URI
        // that got signed (sign() builds both from the one `uri` local): no
        // doubled slash from object_path's own leading slash colliding with
        // canonical_uri's, the bucket segment lives in the path rather than
        // the host (unlike the AWS vector above), the subdirectory stays a
        // separator, and the space became %20.
        assert_eq!(signed.url, "https://examplebucket.s3.amazonaws.com/examplebucket/blobs/by-hash/de/ad%20beef.bin");
        // Exactly one "//", the scheme's -- proof nothing doubled up.
        assert_eq!(signed.url.matches("//").count(), 1);
    }

    // --- Task 2: format_amz_date (hand-rolled, no date library) ------------

    #[test]
    fn amz_date_is_correct_across_leap_years_and_year_boundaries() {
        // A bug in the hand-rolled civil-date math would produce a wrong
        // stamp only on some dates, and every upload made on such a date
        // would fail with a signature error nobody could diagnose from the
        // outside. This sweeps the cases most likely to expose exactly that
        // kind of mistake.
        let cases: &[(u64, &str)] = &[
            (0, "19700101T000000Z"),            // the epoch itself
            (1_709_209_845, "20240229T123045Z"), // 2024-02-29, a leap day
            (1_709_251_200, "20240301T000000Z"), // the day after a leap day
            (1_677_628_799, "20230228T235959Z"), // last day of a non-leap February
            (1_704_067_199, "20231231T235959Z"), // one second before a year boundary
            (1_704_067_200, "20240101T000000Z"), // one second after it
            (4_148_269_687, "21010615T090807Z"), // a date past 2100
        ];

        for &(secs, expected) in cases {
            let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
            let signed = sign(&test_bucket(), "GET", "test.txt", &[], &[], UNSIGNED_PAYLOAD, now);
            assert_eq!(header(&signed, "x-amz-date"), Some(expected), "for {secs} seconds since the epoch");
        }
    }

    // --- Task 2: UreqTransport ----------------------------------------------

    #[test]
    fn ureq_transport_sends_signed_headers_and_streams_only_the_requested_slice_of_a_file() {
        use std::io::Write;
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        type Captured = (String, Vec<(String, String)>, Vec<u8>);

        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("addr");
        let captured: Arc<Mutex<Option<Captured>>> = Arc::new(Mutex::new(None));
        let store = Arc::clone(&captured);
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("one connection");

            let mut header_bytes = Vec::new();
            let mut byte = [0u8; 1];
            while !header_bytes.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).expect("read a header byte");
                header_bytes.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&header_bytes).to_string();
            let mut lines = head.lines();
            let request_line = lines.next().unwrap_or_default().to_string();

            let mut headers = Vec::new();
            let mut content_length = 0usize;
            for line in lines {
                if let Some((name, value)) = line.split_once(':') {
                    let (name, value) = (name.trim().to_string(), value.trim().to_string());
                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.parse().unwrap_or(0);
                    }
                    headers.push((name, value));
                }
            }

            let mut body = vec![0u8; content_length];
            if content_length > 0 {
                stream.read_exact(&mut body).expect("read the body");
            }
            *store.lock().expect("lock") = Some((request_line, headers, body));

            let response_body = b"ok";
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response_body.len());
            let _ = stream.write_all(response_body);
        });

        // Recognizable content at every byte position, so a wrong offset or
        // length shows up as a mismatched slice rather than coincidentally
        // matching.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blob.bin");
        let content: Vec<u8> = (0u8..=255).collect();
        std::fs::write(&path, &content).unwrap();

        let signed = SignedRequest {
            method: "PUT",
            url: format!("http://{addr}/blobs/by-hash/de/adbeef"),
            headers: vec![
                ("x-amz-date".to_string(), "20260909T000000Z".to_string()),
                ("Authorization".to_string(), "AWS4-HMAC-SHA256 test".to_string()),
            ],
        };

        let transport = UreqTransport::new();
        let reply = transport.send(&signed, Body::File { path: &path, offset: 10, len: 20 }).expect("the fake server answers");

        server.join().expect("server thread");
        let (request_line, headers, body) = captured.lock().expect("lock").take().expect("one request captured");

        assert!(request_line.starts_with("PUT /blobs/by-hash/de/adbeef"), "request line was {request_line:?}");
        assert_eq!(body, content[10..30], "the body must be exactly the 20 bytes starting at offset 10");
        assert!(headers.iter().any(|(n, v)| n.eq_ignore_ascii_case("x-amz-date") && v == "20260909T000000Z"));
        assert!(headers.iter().any(|(n, v)| n.eq_ignore_ascii_case("Authorization") && v == "AWS4-HMAC-SHA256 test"));
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, b"ok");
    }

    // --- Task 2: UploadPlan ordering ----------------------------------------

    #[test]
    fn items_yields_blobs_then_manifests_then_release_files_regardless_of_add_order() {
        let mut plan = UploadPlan::new();
        // Added out of order: release file, then blob. items() must still
        // put the blob first.
        plan.release_file("release.json".to_string(), PathBuf::from("r"), 10, Sha256Hex::from_bytes(b"release"));
        plan.blob("blobs/by-hash/aa/aaaa".to_string(), PathBuf::from("b"), 5, Sha256Hex::from_bytes(b"blob"));
        plan.manifest("components/runtime-1.0.0.json".to_string(), PathBuf::from("m"), 7, Sha256Hex::from_bytes(b"manifest"));

        let keys: Vec<&str> = plan.items().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, vec!["blobs/by-hash/aa/aaaa", "components/runtime-1.0.0.json", "release.json"]);

        let groups: Vec<Group> = plan.items().map(|i| i.group).collect();
        assert_eq!(groups, vec![Group::Blob, Group::Manifest, Group::Release]);

        assert_eq!(plan.len(), 3);
        assert_eq!(plan.total_bytes(), 22);
        assert!(!plan.is_empty());
    }

    #[test]
    fn empty_plan_is_empty() {
        let plan = UploadPlan::new();
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
        assert_eq!(plan.total_bytes(), 0);
        assert_eq!(plan.items().count(), 0);
    }

    // --- Task 2: plan_origin -------------------------------------------------

    /// Writes `content` under `origin` at the path `blob_path` gives its
    /// hash, creating the fan-out directory as needed. Returns the key
    /// (relative to `origin`) and the hash.
    fn write_blob(origin: &Path, content: &[u8]) -> (String, Sha256Hex) {
        let sha256 = Sha256Hex::from_bytes(content);
        let key = blob_path(&sha256);
        let full = origin.join(&key);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, content).unwrap();
        (key, sha256)
    }

    fn write_manifest(origin: &Path, name: &str, content: &[u8]) -> String {
        let dir = origin.join(COMPONENTS_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), content).unwrap();
        format!("{COMPONENTS_DIR}/{name}")
    }

    #[test]
    fn plan_origin_orders_blobs_then_manifest_then_release_files_with_the_right_keys() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();

        let (key_a, _) = write_blob(origin, b"alpha blob content");
        let (key_b, _) = write_blob(origin, b"beta blob content, a different length");
        let manifest_key = write_manifest(origin, "runtime-1.0.0.json", b"{\"schema\":\"nsr.component.v1\"}");
        std::fs::write(origin.join(RELEASE_FILE), b"release bytes").unwrap();
        std::fs::write(origin.join(SIGNATURE_FILE), b"deadbeef").unwrap();

        let plan = plan_origin(origin, &|_| None).unwrap();
        assert_eq!(plan.len(), 5);

        let mut expected_blob_keys = [key_a, key_b];
        expected_blob_keys.sort();

        let items: Vec<&UploadItem> = plan.items().collect();
        assert_eq!(items[0].key, expected_blob_keys[0]);
        assert_eq!(items[0].group, Group::Blob);
        assert_eq!(items[1].key, expected_blob_keys[1]);
        assert_eq!(items[1].group, Group::Blob);
        assert_eq!(items[2].key, manifest_key);
        assert_eq!(items[2].group, Group::Manifest);
        assert_eq!(items[3].key, RELEASE_FILE);
        assert_eq!(items[3].group, Group::Release);
        assert_eq!(items[4].key, SIGNATURE_FILE);
        assert_eq!(items[4].group, Group::Release);
    }

    #[test]
    fn plan_origin_skips_an_object_already_at_the_same_length_but_not_at_a_different_one() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let (blob_key, _) = write_blob(origin, b"twelve bytes");
        let manifest_key = write_manifest(origin, "runtime-1.0.0.json", b"{}");
        std::fs::write(origin.join(RELEASE_FILE), b"r").unwrap();
        std::fs::write(origin.join(SIGNATURE_FILE), b"s").unwrap();

        let blob_len = std::fs::metadata(origin.join(&blob_key)).unwrap().len();

        // The bucket already has the blob at exactly its real length: it is
        // skipped. Nothing is reported for the manifest, so it is not.
        let matching_key = blob_key.clone();
        let plan = plan_origin(origin, &move |key| if key == matching_key { Some(blob_len) } else { None }).unwrap();
        assert!(!plan.items().any(|i| i.key == blob_key), "an object at the identical length must be skipped");
        assert!(plan.items().any(|i| i.key == manifest_key), "an object `already` never mentioned must still be planned");

        // The bucket has the blob at a DIFFERENT length: it is not skipped.
        let mismatched_key = blob_key.clone();
        let plan = plan_origin(origin, &move |key| if key == mismatched_key { Some(blob_len + 1) } else { None }).unwrap();
        assert!(plan.items().any(|i| i.key == blob_key), "an object at the wrong length must still be planned");
    }

    #[test]
    fn plan_origin_always_includes_both_release_files_even_when_already_reports_everything_present() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let (blob_key, _) = write_blob(origin, b"content");
        let manifest_key = write_manifest(origin, "runtime-1.0.0.json", b"{}");
        std::fs::write(origin.join(RELEASE_FILE), b"release").unwrap();
        std::fs::write(origin.join(SIGNATURE_FILE), b"sig").unwrap();

        // `already` reports every non-release key present at exactly its
        // real length, so the blob and the manifest are both skipped.
        let origin_owned = origin.to_path_buf();
        let blob_key_owned = blob_key.clone();
        let manifest_key_owned = manifest_key.clone();
        let plan = plan_origin(origin, &move |key| {
            let path = if key == blob_key_owned {
                Some(origin_owned.join(&blob_key_owned))
            } else if key == manifest_key_owned {
                Some(origin_owned.join(&manifest_key_owned))
            } else {
                // release.json and release.json.sig: reported present too,
                // to prove the plan includes them anyway.
                Some(origin_owned.join(key))
            };
            path.and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len())
        })
        .unwrap();

        assert_eq!(plan.len(), 2, "the release files, and only the release files, survive");
        let keys: Vec<&str> = plan.items().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, vec![RELEASE_FILE, SIGNATURE_FILE]);
    }

    #[test]
    fn plan_origin_over_an_empty_directory_is_an_empty_plan() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_origin(dir.path(), &|_| None).unwrap();
        assert!(plan.is_empty());
    }

    // --- Review item 2: an absent release file, when the origin already has
    // published content, is a refusal naming the missing file -------------

    #[test]
    fn plan_origin_refuses_when_the_signature_is_missing_but_the_release_is_present() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        write_blob(origin, b"some content already published");
        std::fs::write(origin.join(RELEASE_FILE), b"release bytes").unwrap();
        // release.json.sig deliberately absent.

        let err = plan_origin(origin, &|_| None).unwrap_err().to_string();
        assert!(err.contains(SIGNATURE_FILE), "must name the missing file: {err}");
        assert!(err.contains("Refusing"), "{err}");
    }

    #[test]
    fn plan_origin_refuses_when_the_release_is_missing_but_the_signature_is_present() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        write_blob(origin, b"some content already published");
        std::fs::write(origin.join(SIGNATURE_FILE), b"sig bytes").unwrap();
        // release.json deliberately absent.

        let err = plan_origin(origin, &|_| None).unwrap_err().to_string();
        assert!(err.contains(RELEASE_FILE), "must name the missing file: {err}");
    }

    #[test]
    fn plan_origin_refuses_when_both_release_files_are_missing_but_the_origin_already_published_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        write_manifest(origin, "runtime-1.0.0.json", b"{}");
        // Neither release.json nor release.json.sig exists: an upload that
        // silently uploaded just the manifest would succeed while nothing
        // new is reachable, and nobody would be told.

        let err = plan_origin(origin, &|_| None).unwrap_err().to_string();
        assert!(err.contains(RELEASE_FILE) && err.contains(SIGNATURE_FILE), "{err}");
    }

    #[test]
    fn plan_origin_over_a_genuinely_fresh_origin_with_neither_release_file_is_still_an_empty_plan() {
        // Distinguishes "nothing has ever been published here" (fine, an
        // empty plan) from "something was published but the release
        // pointer vanished" (a refusal, proven by the three tests above).
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_origin(dir.path(), &|_| None).unwrap();
        assert!(plan.is_empty());
    }

    // --- Review item 3: the publisher journal never enters a plan ---------

    #[test]
    fn plan_origin_never_includes_the_publisher_journal_sitting_in_the_origin() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let (blob_key, _) = write_blob(origin, b"real content");
        let manifest_key = write_manifest(origin, "runtime-1.0.0.json", b"{}");
        std::fs::write(origin.join(RELEASE_FILE), b"release").unwrap();
        std::fs::write(origin.join(SIGNATURE_FILE), b"sig").unwrap();
        // The real journal, written the way the publisher actually writes
        // it, sitting at the origin root right beside everything else.
        crate::publish::journal::append(
            origin,
            &crate::publish::journal::Record {
                schema: crate::publish::journal::JOURNAL_SCHEMA.to_string(),
                at: crate::publish::journal::now(),
                operation: "component".to_string(),
                dry_run: false,
                inputs: serde_json::json!({}),
                sequence: None,
                hashes: vec![],
                uploaded: vec![],
                verified: None,
                outcome: "done".to_string(),
                summary: "test".to_string(),
            },
        )
        .unwrap();
        assert!(origin.join(JOURNAL_FILE).is_file(), "the journal must actually exist for this test to mean anything");

        let plan = plan_origin(origin, &|_| None).unwrap();

        assert_eq!(plan.len(), 4, "the blob, the manifest, and the two release files -- nothing else");
        assert!(plan.items().all(|i| i.key != JOURNAL_FILE));
        assert!(plan.items().any(|i| i.key == blob_key));
        assert!(plan.items().any(|i| i.key == manifest_key));
    }

    #[test]
    fn plan_origin_skips_a_journal_named_file_even_if_one_turns_up_inside_blobs() {
        // The extension/hash checks on a blob file already reject anything
        // that is not a bare sha-256 hex name, so a journal file dropped
        // into `blobs/by-hash/` would otherwise be refused as "not a
        // sha-256 file name" rather than silently uploaded -- safe, but a
        // confusing error for what is really a misplaced file. The
        // NEVER_UPLOADED guard is what turns that into a clean skip instead,
        // which this test is what actually exercises: without the guard,
        // plan_origin would return an Err here, not an Ok that omits it.
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let prefix_dir = origin.join(BLOB_DIR).join("pj");
        std::fs::create_dir_all(&prefix_dir).unwrap();
        std::fs::write(prefix_dir.join(JOURNAL_FILE), b"{\"schema\":\"nsr.publisher-journal.v1\"}\n").unwrap();
        std::fs::write(origin.join(RELEASE_FILE), b"release").unwrap();
        std::fs::write(origin.join(SIGNATURE_FILE), b"sig").unwrap();

        let plan = plan_origin(origin, &|_| None).unwrap();

        assert_eq!(plan.len(), 2, "only the two release files; the misplaced journal file was skipped, not refused");
        assert!(plan.items().all(|i| i.key != JOURNAL_FILE));
    }

    // --- Task 3, review item 1: every signed request's canonical path must
    // match the request URL it is actually sent to, including one with
    // query parameters -------------------------------------------------

    #[test]
    fn multipart_part_request_signs_over_the_same_path_the_request_url_carries() {
        let bucket = test_bucket();
        let key = "components/runtime-1.0.0.json";
        let path = object_path(&bucket, key);
        let query: [(&str, &str); 2] = [("partNumber", "1"), ("uploadId", "abc-123")];

        let req = signed(&bucket, "PUT", key, &query, UNSIGNED_PAYLOAD, fixed_now());

        // The URL's path is exactly the endpoint plus the string sign()
        // both hashed into the canonical request and built the URL from --
        // one local value inside sign(), never two -- so this request
        // shape (the one that also carries query parameters) cannot sign
        // over a different path than it is sent to.
        let expected_prefix = format!("{}{}", bucket.endpoint, path);
        assert!(req.url.starts_with(&expected_prefix), "{}", req.url);
        assert!(req.url.ends_with("?partNumber=1&uploadId=abc-123"), "{}", req.url);

        // And the signature really did cover that path: changing the key
        // (and so the path) changes the Authorization header.
        let other = signed(&bucket, "PUT", "components/other.json", &query, UNSIGNED_PAYLOAD, fixed_now());
        assert_ne!(header(&req, "Authorization"), header(&other, "Authorization"));
    }

    // --- Review item 6: canonical_headers_and_signed refuses to silently
    // let an extra header collide with one sign() always sets --------------

    #[test]
    #[should_panic(expected = "collides")]
    fn canonical_headers_and_signed_rejects_an_extra_header_colliding_with_a_reserved_one() {
        let extra = vec![("X-Amz-Date".to_string(), "bogus".to_string())];
        let _ = canonical_headers_and_signed("host", "20260101T000000Z", UNSIGNED_PAYLOAD, &extra);
    }

    // ========================================================================
    // Task 3: upload()
    // ========================================================================

    use crate::net::fake::FakeHttp;
    use crate::publish::Event;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// What one call to [`FakeTransport::send`] carried, recorded in the
    /// order calls arrived.
    #[derive(Debug, Clone)]
    struct RecordedCall {
        method: &'static str,
        url: String,
        body: RecordedBody,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum RecordedBody {
        Empty,
        Bytes(Vec<u8>),
        File { path: PathBuf, offset: u64, len: u64 },
    }

    /// Records every request in order and can be told to answer the nth call
    /// (1-based) with a given status instead of its ordinary plausible
    /// reply. A transport failure (as opposed to a reply carrying a bad
    /// status) is never exercised here: [`UreqTransport`]'s own test above
    /// already covers turning a real socket error into `Err`, and this fake
    /// exists to drive the retry/multipart/abort logic around whatever
    /// [`Transport::send`] returns, not to re-prove that distinction.
    #[derive(Debug, Default)]
    struct FakeTransport {
        calls: Mutex<Vec<RecordedCall>>,
        fail_at: Mutex<HashMap<usize, u16>>,
        upload_id: String,
    }

    impl FakeTransport {
        fn new() -> Self {
            Self { upload_id: "test-upload-id".to_string(), ..Default::default() }
        }

        /// The nth call (1-based, across every request this transport sees)
        /// answers with `status` instead of its ordinary reply.
        fn fail_call(&self, n: usize, status: u16) {
            self.fail_at.lock().unwrap().insert(n, status);
        }

        fn calls(&self) -> Vec<RecordedCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    fn query_param(url: &str, name: &str) -> Option<String> {
        let query = url.split_once('?')?.1;
        query.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == name).then(|| v.to_string())
        })
    }

    impl Transport for FakeTransport {
        fn send(&self, req: &SignedRequest, body: Body<'_>) -> Result<Reply> {
            let recorded_body = match body {
                Body::Empty => RecordedBody::Empty,
                Body::Bytes(b) => RecordedBody::Bytes(b.to_vec()),
                Body::File { path, offset, len } => RecordedBody::File { path: path.to_path_buf(), offset, len },
            };
            let n = {
                let mut calls = self.calls.lock().unwrap();
                calls.push(RecordedCall { method: req.method, url: req.url.clone(), body: recorded_body });
                calls.len()
            };

            if let Some(&status) = self.fail_at.lock().unwrap().get(&n) {
                return Ok(Reply { status, headers: vec![], body: format!("fake failure {status}").into_bytes() });
            }

            if req.method == "POST" && req.url.contains("uploads=") {
                let xml = format!("<InitiateMultipartUploadResult><UploadId>{}</UploadId></InitiateMultipartUploadResult>", self.upload_id);
                return Ok(Reply { status: 200, headers: vec![], body: xml.into_bytes() });
            }
            if req.method == "PUT" && query_param(&req.url, "partNumber").is_some() {
                let part_number = query_param(&req.url, "partNumber").unwrap_or_default();
                return Ok(Reply {
                    status: 200,
                    headers: vec![("ETag".to_string(), format!("\"etag-{part_number}\""))],
                    body: vec![],
                });
            }
            if req.method == "POST" && query_param(&req.url, "uploadId").is_some() {
                return Ok(Reply { status: 200, headers: vec![], body: b"<CompleteMultipartUploadResult/>".to_vec() });
            }
            if req.method == "DELETE" {
                return Ok(Reply { status: 204, headers: vec![], body: vec![] });
            }
            // A plain, single-part PUT.
            Ok(Reply { status: 200, headers: vec![], body: vec![] })
        }
    }

    fn test_upload_bucket() -> Bucket {
        Bucket {
            endpoint: "https://bucket.example.r2.cloudflarestorage.com".to_string(),
            bucket: "nsr-origin".to_string(),
            region: "auto".to_string(),
            access_key_id: "AKID".to_string(),
            secret_access_key: "secret".to_string(),
        }
    }

    fn small_plan(origin: &Path, contents: &[(&str, &[u8])]) -> UploadPlan {
        let mut plan = UploadPlan::new();
        for (key, bytes) in contents {
            let from = origin.join(key.replace('/', "_"));
            std::fs::write(&from, bytes).unwrap();
            plan.blob(key.to_string(), from, bytes.len() as u64, Sha256Hex::from_bytes(bytes));
        }
        plan
    }

    fn default_request<'a>(bucket: &'a Bucket, plan: &'a UploadPlan) -> UploadRequest<'a> {
        UploadRequest {
            bucket,
            plan,
            multipart_threshold: DEFAULT_MULTIPART_THRESHOLD,
            part_size: DEFAULT_PART_SIZE,
            attempts: DEFAULT_ATTEMPTS,
            public_base: None,
            apply: true,
        }
    }

    fn drain(sink_lines: &[Event]) -> String {
        sink_lines.iter().map(|e| e.line.as_str()).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn a_plan_of_three_small_objects_sends_three_puts_in_the_plans_order() {
        let dir = tempfile::tempdir().unwrap();
        let plan = small_plan(dir.path(), &[("blobs/by-hash/aa/aaaa", b"one"), ("blobs/by-hash/bb/bbbb", b"two"), ("blobs/by-hash/cc/cccc", b"three")]);
        let bucket = test_upload_bucket();
        let req = default_request(&bucket, &plan);
        let transport = FakeTransport::new();
        let http = FakeHttp::default();
        let mut lines = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| lines.push(e)).unwrap();

        assert!(outcome.is_done(), "{outcome:?}");
        let calls = transport.calls();
        assert_eq!(calls.len(), 3, "{calls:?}");
        for (call, (key, _)) in calls.iter().zip(plan.items().map(|i| (i.key.clone(), ()))) {
            assert_eq!(call.method, "PUT");
            assert!(call.url.ends_with(&key), "{} does not end with {key}", call.url);
        }
    }

    #[test]
    fn an_object_at_twice_the_part_size_sends_create_two_parts_then_complete_naming_both_etags_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let part_size = 10u64;
        let bytes = vec![7u8; (part_size * 2) as usize];
        let from = dir.path().join("big.bin");
        std::fs::write(&from, &bytes).unwrap();
        let mut plan = UploadPlan::new();
        plan.blob("blobs/by-hash/aa/big".to_string(), from, bytes.len() as u64, Sha256Hex::from_bytes(&bytes));

        let bucket = test_upload_bucket();
        let req = UploadRequest { multipart_threshold: 1, part_size, ..default_request(&bucket, &plan) };
        let transport = FakeTransport::new();
        let http = FakeHttp::default();
        let mut lines = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| lines.push(e)).unwrap();
        assert!(outcome.is_done(), "{outcome:?}");

        let calls = transport.calls();
        assert_eq!(calls.len(), 4, "create, two parts, complete: {calls:?}");
        assert_eq!(calls[0].method, "POST");
        assert!(calls[0].url.contains("uploads="), "{}", calls[0].url);
        assert_eq!(calls[1].method, "PUT");
        assert_eq!(query_param(&calls[1].url, "partNumber").as_deref(), Some("1"));
        assert_eq!(calls[1].body, RecordedBody::File { path: plan.items().next().unwrap().from.clone(), offset: 0, len: part_size });
        assert_eq!(calls[2].method, "PUT");
        assert_eq!(query_param(&calls[2].url, "partNumber").as_deref(), Some("2"));
        assert_eq!(calls[2].body, RecordedBody::File { path: plan.items().next().unwrap().from.clone(), offset: part_size, len: part_size });
        assert_eq!(calls[3].method, "POST");
        assert_eq!(query_param(&calls[3].url, "uploadId").as_deref(), Some("test-upload-id"));
        let RecordedBody::Bytes(complete_body_bytes) = &calls[3].body else { panic!("complete carries a body: {:?}", calls[3].body) };
        let complete_xml = String::from_utf8(complete_body_bytes.clone()).unwrap();
        let etag1_pos = complete_xml.find("etag-1").expect("names part 1's etag");
        let etag2_pos = complete_xml.find("etag-2").expect("names part 2's etag");
        assert!(etag1_pos < etag2_pos, "parts named in order: {complete_xml}");
    }

    #[test]
    fn a_part_that_fails_once_with_500_is_retried_and_only_that_part_is_resent() {
        let dir = tempfile::tempdir().unwrap();
        let part_size = 10u64;
        let bytes = vec![7u8; (part_size * 2) as usize];
        let from = dir.path().join("big.bin");
        std::fs::write(&from, &bytes).unwrap();
        let mut plan = UploadPlan::new();
        plan.blob("blobs/by-hash/aa/big".to_string(), from, bytes.len() as u64, Sha256Hex::from_bytes(&bytes));

        let bucket = test_upload_bucket();
        let req = UploadRequest { multipart_threshold: 1, part_size, ..default_request(&bucket, &plan) };
        let transport = FakeTransport::new();
        // Call 2 is part 1's only attempt in the happy path (call 1 is
        // create); fail it once with a 500 so it is retried.
        transport.fail_call(2, 500);
        let http = FakeHttp::default();
        let mut lines = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| lines.push(e)).unwrap();
        assert!(outcome.is_done(), "{outcome:?} -- {}", drain(&lines));

        let calls = transport.calls();
        // create, part1 (fails), part1 (retried, succeeds), part2, complete.
        assert_eq!(calls.len(), 5, "{calls:?}");
        assert_eq!(calls[0].method, "POST");
        assert_eq!(calls[1].url, calls[2].url, "the SAME part is retried, not a different one");
        assert_eq!(query_param(&calls[1].url, "partNumber").as_deref(), Some("1"));
        assert_eq!(query_param(&calls[2].url, "partNumber").as_deref(), Some("1"));
        assert_eq!(query_param(&calls[3].url, "partNumber").as_deref(), Some("2"), "part 2 was sent exactly once, never redone");
        assert_eq!(calls[4].method, "POST");
    }

    #[test]
    fn a_403_is_not_retried_and_is_reported_naming_the_status() {
        let dir = tempfile::tempdir().unwrap();
        let plan = small_plan(dir.path(), &[("blobs/by-hash/aa/aaaa", b"one")]);
        let bucket = test_upload_bucket();
        let req = default_request(&bucket, &plan);
        let transport = FakeTransport::new();
        transport.fail_call(1, 403);
        let http = FakeHttp::default();
        let mut lines = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| lines.push(e)).unwrap();

        assert_eq!(transport.calls().len(), 1, "a 4xx other than 429 must never be retried");
        match outcome {
            Outcome::Refused(r) => assert!(r.summary.contains("403"), "{}", r.summary),
            other => panic!("expected a refusal naming 403, got {other:?}"),
        }
    }

    #[test]
    fn a_failure_during_a_multipart_upload_sends_abort_before_returning() {
        let dir = tempfile::tempdir().unwrap();
        let part_size = 10u64;
        let bytes = vec![7u8; (part_size * 2) as usize];
        let from = dir.path().join("big.bin");
        std::fs::write(&from, &bytes).unwrap();
        let mut plan = UploadPlan::new();
        plan.blob("blobs/by-hash/aa/big".to_string(), from, bytes.len() as u64, Sha256Hex::from_bytes(&bytes));

        let bucket = test_upload_bucket();
        let req = UploadRequest { multipart_threshold: 1, part_size, attempts: 3, ..default_request(&bucket, &plan) };
        let transport = FakeTransport::new();
        // Part 1 (call 2) fails on every attempt (2, 3, 4 for 3 attempts),
        // exhausting retries.
        transport.fail_call(2, 500);
        transport.fail_call(3, 500);
        transport.fail_call(4, 500);
        let http = FakeHttp::default();
        let mut lines = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| lines.push(e)).unwrap();

        assert!(matches!(outcome, Outcome::Refused(_)), "{outcome:?}");
        let calls = transport.calls();
        let last = calls.last().expect("at least the create and the failed attempts");
        assert_eq!(last.method, "DELETE", "the last call must be AbortMultipartUpload: {calls:?}");
        assert_eq!(query_param(&last.url, "uploadId").as_deref(), Some("test-upload-id"));
        assert!(!calls.iter().any(|c| c.method == "POST" && query_param(&c.url, "uploadId").is_some()), "CompleteMultipartUpload must never be sent: {calls:?}");
    }

    #[test]
    fn progress_rides_on_the_lines_from_zero_to_the_total_through_every_part() {
        let dir = tempfile::tempdir().unwrap();
        let part_size = 10u64;
        let big = vec![7u8; (part_size * 2) as usize];
        let big_from = dir.path().join("big.bin");
        std::fs::write(&big_from, &big).unwrap();
        let small_from = dir.path().join("small.bin");
        std::fs::write(&small_from, b"one").unwrap();
        let mut plan = UploadPlan::new();
        plan.blob("blobs/by-hash/aa/big".to_string(), big_from, big.len() as u64, Sha256Hex::from_bytes(&big));
        plan.blob("blobs/by-hash/bb/small".to_string(), small_from, 3, Sha256Hex::from_bytes(b"one"));
        let total = plan.total_bytes();
        assert_eq!(total, 23);

        let bucket = test_upload_bucket();
        let req = UploadRequest { multipart_threshold: 5, part_size, attempts: 1, ..default_request(&bucket, &plan) };
        let transport = FakeTransport::new();
        let http = FakeHttp::default();
        let mut events = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| events.push(e)).unwrap();

        assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
        let progress: Vec<(u64, u64, String)> =
            events.iter().filter_map(|e| e.progress.as_ref().map(|p| (p.done, p.total, p.label.clone()))).collect();
        assert_eq!(progress.first().map(|p| (p.0, p.1)), Some((0, total)), "the bar starts at zero: {progress:?}");
        assert!(progress.iter().all(|p| p.1 == total), "every report carries the same total: {progress:?}");
        assert!(progress.contains(&(10, total, "Uploading part 1 of 2".to_string())), "{progress:?}");
        assert!(progress.contains(&(20, total, "Uploading part 2 of 2".to_string())), "{progress:?}");
        assert_eq!(progress.last().map(|p| p.0), Some(total), "the last report is the whole total: {progress:?}");
        assert!(progress.windows(2).all(|w| w[0].0 <= w[1].0), "done never goes backwards: {progress:?}");
        // Every progress report is also an ordinary line a console prints.
        assert!(events.iter().filter(|e| e.progress.is_some()).all(|e| !e.line.is_empty()));
    }

    #[test]
    fn apply_false_sends_nothing_and_describes_every_object() {
        let dir = tempfile::tempdir().unwrap();
        let plan = small_plan(dir.path(), &[("blobs/by-hash/aa/aaaa", b"one"), ("blobs/by-hash/bb/bbbb", b"two")]);
        let bucket = test_upload_bucket();
        let req = UploadRequest { apply: false, ..default_request(&bucket, &plan) };
        let transport = FakeTransport::new();
        let http = FakeHttp::default();
        let mut lines = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| lines.push(e)).unwrap();

        assert!(matches!(outcome, Outcome::Described), "{outcome:?}");
        assert_eq!(transport.calls().len(), 0, "apply: false must send nothing");
        let text = drain(&lines);
        assert!(text.contains("blobs/by-hash/aa/aaaa"), "{text}");
        assert!(text.contains("blobs/by-hash/bb/bbbb"), "{text}");
    }

    #[test]
    fn read_back_mismatch_is_a_refusal_naming_the_edge_serving_something_else() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let release_bytes = b"{\"schema\":\"nsr.release.v1\"}";
        let release_from = origin.join(RELEASE_FILE);
        std::fs::write(&release_from, release_bytes).unwrap();
        let mut plan = UploadPlan::new();
        plan.release_file(RELEASE_FILE.to_string(), release_from.clone(), release_bytes.len() as u64, Sha256Hex::from_bytes(release_bytes));

        let bucket = test_upload_bucket();
        let public_base = "https://files.example.test/";
        let req = UploadRequest { public_base: Some(public_base), ..default_request(&bucket, &plan) };
        let transport = FakeTransport::new();
        // The edge serves DIFFERENT bytes than what was just uploaded.
        let http = FakeHttp::default().reply(&public_url(public_base, RELEASE_FILE), 200, b"{\"schema\":\"stale\"}");
        let mut lines = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| lines.push(e)).unwrap();

        match outcome {
            Outcome::Refused(r) => assert!(r.summary.contains("serving something else"), "{}", r.summary),
            other => panic!("expected a refusal, got {other:?}"),
        }
        // The upload itself still happened: this is the whole point of a
        // refusal rather than pretending the objects were never sent.
        assert_eq!(transport.calls().len(), 1);
    }

    #[test]
    fn read_back_match_is_recorded_as_verified_and_the_run_is_done() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let release_bytes = b"{\"schema\":\"nsr.release.v1\"}";
        let release_from = origin.join(RELEASE_FILE);
        std::fs::write(&release_from, release_bytes).unwrap();
        let mut plan = UploadPlan::new();
        plan.release_file(RELEASE_FILE.to_string(), release_from.clone(), release_bytes.len() as u64, Sha256Hex::from_bytes(release_bytes));

        let bucket = test_upload_bucket();
        let public_base = "https://files.example.test/";
        let req = UploadRequest { public_base: Some(public_base), ..default_request(&bucket, &plan) };
        let transport = FakeTransport::new();
        let http = FakeHttp::default().reply(&public_url(public_base, RELEASE_FILE), 200, release_bytes.as_slice());
        let mut lines = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| lines.push(e)).unwrap();

        match outcome {
            Outcome::Done(uploaded) => {
                assert_eq!(uploaded.objects, 1);
                assert_eq!(uploaded.bytes, release_bytes.len() as u64);
                assert_eq!(uploaded.skipped, 0);
                assert_eq!(uploaded.verified.len(), 1);
                assert!(uploaded.verified[0].contains(RELEASE_FILE));
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_plan_uploads_nothing_and_is_done_with_zero_objects() {
        let plan = UploadPlan::new();
        let bucket = test_upload_bucket();
        let req = default_request(&bucket, &plan);
        let transport = FakeTransport::new();
        let http = FakeHttp::default();
        let mut lines = Vec::new();

        let outcome = upload(&transport, &http, &req, &mut |e| lines.push(e)).unwrap();

        assert_eq!(transport.calls().len(), 0);
        match outcome {
            Outcome::Done(uploaded) => assert_eq!(uploaded, Uploaded::default()),
            other => panic!("expected Done with nothing uploaded, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// What the bucket already holds, in one pass.
//
// Every planner used to ask HEAD once per object to learn what was already
// on the bucket. The origin holds sixteen thousand blobs, so a publish spent
// ten minutes asking before it sent a byte, with nothing on screen: the
// Publisher looked hung, twice on 2026-09-10. ListObjectsV2 answers a
// thousand keys a page, so the same question is seventeen requests.

/// Every key on the bucket under `prefix`, with its size. The inner `Err`
/// says why the bucket would not list (the HTTP status and what it said:
/// a token scoped to objects alone, a stand-in without the call), so the
/// caller can fall back to asking per key and say why it is slow.
pub fn list_keys(transport: &dyn Transport, bucket: &Bucket, prefix: &str) -> Result<std::result::Result<std::collections::HashMap<String, u64>, String>> {
    let mut found = std::collections::HashMap::new();
    let mut token: Option<String> = None;
    loop {
        let mut query: Vec<(&str, &str)> = vec![("list-type", "2"), ("max-keys", "1000"), ("prefix", prefix)];
        if let Some(t) = token.as_deref() {
            query.push(("continuation-token", t));
        }
        let path = format!("/{}/", bucket.bucket);
        let req = sign(bucket, "GET", &path, &query, &[], EMPTY_PAYLOAD_SHA256, std::time::SystemTime::now());
        // One request per thousand keys, asked for again on a 429, a 5xx or
        // a dropped connection before the listing is given up on, the way
        // every upload request is. A listing that fails after the component
        // was written would otherwise fail the whole publish for one reset.
        let reply = {
            let mut attempt = 1u32;
            loop {
                match transport.send(&req, Body::Empty) {
                    Ok(r) if (r.status == 429 || (500..600).contains(&r.status)) && attempt < 3 => {
                        backoff(attempt);
                        attempt += 1;
                    }
                    Ok(r) => break r,
                    Err(_) if attempt < 3 => {
                        backoff(attempt);
                        attempt += 1;
                    }
                    Err(e) => return Err(e),
                }
            }
        };
        if !(200..300).contains(&reply.status) {
            let said: String = String::from_utf8_lossy(&reply.body).chars().filter(|c| !c.is_control()).take(160).collect();
            return Ok(Err(format!("HTTP {} {}", reply.status, said.trim())));
        }
        let xml = String::from_utf8_lossy(&reply.body);
        for (key, size) in listed_objects(&xml) {
            found.insert(key, size);
        }
        match continuation_token(&xml) {
            Some(t) => token = Some(t),
            None => return Ok(Ok(found)),
        }
    }
}

/// The size of one object on the bucket, by HEAD, or `None` when it is not
/// there. The slow way; [`already_on_bucket`] uses it only as a fallback.
pub fn head_length(transport: &dyn Transport, bucket: &Bucket, key: &str) -> Option<u64> {
    let path = object_path(bucket, key);
    let req = sign(bucket, "HEAD", &path, &[], &[], EMPTY_PAYLOAD_SHA256, std::time::SystemTime::now());
    let reply = transport.send(&req, Body::Empty).ok()?;
    if !(200..300).contains(&reply.status) {
        return None;
    }
    reply
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<u64>().ok())
}

/// "Is this key on the bucket, and how long is it?" for the planners: one
/// listing up front when the bucket allows it, a HEAD per key when it does
/// not. The note says which, because the difference is ten minutes.
#[allow(clippy::type_complexity)]
pub fn already_on_bucket<'a>(
    transport: &'a dyn Transport,
    bucket: &'a Bucket,
) -> Result<(Box<dyn Fn(&str) -> Option<u64> + 'a>, String)> {
    match list_keys(transport, bucket, "")? {
        Ok(found) => {
            let note = format!("Listed        {} object(s) already on the bucket, in one pass.", thousands(found.len() as u64));
            Ok((Box::new(move |key: &str| found.get(key).copied()), note))
        }
        Err(why) => Ok((
            Box::new(move |key: &str| head_length(transport, bucket, key)),
            format!("The bucket would not list its objects ({why}); asking about each one (slow)."),
        )),
    }
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&amp;", "&")
}

fn xml_text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    Some(&block[start..end])
}

/// `(key, size)` for every `<Contents>` entry of a ListObjectsV2 page.
fn listed_objects(xml: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<Contents>") {
        let after = &rest[start + "<Contents>".len()..];
        let Some(end) = after.find("</Contents>") else { break };
        let block = &after[..end];
        if let (Some(key), Some(size)) = (xml_text(block, "Key"), xml_text(block, "Size").and_then(|s| s.trim().parse::<u64>().ok())) {
            out.push((xml_unescape(key), size));
        }
        rest = &after[end + "</Contents>".len()..];
    }
    out
}

/// The token for the next page, when the page says there is one.
fn continuation_token(xml: &str) -> Option<String> {
    if xml_text(xml, "IsTruncated")?.trim() != "true" {
        return None;
    }
    xml_text(xml, "NextContinuationToken").map(xml_unescape)
}

#[cfg(test)]
mod listing_tests {
    use super::*;
    use std::sync::Mutex;

    /// A transport that answers each call from a script and records the URLs.
    #[derive(Debug)]
    struct Scripted {
        replies: Mutex<Vec<Reply>>,
        urls: Mutex<Vec<String>>,
    }
    impl Transport for Scripted {
        fn send(&self, req: &SignedRequest, _body: Body<'_>) -> Result<Reply> {
            self.urls.lock().unwrap().push(req.url.clone());
            Ok(self.replies.lock().unwrap().remove(0))
        }
    }
    fn bucket() -> Bucket {
        Bucket {
            endpoint: "https://acct.r2.cloudflarestorage.com".into(),
            bucket: "reborn-content".into(),
            region: "auto".into(),
            access_key_id: "AKIA".into(),
            secret_access_key: "secret".into(),
        }
    }
    fn page(entries: &[(&str, u64)], next: Option<&str>) -> Reply {
        let mut xml = String::from("<?xml version=\"1.0\"?><ListBucketResult><Name>reborn-content</Name>");
        for (k, n) in entries {
            xml.push_str(&format!("<Contents><Key>{k}</Key><LastModified>x</LastModified><ETag>e</ETag><Size>{n}</Size></Contents>"));
        }
        match next {
            Some(t) => xml.push_str(&format!("<IsTruncated>true</IsTruncated><NextContinuationToken>{t}</NextContinuationToken>")),
            None => xml.push_str("<IsTruncated>false</IsTruncated>"),
        }
        xml.push_str("</ListBucketResult>");
        Reply { status: 200, headers: vec![], body: xml.into_bytes() }
    }

    #[test]
    fn a_listing_walks_every_page_and_reports_each_key_with_its_size() {
        let t = Scripted {
            replies: Mutex::new(vec![page(&[("blobs/by-hash/ab/abcd", 10), ("components/runtime-1.0.0.json", 20)], Some("tok&en=1")), page(&[("release.json", 30)], None)]),
            urls: Mutex::new(vec![]),
        };
        let found = list_keys(&t, &bucket(), "").unwrap().expect("the bucket listed");
        assert_eq!(found.len(), 3);
        assert_eq!(found.get("blobs/by-hash/ab/abcd"), Some(&10));
        assert_eq!(found.get("release.json"), Some(&30));
        let urls = t.urls.lock().unwrap();
        assert_eq!(urls.len(), 2, "one request per page");
        assert!(urls[0].contains("/reborn-content/?") && urls[0].contains("list-type=2"), "{}", urls[0]);
        assert!(urls[1].contains("continuation-token=tok%26en%3D1"), "the token travels back encoded: {}", urls[1]);
    }

    #[test]
    fn a_bucket_that_refuses_to_list_yields_none_so_the_caller_falls_back() {
        let t = Scripted { replies: Mutex::new(vec![Reply { status: 403, headers: vec![], body: b"AccessDenied".to_vec() }]), urls: Mutex::new(vec![]) };
        assert!(list_keys(&t, &bucket(), "").unwrap().is_err());
    }

    #[test]
    fn keys_with_xml_escapes_come_back_as_written() {
        let objects = listed_objects("<Contents><Key>a &amp; b</Key><Size>1</Size></Contents>");
        assert_eq!(objects, vec![("a & b".to_string(), 1)]);
        assert_eq!(continuation_token("<IsTruncated>false</IsTruncated>"), None);
    }

    #[test]
    fn already_on_bucket_answers_from_the_listing_and_says_so() {
        let t = Scripted { replies: Mutex::new(vec![page(&[("release.json", 30)], None)]), urls: Mutex::new(vec![]) };
        let b = bucket();
        let (already, note) = already_on_bucket(&t, &b).unwrap();
        assert!(note.starts_with("Listed        1 object(s)"), "{note}");
        assert_eq!(already("release.json"), Some(30));
        assert_eq!(already("missing"), None);
        assert_eq!(t.urls.lock().unwrap().len(), 1, "no HEAD followed the listing");
    }

    fn write_blob(dir: &std::path::Path, bytes: &[u8]) -> Sha256Hex {
        let sha = Sha256Hex::from_bytes(bytes);
        let p = dir.join(&sha.as_str()[..2]).join(sha.as_str());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
        sha
    }

    #[test]
    fn the_public_plan_never_contains_a_private_blob() {
        let t = tempfile::tempdir().unwrap();
        write_blob(&t.path().join(crate::origin::PRIVATE_BLOB_DIR), b"league only");
        let plan = plan_origin(t.path(), &|_| None).unwrap();
        assert!(plan.is_empty(), "{:?}", plan.items().map(|i| &i.key).collect::<Vec<_>>());
    }

    #[test]
    fn the_private_plan_holds_only_private_blobs_under_the_public_key_layout() {
        let t = tempfile::tempdir().unwrap();
        let public = write_blob(&t.path().join(BLOB_DIR), b"retail");
        let private = write_blob(&t.path().join(crate::origin::PRIVATE_BLOB_DIR), b"league only");
        let plan = plan_private(t.path(), &|_| None).unwrap();
        let keys: Vec<&str> = plan.items().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, vec![blob_path(&private).as_str()]);
        assert!(!keys.contains(&blob_path(&public).as_str()));
        assert!(plan.items().all(|i| i.group == Group::Blob));
        let already_there = plan_private(t.path(), &|k| (k == blob_path(&private)).then_some(11)).unwrap();
        assert!(already_there.is_empty(), "a blob the bucket holds at the right length is skipped");
    }
}
