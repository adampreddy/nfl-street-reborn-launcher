# Reborn Publisher, plan 2 of 4: native upload, status, and the journal

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `reborn_core::publish` the three things it still lacks before an app can drive it: it can upload an origin to the bucket itself in a safe order, it can read what the bucket currently serves and diff it against the local origin, and it records every operation in a journal.

**Architecture:** Three new modules under `crates/reborn-core/src/publish/`. `upload.rs` signs S3 requests (AWS SigV4) and sends them through a narrow `Transport` trait, so a test can stand in a loopback server; an `UploadPlan` type makes the safe order (blobs, then component manifests, then release files) a property of the type rather than a convention. `status.rs` reads the local origin and the hosted manifests and returns a typed diff. `journal.rs` appends one JSON line per operation. `rebornctl` gains `publish upload` and `publish status` so all of it is usable and testable before the app exists.

**Tech Stack:** Rust 2021 (workspace rust-version 1.82); `hmac`, `base64` and `percent-encoding` are already in `Cargo.lock` (transitive) so adding them as direct dependencies of `reborn-core` resolves offline; `sha2`, `hex`, `ureq` and `serde_json` are already direct dependencies.

**Spec:** `docs/superpowers/specs/2026-09-08-reborn-publisher-design.md`, sections 3.3 (native upload), 5.1 (status), 6 (bookkeeping), 7 (safety rules), 8 (testing).

**Predecessor:** plan 1 (`docs/superpowers/plans/2026-09-08-publisher-1-core-publish.md`), merged at `0cf3996`. Its ledger is `docs/superpowers/plans/2026-09-08-publisher-1-core-publish-ledger.md` and it carries one deferral this plan must honor (Task 6 below).

## Global Constraints

- Work on a branch in the existing worktree: `cd target\ship-tree && git checkout -B publisher-upload main`. Another agent edits the main tree concurrently; never `git add -A`, add the exact paths each task names. Never use bare `git stash`.
- Everything new lives behind the existing `publish` cargo feature, so a player's launcher links none of it. Run tests with `cargo test -p reborn-core --features publish`.
- **No secret ever reaches a printed line, an `Event`, the journal, or a `Debug` output.** The bucket's access key id and secret, and the site token, are secrets. `RosterRequest` in plan 1 hand-writes `Debug` for exactly this reason; follow that precedent for every new type that holds one.
- **Never write to a real data root or the real bucket in a test.** Tests use `tempfile::tempdir()` and the loopback stand-in. The one real-bucket check is Task 7 and it writes only under a `test/` prefix and deletes what it wrote.
- The upload order is blobs and component manifests first, release files (`release.json` then `release.json.sig`) last, or for the launcher: the installer first, `latest.json` last. This is enforced by the plan type, not by the caller remembering.
- No em dashes anywhere. US English in every string a person reads.
- Commit trailers on every commit:
  `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`
  `Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK`

## File structure

| File | Responsibility |
|---|---|
| `crates/reborn-core/src/publish/upload.rs` (create) | SigV4 signing, the `Transport` trait and its ureq implementation, `UploadPlan`, `upload`, verification. |
| `crates/reborn-core/src/publish/status.rs` (create) | `Status`, `read_status`: the local origin, the hosted release and launcher manifests, and the diff. |
| `crates/reborn-core/src/publish/journal.rs` (create) | `Record`, `append`, `read_all`. |
| `crates/reborn-core/src/publish/mod.rs` (modify) | Declare the three modules; `next_sequence` gains the refusal from plan 1's deferral. |
| `crates/reborn-core/Cargo.toml` (modify) | `hmac`, `base64`, `percent-encoding` as optional dependencies enabled by the `publish` feature. |
| `crates/reborn-cli/src/publish.rs` (modify) | `Upload` and `Status` subcommands. |
| `Tools/fake-s3.py` (create) | The loopback stand-in: checks signatures, records arrival order, serves objects back. |
| `Tools/e2e-upload.py` (create) | End to end: publish an origin, upload it to the stand-in, read status against it, verify. |

## Parallel batches

Tasks 1, 4, 5 and 6 touch disjoint files and may run at the same time. Task 2 depends on Task 1. Task 3 depends on Task 2. Task 7 depends on everything.

- Batch A (parallel): Task 1 (signing), Task 4 (journal), Task 5 (status), Task 6 (`next_sequence` refusal).
- Batch B: Task 2 (transport and plan) after Task 1.
- Batch C: Task 3 (upload and verify) after Task 2.
- Batch D: Task 7 (the CLI surface and the end-to-end script) after all of the above.

---

### Task 1: SigV4 signing

**Files:**
- Create: `crates/reborn-core/src/publish/upload.rs` (signing section and its tests only; Task 2 adds the transport)
- Modify: `crates/reborn-core/Cargo.toml`
- Modify: `crates/reborn-core/src/publish/mod.rs` (add `pub mod upload;`)

**Interfaces produced:**

```rust
/// The bucket's address and credentials. Hand-written Debug: the secret
/// never prints.
pub struct Bucket {
    pub endpoint: String,   // "https://<account>.r2.cloudflarestorage.com"
    pub bucket: String,     // "reborn-content"
    pub region: String,     // "auto" for R2
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// One request to sign: method, the object key, query string, headers the
/// caller set, and the payload's sha256 (or the unsigned-payload marker).
pub struct SignedRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// Sign one request. `now` is passed in so tests are deterministic.
pub fn sign(
    bucket: &Bucket,
    method: &'static str,
    key: &str,
    query: &[(&str, &str)],
    extra_headers: &[(String, String)],
    payload_sha256: &str,
    now: std::time::SystemTime,
) -> SignedRequest;
```

**Behavior:** AWS Signature Version 4, service `s3`. The canonical request URI-encodes the key path segment by segment (`/` stays a separator, everything else percent-encodes, including spaces as `%20` not `+`). Query parameters sort by name. `host`, `x-amz-content-sha256` and `x-amz-date` are always signed; any `extra_headers` are signed too. The signed-headers list is lowercase, sorted, semicolon-joined. The credential scope is `<yyyymmdd>/<region>/s3/aws4_request`.

**Steps:**

- [ ] **Step 1: Branch and dependencies.** `git checkout -B publisher-upload main`. Add to `crates/reborn-core/Cargo.toml`, as optional dependencies enabled by the `publish` feature (matching the existing `getrandom` pattern): `hmac = { version = "0.12", optional = true }`, `base64 = { version = "0.22", optional = true }`, `percent-encoding = { version = "2", optional = true }`. Use whatever versions `Cargo.lock` already resolves so nothing new is downloaded; check with `grep -A2 '^name = "hmac"' Cargo.lock`. Extend the feature: `publish = ["dep:getrandom", "dep:hmac", "dep:base64", "dep:percent-encoding"]`. Confirm `cargo build -p reborn-core --features publish` succeeds and downloads nothing.

- [ ] **Step 2: Write the failing tests.** Use the AWS published SigV4 test suite values. The canonical case, which you must reproduce exactly:

  Given `Bucket { endpoint: "https://examplebucket.s3.amazonaws.com", bucket: "examplebucket", region: "us-east-1", access_key_id: "AKIAIOSFODNN7EXAMPLE", secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY" }`, a `GET` of key `test.txt` with header `range: bytes=0-9`, payload sha256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (the empty-string hash) and a timestamp of 2013-05-24T00:00:00Z, the `Authorization` header must be exactly:

  `AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request,SignedHeaders=host;range;x-amz-content-sha256;x-amz-date,Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41`

  Write additional tests for: a key containing a space (must encode as `%20` in the canonical URI and appear in the signature), a key with a `/` (stays a separator), two query parameters given out of order (must sort by name in the canonical query), and that the secret never appears in `SignedRequest`'s `Debug` output or in any header value other than the derived signature. Also assert `Bucket`'s `Debug` prints the secret as `<set>` and the access key id in full (the id is not a secret; the secret is).

- [ ] **Step 3: Run to see them fail.** `cargo test -p reborn-core --features publish publish::upload` — expect compile errors.

- [ ] **Step 4: Implement.** HMAC-SHA256 via the `hmac` crate over `sha2::Sha256`. The derived signing key is `HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), "s3"), "aws4_request")`. Hex-encode the final signature lowercase. Percent-encode with `percent-encoding`'s `NON_ALPHANUMERIC` set minus `-._~`, applied per path segment.

- [ ] **Step 5: Run the tests.** All green, no warnings.

- [ ] **Step 6: Commit** `crates/reborn-core/src/publish/upload.rs`, `crates/reborn-core/src/publish/mod.rs`, `crates/reborn-core/Cargo.toml`, `Cargo.lock`. Subject: `core publish: sign S3 requests with SigV4`.

---

### Task 2: The transport and the ordered upload plan

**Files:**
- Modify: `crates/reborn-core/src/publish/upload.rs`

**Interfaces produced:**

```rust
/// One HTTP exchange. Narrow on purpose: a test stands in for it.
pub trait Transport: Send + Sync + std::fmt::Debug {
    fn send(&self, req: &SignedRequest, body: Body<'_>) -> Result<Reply>;
}
pub enum Body<'a> { Empty, Bytes(&'a [u8]), File { path: &'a Path, offset: u64, len: u64 } }
pub struct Reply { pub status: u16, pub headers: Vec<(String, String)>, pub body: Vec<u8> }
pub struct UreqTransport { /* ureq::Agent */ }

/// What to upload, in the only order that is safe.
///
/// The three groups are private; a caller adds to a group by name and the
/// plan yields them in order. A caller cannot express "release first".
pub struct UploadPlan { /* groups */ }
impl UploadPlan {
    pub fn new() -> Self;
    pub fn blob(&mut self, key: String, from: PathBuf, bytes: u64, sha256: Sha256Hex);
    pub fn manifest(&mut self, key: String, from: PathBuf, bytes: u64, sha256: Sha256Hex);
    pub fn release_file(&mut self, key: String, from: PathBuf, bytes: u64, sha256: Sha256Hex);
    /// Every item, blobs then manifests then release files, each group in
    /// the order it was added.
    pub fn items(&self) -> impl Iterator<Item = &UploadItem>;
    pub fn total_bytes(&self) -> u64;
    pub fn len(&self) -> usize;
}
pub struct UploadItem { pub key: String, pub from: PathBuf, pub bytes: u64, pub sha256: Sha256Hex, pub group: Group }
pub enum Group { Blob, Manifest, Release }

/// Build the plan for an origin directory: every blob and manifest the
/// bucket does not already hold at the right length, then the release files.
pub fn plan_origin(origin: &Path, already: &dyn Fn(&str) -> Option<u64>) -> Result<UploadPlan>;
```

**Steps:**

- [ ] **Step 1: Write the failing tests.**
  - `items()` yields blobs, then manifests, then release files, whatever order they were added in. Add a release file first, then a blob, and assert the blob comes out first.
  - `plan_origin` over a temp origin holding two blobs, one component manifest, `release.json` and `release.json.sig` produces five items in that order, with the right keys (`blobs/by-hash/<aa>/<hash>`, `components/<name>-<version>.json`, `release.json`, `release.json.sig`; take the blob path shape from `crate::origin::blob_path`).
  - `plan_origin` skips an object the `already` closure reports at the identical length, and does not skip one reported at a different length.
  - A plan whose `already` reports everything present still includes the two release files: they change on every publish and must always be re-uploaded.
- [ ] **Step 2: Run to see them fail.**
- [ ] **Step 3: Implement.** `UreqTransport` sets the signed headers on a `ureq` request and streams a file body from an offset without loading it. Keep the groups as three `Vec<UploadItem>`.
- [ ] **Step 4: Run the tests.**
- [ ] **Step 5: Commit.** Subject: `core publish: an upload plan whose order cannot be got wrong`.

---

### Task 3: Uploading and verifying

**Files:**
- Modify: `crates/reborn-core/src/publish/upload.rs`

**Interfaces produced:**

```rust
pub struct UploadRequest<'a> {
    pub bucket: &'a Bucket,
    pub plan: &'a UploadPlan,
    /// Objects at or above this size go multipart. Default 256 MiB.
    pub multipart_threshold: u64,
    /// Multipart chunk size. Default 100 MiB.
    pub part_size: u64,
    /// Attempts per object or per part before giving up. Default 3.
    pub attempts: u32,
    /// The public host the read-back uses, for example
    /// "https://files.nflstreetreborn.com/". Read-back is skipped when None.
    pub public_base: Option<&'a str>,
    pub apply: bool,
}
pub struct Uploaded { pub objects: u64, pub bytes: u64, pub skipped: u64, pub verified: Vec<String> }
pub fn upload(transport: &dyn Transport, http: &dyn Http, req: &UploadRequest<'_>, sink: Sink<'_>) -> Result<Outcome<Uploaded>>;
```

**Behavior:**
- Below the threshold: one `PUT` with the payload's real sha256 in `x-amz-content-sha256`. Above it: `CreateMultipartUpload`, then a `PUT` per part with `partNumber` and `uploadId` query parameters, then `CompleteMultipartUpload` with the parts' ETags in order. Any failure after the create must send `AbortMultipartUpload` so no parts linger, even on an error return.
- Retry a transport failure or a 5xx up to `attempts` times with a short backoff. Never retry a 4xx other than 429: a signature error would repeat forever.
- After the last object, and only when `public_base` is set, fetch `release.json` (or `latest.json` when the plan has no release files) over the public host with `http` and compare its bytes with the local file. Report a mismatch as a refusal, not an error: the upload happened and the operator needs to know the edge is stale.
- `apply: false` describes every object and its size and uploads nothing (`Outcome::Described`).
- Emit one line per object at the same shape as the rest of the module (`out!`), plus a progress line per multipart part. Never emit a header value.

**Steps:**

- [ ] **Step 1: Write the failing tests** against a `FakeTransport` that records every request in order and can be told to fail the nth call with a given status.
  - A plan of three small objects sends three `PUT`s in the plan's order.
  - An object at twice the part size sends create, two part PUTs, then complete, with the parts numbered 1 and 2 and the complete body naming both ETags in order.
  - A part that fails once with a 500 is retried and the upload succeeds; the recorded calls show four requests for two parts.
  - A 403 is not retried: exactly one attempt, and the result is an error or refusal naming the status.
  - A failure during a multipart upload sends `AbortMultipartUpload` before returning.
  - `apply: false` sends nothing and returns `Described`.
  - Read-back mismatch: a `FakeHttp` serving different bytes for `release.json` makes the outcome `Refused` and the message says the edge is serving something else.
- [ ] **Step 2: Run to see them fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests.**
- [ ] **Step 5: Commit.** Subject: `core publish: upload objects, multipart for the large ones, and read back`.

---

### Task 4: The journal

**Files:**
- Create: `crates/reborn-core/src/publish/journal.rs`
- Modify: `crates/reborn-core/src/publish/mod.rs` (add `pub mod journal;`)

**Interfaces produced:**

```rust
pub const JOURNAL_FILE: &str = "publisher-journal.jsonl";
#[derive(Serialize, Deserialize)]
pub struct Record {
    pub schema: String,          // "nsr.publisher-journal.v1"
    pub at: String,              // RFC 3339 local time, the way the rest of the product stamps
    pub operation: String,       // "roster", "game-patch", "release", "component", "upload", "launcher"
    pub dry_run: bool,
    pub inputs: serde_json::Value,   // paths and versions, never a secret
    pub sequence: Option<u64>,
    pub hashes: Vec<(String, String)>,  // (what, hex)
    pub uploaded: Vec<String>,          // object keys
    pub verified: Option<String>,       // what the read-back said
    pub outcome: String,                // "done" | "described" | "refused"
    pub summary: String,
}
pub fn append(origin: &Path, record: &Record) -> Result<()>;
pub fn read_all(origin: &Path) -> Result<Vec<Record>>;
pub fn read_last(origin: &Path, n: usize) -> Result<Vec<Record>>;
```

**Behavior:** one JSON object per line, appended, created if absent. `read_all` skips a malformed line rather than failing the whole read, and counts what it skipped in a returned tally (a corrupt line must not hide the rest of the history). Never write a value that came from the vault or a key file: the writer is responsible, and a test asserts a record built from a request carrying a token does not contain it.

**Steps:**

- [ ] **Step 1: Write the failing tests.** Append three records and read them back in order. A hand-written malformed line between two good ones is skipped and both good ones still come back. `read_last(2)` returns the last two in order. A record whose `inputs` was built from a struct holding a token does not contain the token's value.
- [ ] **Step 2: Run to see them fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests.**
- [ ] **Step 5: Commit.** Subject: `core publish: a journal, one line per operation`.

---

### Task 5: Status

**Files:**
- Create: `crates/reborn-core/src/publish/status.rs`
- Modify: `crates/reborn-core/src/publish/mod.rs` (add `pub mod status;`)

**Interfaces produced:**

```rust
pub struct Status {
    pub local: Option<LocalOrigin>,
    pub hosted: Option<HostedRelease>,
    pub launcher: Option<HostedLauncher>,
    /// Components published locally that the bucket does not serve, or
    /// serves at a different version.
    pub pending: Vec<Pending>,
    pub problems: Vec<String>,
}
pub struct LocalOrigin { pub version: String, pub sequence: u64, pub pins: Vec<(ComponentId, String)>, pub components_on_disk: Vec<(ComponentId, String, u64)> }
pub struct HostedRelease { pub version: String, pub sequence: u64, pub pins: Vec<(ComponentId, String)>, pub verified: bool }
pub struct HostedLauncher { pub version: String, pub bytes: u64, pub sha256: String, pub file_name: String }
pub struct Pending { pub component: ComponentId, pub local: String, pub hosted: Option<String>, pub reason: PendingReason }
pub enum PendingReason { NotServed, OlderOnBucket, LocalReleaseNewer }
pub fn read_status(http: &dyn Http, origin: &Path, origin_base: &str, launcher_manifest: &str, verifier: &dyn SignatureVerifier) -> Status;
```

**Behavior:** never fails as a whole. A local origin that does not exist, a bucket that cannot be reached, a signature that does not verify: each becomes an entry in `problems` and the rest of the status still fills in. The hosted release is verified with the compiled-in keys and `verified` says whether it opened. `pending` compares the local `release.json`'s pins with the hosted ones and lists what differs.

**Steps:**

- [ ] **Step 1: Write the failing tests** against `FakeHttp` (already in `crate::net`) and temp origins.
  - A local origin at sequence 4 and a bucket at sequence 3 produce a `LocalReleaseNewer` pending entry for each pin that differs.
  - A component on disk that no release pins is reported in `components_on_disk` but not in `pending`.
  - An unreachable bucket produces a `problems` entry naming the URL and leaves `local` fully populated.
  - A hosted release whose signature does not verify sets `verified: false` and adds a problem; the version and sequence still parse and show.
  - No local origin at all: `local` is `None`, one problem, and the hosted side still reads.
- [ ] **Step 2: Run to see them fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests.**
- [ ] **Step 5: Commit.** Subject: `core publish: read what the bucket serves and diff it against the origin`.

---

### Task 6: `next_sequence` refuses an unparseable manifest

**Files:**
- Modify: `crates/reborn-core/src/publish/mod.rs`

This is plan 1's deferred finding, recorded in its ledger. Today `next_sequence` does `if let Ok(c) = Component::parse(&bytes)`, so a truncated or corrupt component manifest counts as absent and the returned sequence can collide with one already published. A component republished at a sequence that is not above the installed one is content a player's launcher considers itself already current on: a publish that reports success and changes nothing.

**Interface change:**

```rust
pub enum SequenceProblem { Unreadable { path: PathBuf, why: String } }
pub fn next_sequence(origin: &Path, component: ComponentId) -> Result<u64>;  // unchanged signature
```

**Behavior:** a file under `components/` whose name matches the component's prefix and which does not parse now returns `Err` naming the file and saying the sequence cannot be trusted until it is fixed or removed. A file that parses but belongs to a different component is still skipped silently, as now. An unreadable directory is still "no manifests, sequence 1".

**Steps:**

- [ ] **Step 1: Write the failing tests.** A corrupt `runtime-1.0.1.json` beside a good `runtime-1.0.0.json` makes `next_sequence(origin, Runtime)` return `Err` whose message names the file. A corrupt `roster-x.json` does not affect `next_sequence(origin, Runtime)`. The existing behavior (one above the highest, 1 when there are none) still holds; keep plan 1's existing test.
- [ ] **Step 2: Run to see them fail** (the corrupt-file test will pass a wrong value today, so assert on the `Err`).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests**, and also `cargo test -p reborn-core --features publish publish::` to prove nothing else depended on the silent skip.
- [ ] **Step 5: Commit.** Subject: `core publish: refuse a sequence computed over a manifest that does not parse`.

---

### Task 7: The CLI surface and the end-to-end proof

**Files:**
- Modify: `crates/reborn-cli/src/publish.rs`
- Create: `Tools/fake-s3.py`
- Create: `Tools/e2e-upload.py`
- Modify: `Tools/test-all.ps1` (run the new script alongside the others)

**The CLI:**

```
rebornctl publish upload --origin <dir> [--endpoint <url>] [--bucket <name>] [--public-base <url>] [--apply]
rebornctl publish status [--origin <dir>] [--origin-base <url>] [--launcher <url>]
```

Credentials come from the vault (`nsr.r2.access`, `nsr.r2.secret`), with `publish token` gaining `--r2-access-from-file` and `--r2-secret-from-file` alongside the existing site token, and printing only "stored" or "missing". `--endpoint` and `--bucket` default to the values a new `publish::vault::bucket(&Platform)` reads from the vault, so no secret is ever typed on a command line.

**`Tools/fake-s3.py`:** a loopback HTTP server that verifies each request's `Authorization` header against a known key using the same canonical-request rules, records the order object keys arrive, stores bodies in memory, supports multipart (create returns an upload id, part PUTs return ETags, complete assembles them), serves objects back for GET and HEAD, and exposes `/__order` returning the arrival order as JSON so the script can assert it.

**`Tools/e2e-upload.py`:** publish a small origin with the real `rebornctl`, upload it to the stand-in with `--apply`, assert from `/__order` that every blob and manifest arrived before `release.json` and that `release.json.sig` arrived last, run `publish status` against the stand-in and assert it reports nothing pending, then publish a second component version, assert status now reports it pending, upload again and assert status is clean. Also assert an interrupted upload (tell the stand-in to fail one part) leaves no multipart upload open.

**Steps:**

- [ ] **Step 1: Write `Tools/fake-s3.py`** and prove it standalone: start it, sign a request with the core's signer through a tiny Rust test or `rebornctl publish upload` against it, and confirm it accepts a correct signature and rejects a tampered one.
- [ ] **Step 2: Add the CLI subcommands.** Follow plan 1's shape exactly: parse, build a request, print events with the existing `print` sink, map the outcome with `exit_of`.
- [ ] **Step 3: Write `Tools/e2e-upload.py`.**
- [ ] **Step 4: Run it.** `py -3 Tools\e2e-upload.py` must pass.
- [ ] **Step 5: Run everything.** `cargo test --workspace`, `py -3 Tools\e2e-update.py`, and `powershell -File .\Tools\test-all.ps1 -NoRust`. All green.
- [ ] **Step 6: One real-bucket check.** With the real credentials in the vault, upload a single small file to `test/publisher-check-<timestamp>.txt`, read it back over the public host, confirm the bytes match, then delete it and confirm it is gone. This is the only test that touches the real bucket. Record the object key and the outcome in the report. If the credentials are not in the vault, say so and skip this step rather than inventing a result.
- [ ] **Step 7: Commit.** Subject: `cli: publish upload and publish status, with a loopback S3 stand-in`.

---

## Self-review

**Spec coverage:** 3.3 native upload (Tasks 1 to 3: SigV4, multipart with retries and abort, enforced order, read-back over the public host); 5.1 status (Task 5, plus the CLI in Task 7); 6 bookkeeping (Task 4 journal, Task 6 sequence); 7 safety (the order is a type property, `apply: false` everywhere, no real data root in tests, one bounded real-bucket check); 8 testing (the loopback stand-in, the AWS vectors, `Tools/e2e-upload.py`). Deferred to plan 3 on purpose: wiring the journal into each verb's call path (the app writes the records, and plan 3 owns the call sites), and the runtime lint of spec 5.6 (it belongs with the Content page).

**Placeholder scan:** none. Every task names its files, its interfaces with exact signatures, and what each test asserts.

**Type consistency:** `Sink<'_>`, `Outcome<T>`, `out!` and `refuse` are plan 1's, unchanged. `Sha256Hex`, `ComponentId`, `Http`, `FakeHttp` and `SignatureVerifier` are existing core types. `Bucket` is produced by Task 1 and consumed by Tasks 3 and 7. `UploadPlan` is produced by Task 2 and consumed by Task 3. `Status` is produced by Task 5 and consumed by Task 7.
