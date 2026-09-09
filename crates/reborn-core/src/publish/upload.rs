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
use crate::origin::{blob_path, BLOB_DIR, RELEASE_FILE, SIGNATURE_FILE};
use crate::publish::{io_err, COMPONENTS_DIR};
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
fn canonical_headers_and_signed(host: &str, amz_date: &str, payload_sha256: &str, extra_headers: &[(String, String)]) -> (String, String) {
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
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
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
#[derive(Debug)]
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
                let sha256 = Sha256Hex::parse(&name)
                    .map_err(|_| invalid(format!("{} is not a sha-256 file name; refusing to plan its upload", file.display())))?;
                let bytes = std::fs::metadata(&file).map_err(|e| io_err(&file, e))?.len();
                let key = blob_path(&sha256);
                blobs.push((key, file, bytes, sha256));
            }
        }
    }
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
            if !file.is_file() || !name.ends_with(".json") {
                continue;
            }
            let contents = std::fs::read(&file).map_err(|e| io_err(&file, e))?;
            let bytes = contents.len() as u64;
            let sha256 = Sha256Hex::from_bytes(&contents);
            let key = format!("{COMPONENTS_DIR}/{name}");
            manifests.push((key, file, bytes, sha256));
        }
    }
    manifests.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, from, bytes, sha256) in manifests {
        if already(&key) != Some(bytes) {
            plan.manifest(key, from, bytes, sha256);
        }
    }

    for name in [RELEASE_FILE, SIGNATURE_FILE] {
        let from = origin.join(name);
        if !from.is_file() {
            continue;
        }
        let contents = std::fs::read(&from).map_err(|e| io_err(&from, e))?;
        let bytes = contents.len() as u64;
        let sha256 = Sha256Hex::from_bytes(&contents);
        plan.release_file(name.to_string(), from, bytes, sha256);
    }

    Ok(plan)
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
}
