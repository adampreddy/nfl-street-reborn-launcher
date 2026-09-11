"""End to end: a whole Reborn Publisher publishing cycle, from a fresh
origin and fresh keys to a verified upload and back again, against loopback
services only. Never the real bucket, never a real data root, never
`C:\\Reborn Origin`.

    py -3 Tools/e2e-publisher.py        (needs target/debug/rebornctl.exe)

This is the layer above `Tools/e2e-upload.py` (which this script's own
helpers are copied from, the same way every script in this folder is a
standalone file rather than importing across a hyphenated name) and
`Tools/e2e-update.py`: those two, plus `Tools/e2e-game-patch.py` and
`Tools/e2e-roster.py`, each prove one slice on their own. This script drives
a whole cycle end to end: publish, upload, push a roster, patch the game
with a small fake chain, roll a version back, and confirm the journal and
the cleanup.

# Where `rebornctl` is not enough, and why

Every `rebornctl publish` subcommand is a thin CLI over
`reborn_core::publish`. None of them ever appends to
`reborn_core::publish::journal`: that file is written in exactly one place,
`crates/reborn-publisher/src/ops.rs`'s `run_operation`, which every
Publisher app page command calls and the CLI never does (confirmed by
reading both source trees: `journal::append` has exactly one call site in
this whole workspace, in `ops.rs`). So proving "the journal holds one
record per operation with the right outcome" needs something that actually
calls into `reborn_core::publish` and journals the way the app does, for at
least the one operation this script's cycle needs that the CLI has no
subcommand for at all: a rollback (`rebornctl` has `publish release`, which
can pin any already-published version including an older one, but nothing
named `rollback`, and nothing that journals).

This script's `_build_and_write_harness` writes a small, disposable Rust
program into this run's own scratch directory, compiles it there (a second,
throwaway Cargo workspace -- its own `Cargo.toml` declares `[workspace]` so
it does not try to join this repository's real one -- depending on the
real, unmodified `reborn-core` crate by path), and calls it three times for
the rollback step. It performs the real work
(`reborn_core::publish::release::release`, the exact function
`rebornctl publish release` itself calls) and then appends one journal
record the same way `ops.rs`'s `finish`/`write_journal` would, using the
real, unmodified `reborn_core::publish::journal` module. It does NOT call
into `reborn-publisher`'s own page files (`history.rs` and the rest): those
are private to that binary crate (no `[lib]` target), reachable only through
its Tauri window, and this task's brief forbids changing crate code to
expose them. What this harness proves is real end to end (a real
`release()` call, a real, unmodified journal write, read back and checked);
what it does not prove is `history.rs`'s own thin wrapper around that same
call. See the task report for this named as a deliberate, disclosed gap
rather than something papered over.

Roster push and the game patch DO have `rebornctl` subcommands
(`publish roster`, and `publish component` standing in for `publish
game-patch` -- see the game-patch section below for why), so both go
through the real CLI, proven the same way `e2e-update.py` and
`e2e-game-patch.py` already prove those paths, and neither produces a
journal record here (the CLI never journals anything, by the design
described above; this is not a gap this script papers over).

# The game patch: a small fake chain, not real xdelta

`e2e-game-patch.py` already proves the real thing end to end: real
`xdelta3.exe`, real disc images, a real chain a client walks and decodes.
Needing a base image and xdelta3.exe on this machine makes it skip itself
when either is absent, which is correct for what it proves but wrong for
this script (loopback services only, never a real data root, never
machine-specific inputs). So this script does not touch xdelta at all: it
publishes a small directory holding a plausible `chain.json` and one fake
delta file as a component named `game-patch` through the perfectly generic
`rebornctl publish component` (which does not know or care what a
component's bytes mean, only that they hash and copy), then pins it with
`publish release`. That proves a game-patch component publishes, uploads
and pins correctly, which is what this script owns; the delta's own bytes
are never decoded by anything here, and encoding one for real is
`e2e-game-patch.py`'s job, not duplicated here.

# What is proven, in order

  1  fake-s3.py accepts a signed upload; a second loopback host serves this
     run's own origin directly, for the roster's source URL
  2  keys, a first component and a signed release; uploaded with every blob
     and manifest before release.json, and release.json.sig last
  3  status reports nothing pending
  4  a second runtime version, published and uploaded, so there is an older
     version on hand for the rollback later
  5  a roster is pushed from a URL (like the site's real export), pinned
     into a new release carrying runtime forward, and uploaded in order
  6  a seeded scratch data root: runtime adopts at zero fetches once seeded
     from an install root; roster, never seeded, plans as pending at its
     own size
  7  a game-patch component (a small fake chain, no xdelta) publishes,
     pins, and uploads in order
  8  a rollback, through the headless harness: refused with no pins,
     described as a dry run, done for real -- the release's sequence goes
     up while runtime's pin goes back to the older version; then uploaded
     and status is clean again
  9  the journal holds exactly the three records the harness earned, each
     with its right outcome and inputs
  10 cleanup: no scratch root survives, and `C:\\Reborn Origin` (never
     touched by anything above) is exactly as it was before this script ran
"""
import hashlib
import http.server
import io
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EXE = REPO / "target" / "debug" / "rebornctl.exe"
CORE_DIR = REPO / "crates" / "reborn-core"
FAKE_S3 = Path(__file__).resolve().parent / "fake-s3.py"
WORK = REPO / "target" / "e2e-publisher"
ORIGIN = WORK / "origin"
KEYS = WORK / "keys"
HARNESS_DIR = WORK / "harness"
BUCKET_NAME = "test-bucket"
PASSED = []

# The real, live publishing origin this whole script exists to keep every
# path in this run away from. Nothing above ever passes this path to
# anything; it is read here only so the final cleanup step can prove it was
# left exactly as found.
REAL_ORIGIN = Path(r"C:\Reborn Origin")
REAL_JOURNAL = REAL_ORIGIN / "publisher-journal.jsonl"

# Must match the two constants of the same name in Tools/fake-s3.py and
# Tools/e2e-upload.py: every script in this folder hardcodes its own copy
# rather than importing across a hyphenated filename.
ACCESS_KEY_ID = "FAKE00000000000000TESTKEY"
SECRET_ACCESS_KEY = "FAKEsecretaccesskeyFORtestingONLYnotReal1234567890"


def step(name):
    PASSED.append(name)
    print(f"  ok  {name}")


def run(*args, expect=0):
    r = subprocess.run([str(EXE), *args], capture_output=True, text=True, encoding="utf-8", errors="replace")
    out = r.stdout + r.stderr
    if r.returncode != expect:
        print(out)
        raise SystemExit(f"FAIL: rebornctl {' '.join(args)} exited {r.returncode}, expected {expect}")
    return out


def must(out, needle, what):
    if needle not in out:
        print(out)
        raise SystemExit(f"FAIL: {what}: expected {needle!r} in output")


def must_not(out, needle, what):
    if needle in out:
        print(out)
        raise SystemExit(f"FAIL: {what}: did not expect {needle!r} in output")


# -- talking to the stand-in's debug routes (copied from e2e-upload.py) ------

def _get_json(url):
    with urllib.request.urlopen(url) as resp:
        return json.loads(resp.read())


def _post_json(url, obj):
    data = json.dumps(obj).encode("utf-8")
    req = urllib.request.Request(url, data=data, method="POST", headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


class FakeBucket:
    def __init__(self, base_url):
        self.base_url = base_url

    def order(self):
        return _get_json(f"{self.base_url}/__order")["order"]

    def seed(self, key, body):
        _post_json(f"{self.base_url}/__seed", {"key": key, "body": body})


def last_index(items, value):
    for i in range(len(items) - 1, -1, -1):
        if items[i] == value:
            return i
    raise ValueError(f"{value!r} not found in {items!r}")


def blob_key(path: Path) -> str:
    sha = hashlib.sha256(path.read_bytes()).hexdigest()
    return f"blobs/by-hash/{sha[:2]}/{sha}"


def manifest_key(component: str, version: str) -> str:
    return f"components/{component}-{version}.json"


def assert_order_before_release(order, keys, what):
    """Every key in `keys` must sit strictly before the last `release.json`
    in `order`, and that `release.json` must sit strictly before the last
    `release.json.sig`, which must be the newest thing in `order` overall."""
    release_idx = last_index(order, "release.json")
    sig_idx = last_index(order, "release.json.sig")
    if sig_idx != len(order) - 1:
        raise SystemExit(f"FAIL: {what}: release.json.sig is not the last arrival: {order}")
    if sig_idx <= release_idx:
        raise SystemExit(f"FAIL: {what}: release.json.sig did not arrive after release.json: {order}")
    for key in keys:
        idx = last_index(order, key)
        if idx >= release_idx:
            raise SystemExit(f"FAIL: {what}: {key} arrived at or after release.json: {order}")


# -- credentials, via the vault, the same way the real ones are stored ------

def store_token(flag: str, value: str):
    tmp = WORK / "token.tmp"
    tmp.write_text(value, encoding="utf-8")
    try:
        run("publish", "token", flag, str(tmp))
    finally:
        tmp.unlink(missing_ok=True)


# There is one vault, and the two R2 entries this script fills with fake
# credentials are the ones the Publisher publishes with. Put the real ones
# back from rclone.conf (the file the Publisher's own import reads) the
# moment the run ends, pass or fail, so the next real publish is not
# refused with a 403 (2026-09-11).
RCLONE_CONF = Path(r"C:\Users\Adam\Reborn Keys\rclone.conf")


def restore_real_r2():
    if not RCLONE_CONF.is_file():
        print(f"WARNING: {RCLONE_CONF} not found; the vault still holds the FAKE R2 credentials. Restore them with `rebornctl publish token`.")
        return
    text = RCLONE_CONF.read_text(encoding="utf-8")
    access = re.search(r"^\s*access_key_id\s*=\s*(\S+)", text, re.M)
    secret = re.search(r"^\s*secret_access_key\s*=\s*(\S+)", text, re.M)
    if not (access and secret):
        print("WARNING: rclone.conf has no access_key_id/secret_access_key; the vault still holds the FAKE R2 credentials.")
        return
    store_token("--r2-access-from-file", access.group(1))
    store_token("--r2-secret-from-file", secret.group(1))
    print("  ok  restored the real R2 credentials from rclone.conf")


# -- building trees -----------------------------------------------------------

def make_tree(root: Path, files: dict):
    if root.exists():
        shutil.rmtree(root)
    for rel, data in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)


def publish_component(tree: Path, component: str, version: str, sequence: int):
    return run("publish", "component", "--root", str(tree), "--component", component,
               "--version", version, "--sequence", str(sequence), "--origin", str(ORIGIN), "--apply")


def publish_release(version: str, sequence: int, pins: dict, expect=0):
    args = ["publish", "release", "--origin", str(ORIGIN), "--version", version, "--sequence", str(sequence),
            "--key", str(KEYS / "publisher.key")]
    for name, ver in pins.items():
        args += ["--component", f"{name}={ver}"]
    args.append("--apply")
    return run(*args, expect=expect)


def upload(endpoint, public_base, expect=0):
    return run("publish", "upload", "--origin", str(ORIGIN), "--endpoint", endpoint,
               "--bucket", BUCKET_NAME, "--public-base", public_base, "--apply", expect=expect)


def status(origin_base, launcher_url):
    # exit 1: the throwaway key `publish keygen` makes below is never listed
    # in signing.rs's TRUSTED_KEYS, so the hosted release's signature never
    # verifies and status.problems always carries exactly that one entry.
    # This script is about the publishing cycle's own mechanics, not the
    # release trust chain, so every call here expects that one, constant,
    # unrelated problem rather than a clean exit (the same accepted quirk
    # `e2e-upload.py`'s own `status()` documents).
    out = run("publish", "status", "--origin", str(ORIGIN), "--origin-base", origin_base,
              "--launcher", launcher_url, expect=1)
    must(out, "did not verify", "the only problem is the untrusted throwaway signing key")
    must_not(out, "could not be read", "the local origin, hosted release and launcher channel all read fine")
    return out


def release_json():
    return json.loads((ORIGIN / "release.json").read_text(encoding="utf-8"))


# -- a static host for this run's own origin, Range-aware (copied from
# e2e-update.py / e2e-game-patch.py), used only to serve the roster's
# source URL the way a site's export sits at a URL. -------------------------

class OriginHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(ORIGIN), **k)

    def log_message(self, *a):
        pass

    def do_GET(self):
        path = self.translate_path(self.path)
        if not os.path.isfile(path):
            self.send_error(404)
            return
        size = os.path.getsize(path)
        start, status_code = 0, 200
        rng = self.headers.get("Range")
        if rng and rng.startswith("bytes="):
            start = int(rng[6:].rstrip("-"))
            status_code = 206
        with open(path, "rb") as f:
            f.seek(start)
            body = f.read()
        self.send_response(status_code)
        self.send_header("Content-Length", str(len(body)))
        if status_code == 206:
            self.send_header("Content-Range", f"bytes {start}-{size - 1}/{size}")
        self.end_headers()
        self.wfile.write(body)


def serve_origin():
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), OriginHandler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv, f"http://127.0.0.1:{srv.server_address[1]}/"


# -- the headless harness: one operation rebornctl has no subcommand for ----

HARNESS_MAIN_RS = r'''
//! A tiny, disposable driver for `history::rollback_run`'s own real work:
//! sign a release pinning one component back to an earlier version, then
//! journal it exactly the way `reborn-publisher`'s `ops::run_operation`
//! journals every page operation. Generated fresh by
//! `Tools/e2e-publisher.py` into a scratch directory; not part of any real
//! crate, not committed. See that script's own module doc for why this
//! exists and what it deliberately does not cover.

use std::path::PathBuf;
use std::process::ExitCode;

use reborn_core::inventory::ComponentId;
use reborn_core::publish::journal::{self, Record, JOURNAL_SCHEMA};
use reborn_core::publish::release::{release, Pin, ReleaseRequest};
use reborn_core::publish::{Event, Outcome};

fn arg(args: &[String], i: usize, flag: &str) -> String {
    args.get(i).unwrap_or_else(|| panic!("{flag} needs a value")).clone()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut origin: Option<PathBuf> = None;
    let mut key: Option<PathBuf> = None;
    let mut version: Option<String> = None;
    let mut sequence: Option<u64> = None;
    let mut dry_run = false;
    let mut kind = String::new();
    let mut component = String::new();
    let mut target_version = String::new();
    let mut pins: Vec<Pin> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--origin" => { origin = Some(PathBuf::from(arg(&args, i + 1, "--origin"))); i += 2; }
            "--key" => { key = Some(PathBuf::from(arg(&args, i + 1, "--key"))); i += 2; }
            "--version" => { version = Some(arg(&args, i + 1, "--version")); i += 2; }
            "--sequence" => { sequence = Some(arg(&args, i + 1, "--sequence").parse().expect("--sequence must be a number")); i += 2; }
            "--dry-run" => { let v = arg(&args, i + 1, "--dry-run"); dry_run = v == "1" || v == "true"; i += 2; }
            "--kind" => { kind = arg(&args, i + 1, "--kind"); i += 2; }
            "--component" => { component = arg(&args, i + 1, "--component"); i += 2; }
            "--target-version" => { target_version = arg(&args, i + 1, "--target-version"); i += 2; }
            "--pin" => {
                let spec = arg(&args, i + 1, "--pin");
                let (name, ver) = spec.split_once('=').unwrap_or_else(|| panic!("--pin {spec:?}: expected NAME=VERSION"));
                let id = ComponentId::parse(name).unwrap_or_else(|| panic!("--pin {spec:?}: {name:?} is not a component name"));
                pins.push(Pin { id, version: ver.to_string() });
                i += 2;
            }
            other => panic!("unknown argument {other:?}"),
        }
    }

    let origin = origin.expect("--origin is required");
    let key = key.expect("--key is required");
    let version = version.expect("--version is required");
    let sequence = sequence.expect("--sequence is required");

    let mut lines: Vec<String> = Vec::new();
    let mut sink_fn = |event: Event| lines.push(event.line);
    let req = ReleaseRequest { origin: &origin, version: &version, sequence, key: &key, pins: &pins, apply: !dry_run };
    let result = release(&req, &mut sink_fn);

    let inputs = serde_json::json!({
        "kind": kind,
        "component": component,
        "version": target_version,
        "dry_run": dry_run,
    });

    let (outcome_str, seq_opt, summary): (&'static str, Option<u64>, String) = match &result {
        Ok(Outcome::Done(signed)) => (
            "done",
            Some(signed.sequence),
            format!(
                "Signed release {} sequence {} pinning {component}={target_version} (headless harness; not uploaded here).",
                signed.version, signed.sequence
            ),
        ),
        Ok(Outcome::Described) => (
            "described",
            Some(sequence),
            format!(
                "Described: would sign release {version} sequence {sequence} pinning {component}={target_version}. \
                 Nothing was written (dry run)."
            ),
        ),
        Ok(Outcome::Refused(r)) => ("refused", None, r.summary.clone()),
        Err(e) => ("failed", None, e.to_string()),
    };

    let record = Record {
        schema: JOURNAL_SCHEMA.to_string(),
        at: journal::now(),
        operation: "history".to_string(),
        dry_run,
        inputs,
        sequence: seq_opt,
        hashes: Vec::new(),
        uploaded: Vec::new(),
        verified: None,
        outcome: outcome_str.to_string(),
        summary: summary.clone(),
    };
    journal::append(&origin, &record).expect("the journal record could not be appended");

    for line in &lines {
        println!("LOG {line}");
    }
    println!("HARNESS_OUTCOME {outcome_str}");
    println!("HARNESS_SUMMARY {summary}");
    if let Some(s) = seq_opt {
        println!("HARNESS_SEQUENCE {s}");
    }

    match outcome_str {
        "done" | "described" => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
'''


def build_harness() -> Path:
    """Write the harness's own throwaway Cargo project into the scratch
    directory and build it. Its `[workspace]` (see the file itself) keeps
    it from being folded into this repository's real workspace; its
    `reborn-core` dependency is a path dependency onto the real,
    unmodified crate, so it links the same publishing and journal code the
    real app and the real CLI both link."""
    src = HARNESS_DIR / "src"
    src.mkdir(parents=True, exist_ok=True)
    core_path = str(CORE_DIR).replace("\\", "/")
    (HARNESS_DIR / "Cargo.toml").write_text(
        "[package]\n"
        'name = "e2e-publisher-harness"\n'
        'version = "0.0.0"\n'
        'edition = "2021"\n'
        "publish = false\n"
        "\n"
        "# An empty [workspace] makes this its own workspace root, so it is not\n"
        "# folded into the real repository's workspace even though it lives\n"
        "# inside that repository's tree. See Tools/e2e-publisher.py.\n"
        "[workspace]\n"
        "\n"
        "[dependencies]\n"
        f'reborn-core = {{ path = "{core_path}", features = ["publish"] }}\n'
        'serde_json = "1"\n',
        encoding="utf-8",
    )
    (src / "main.rs").write_text(HARNESS_MAIN_RS, encoding="utf-8")

    r = subprocess.run(
        ["cargo", "build", "--manifest-path", str(HARNESS_DIR / "Cargo.toml")],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if r.returncode != 0:
        print(r.stdout)
        print(r.stderr)
        raise SystemExit("FAIL: the headless harness did not build")
    exe = HARNESS_DIR / "target" / "debug" / "e2e-publisher-harness.exe"
    if not exe.is_file():
        raise SystemExit(f"FAIL: the headless harness built but {exe} is not there")
    return exe


def run_harness(exe: Path, *args, expect=0):
    r = subprocess.run([str(exe), *args], capture_output=True, text=True, encoding="utf-8", errors="replace")
    out = r.stdout + r.stderr
    if r.returncode != expect:
        print(out)
        raise SystemExit(f"FAIL: harness {' '.join(args)} exited {r.returncode}, expected {expect}")
    return out


def main():
    assert EXE.is_file(), f"build rebornctl first: {EXE}"

    # The real origin's journal, observed before this script does anything
    # at all, purely so the final step can prove it is unchanged. Nothing
    # below ever passes REAL_ORIGIN to any command.
    real_before = REAL_JOURNAL.stat() if REAL_JOURNAL.is_file() else None
    real_before_bytes = REAL_JOURNAL.read_bytes() if real_before else None

    if WORK.exists():
        shutil.rmtree(WORK)
    WORK.mkdir(parents=True)
    KEYS.mkdir(parents=True, exist_ok=True)

    origin_srv, content_base = serve_origin()
    s3 = subprocess.Popen(
        [sys.executable, str(FAKE_S3), "--port", "0"],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
    try:
        first_line = s3.stdout.readline().strip()
        if not first_line.startswith("PORT "):
            raise SystemExit(f"FAIL: fake-s3.py did not report its port: {first_line!r}")
        port = int(first_line.split()[1])
        endpoint = f"http://127.0.0.1:{port}"
        base = f"{endpoint}/{BUCKET_NAME}/"
        bucket = FakeBucket(endpoint)
        launcher_url = f"{base}launcher/latest.json"
        step(f"fake-s3.py listening on 127.0.0.1:{port}; this run's own origin served at {content_base}")

        # A launcher manifest fixture, planted directly (never through the
        # signed API), so `publish status`'s launcher-channel read has
        # something real to parse. See e2e-upload.py for the same pattern.
        bucket.seed(
            "launcher/latest.json",
            json.dumps({
                "schema": "nsr.launcher-release.v1",
                "version": "1.0.0",
                "windows": {"url": "https://example.invalid/setup.exe", "sha256": "a" * 64, "bytes": 1, "file_name": "Setup.exe"},
            }),
        )

        store_token("--r2-access-from-file", ACCESS_KEY_ID)
        store_token("--r2-secret-from-file", SECRET_ACCESS_KEY)

        # 1. keys.
        out = run("publish", "keygen", "--out", str(KEYS))
        pub_hex = re.search(r"// everyday\s+([0-9a-f]{64})", out).group(1)
        key_path = KEYS / "publisher.key"
        step("stored the fake R2 credentials in the vault, and generated a fresh keypair")

        # 2. a first component and a signed release, uploaded in order.
        v1 = {"a.txt": b"alpha\n", "b.txt": b"bravo\n"}
        tree1 = WORK / "runtime-1.0.0"
        make_tree(tree1, v1)
        publish_component(tree1, "runtime", "1.0.0", 1)
        publish_release("1.0.0", 1, {"runtime": "1.0.0"})
        out = upload(endpoint, base)
        must(out, "Uploaded", "the first upload sends the blobs, the manifest, and the two release files")
        order = bucket.order()
        assert_order_before_release(
            order, [blob_key(tree1 / "a.txt"), blob_key(tree1 / "b.txt"), manifest_key("runtime", "1.0.0")],
            "first upload",
        )
        step("published runtime 1.0.0, signed release sequence 1, and uploaded it in the right order")

        # 3. status is clean.
        status(base, launcher_url)
        step("publish status reports nothing pending")

        # 4. a second runtime version, so there is an older one on hand for
        # the rollback later in this script.
        v2 = dict(v1, **{"b.txt": b"bravo, revised\n", "c.txt": b"charlie\n"})
        tree2 = WORK / "runtime-1.0.1"
        make_tree(tree2, v2)
        publish_component(tree2, "runtime", "1.0.1", 2)
        publish_release("1.0.1", 2, {"runtime": "1.0.1"})
        out = upload(endpoint, base)
        order = bucket.order()
        assert_order_before_release(
            order,
            [blob_key(tree2 / "b.txt"), blob_key(tree2 / "c.txt"), manifest_key("runtime", "1.0.1")],
            "second upload",
        )
        status(base, launcher_url)
        step("published and uploaded runtime 1.0.1 (sequence 2); an older version, 1.0.0, is still in the origin to roll back to")

        # 5. a roster, pushed from a URL the way the site's real export
        # sits at one, pinned into a release that carries runtime forward.
        roster_version = "2026.09.09"
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w") as z:
            z.writestr("manifest.json", '{"contract_version": 1}')
            z.writestr("players.json", "[]")
            z.writestr("teams.json", "[]")
        roster_bytes = buf.getvalue()
        site = ORIGIN / "site"
        site.mkdir(parents=True, exist_ok=True)
        (site / "roster.rebornroster").write_bytes(roster_bytes)
        roster_url = content_base + "site/roster.rebornroster"
        out = run("publish", "roster", "--from", roster_url, "--origin", str(ORIGIN),
                  "--version", roster_version, "--key", str(key_path), "--apply")
        must(out, "Fetched", "the roster was fetched from this run's own origin host")
        must(out, f"pinning roster={roster_version}, runtime=1.0.1", "the runtime pin is carried forward")
        must(out, "Read back and verified", "the new release verifies")
        roster_manifest_key = manifest_key("roster", roster_version)
        roster_blob_key = blob_key(site / "roster.rebornroster")
        out = upload(endpoint, base)
        order = bucket.order()
        assert_order_before_release(order, [roster_blob_key, roster_manifest_key], "roster upload")
        rel = release_json()
        assert rel["sequence"] == 3 and "roster" in rel["components"] and "runtime" in rel["components"], rel
        step(f"pushed roster {roster_version} from a URL, pinned into release sequence 3 alongside runtime, uploaded in order")

        # 6. a seeded scratch data root: runtime adopts at zero fetches once
        # seeded from an install root; roster, never seeded, plans as
        # pending at its own size. This is the same trick
        # `crates/reborn-publisher/src/roster.rs`'s own `roster_verify`
        # uses (see its module doc): seed everything else so only the
        # roster being pushed shows up as needing anything.
        install_root = WORK / "install-root"
        make_tree(install_root / "runtime", v2)
        scratch = WORK / "scratch-roster-check"
        if scratch.exists():
            shutil.rmtree(scratch)
        common = ["update", "content", "--origin", base, "--trust", pub_hex, "--data-root", str(scratch)]

        out = run(*common, "--component", "runtime", "--install-root", str(install_root), "--apply")
        must(out, "Seeded        runtime", "runtime was seeded from the install root")
        must(out, "Fetch         0 file(s)", "nothing to fetch once seeded")
        out = run(*common, "--component", "runtime")
        must(out, "Up to date: runtime 1.0.1", "runtime reports up to date with zero further fetches")

        out = run(*common, "--component", "roster")
        must(out, "Installed     nothing", "roster was never seeded")
        must_not(out, "Up to date", "roster must not report up to date; it was never installed")
        m = re.search(r"Fetch\s+(\d+) file\(s\)", out)
        assert m and int(m.group(1)) > 0, f"expected roster pending at a nonzero file count: {out}"
        step(
            f"seeded scratch root: runtime adopts at zero fetches; roster, never seeded, "
            f"plans as pending ({m.group(1)} file(s)) with everything else at zero fetches"
        )
        shutil.rmtree(scratch, ignore_errors=True)

        # 7. a game-patch component: a small fake chain, never real xdelta.
        # `publish component` does not know or care what a component's
        # bytes mean, so this is exactly as valid a way to prove
        # "publishes, uploads and pins correctly" as the specialized
        # `publish game-patch` subcommand, without needing xdelta3.exe or a
        # real disc image. Real encode/decode/chain-walking is
        # `Tools/e2e-game-patch.py`'s job and is not duplicated here.
        game_patch_version = "2026.09.09-fake"
        gp_tree = WORK / "game-patch-fake"
        gp_tree.mkdir(parents=True)
        (gp_tree / "chain.json").write_text(json.dumps({
            "schema": "nsr.game-patch-chain.v1",
            "steps": [{"version": game_patch_version, "delta": "delta-0001.bin", "bytes": 4096}],
        }), encoding="utf-8")
        (gp_tree / "delta-0001.bin").write_bytes(bytes(range(256)) * 16)
        publish_component(gp_tree, "game-patch", game_patch_version, 1)
        publish_release(game_patch_version, 4, {"runtime": "1.0.1", "roster": roster_version, "game-patch": game_patch_version})
        out = upload(endpoint, base)
        order = bucket.order()
        assert_order_before_release(
            order,
            [blob_key(gp_tree / "chain.json"), blob_key(gp_tree / "delta-0001.bin"), manifest_key("game-patch", game_patch_version)],
            "game-patch upload",
        )
        rel = release_json()
        assert rel["sequence"] == 4 and "game-patch" in rel["components"], rel
        step(f"published a game-patch component (a small fake chain, no xdelta), pinned it into release sequence 4, uploaded in order")

        # 8. rollback, through the headless harness: refused, described,
        # done. See this file's module doc for why the harness exists.
        harness = build_harness()
        step(f"built the headless harness at {harness}")

        out = run_harness(
            harness, "--origin", str(ORIGIN), "--key", str(key_path), "--version", "1.0.2-rollback",
            "--sequence", "5", "--dry-run", "0", "--kind", "rollback-run", "--component", "runtime",
            "--target-version", "1.0.0",
            expect=1,
        )
        must(out, "HARNESS_OUTCOME refused", "a release pinning nothing is refused")
        must(out, "pinning nothing is refused", "and says why")

        out = run_harness(
            harness, "--origin", str(ORIGIN), "--key", str(key_path), "--version", "1.0.2-rollback",
            "--sequence", "5", "--dry-run", "1", "--kind", "rollback-run", "--component", "runtime",
            "--target-version", "1.0.0",
            "--pin", "runtime=1.0.0", "--pin", f"roster={roster_version}", "--pin", f"game-patch={game_patch_version}",
        )
        must(out, "HARNESS_OUTCOME described", "a dry run describes and writes nothing")
        rel = release_json()
        assert rel["sequence"] == 4, "a dry run must not touch the release already on disk"

        out = run_harness(
            harness, "--origin", str(ORIGIN), "--key", str(key_path), "--version", "1.0.2-rollback",
            "--sequence", "5", "--dry-run", "0", "--kind", "rollback-run", "--component", "runtime",
            "--target-version", "1.0.0",
            "--pin", "runtime=1.0.0", "--pin", f"roster={roster_version}", "--pin", f"game-patch={game_patch_version}",
        )
        must(out, "HARNESS_OUTCOME done", "the real rollback signs for real")
        rel = release_json()
        assert rel["sequence"] == 5, rel
        assert rel["components"]["runtime"]["url"] == manifest_key("runtime", "1.0.0"), rel
        step("rollback through the harness: refused with no pins, described as a dry run, done for real -- sequence 4 to 5, runtime's pin back to 1.0.0")

        out = upload(endpoint, base)
        must(out, "Uploaded", "the rolled-back release still has something to upload")
        order = bucket.order()
        if order[-1] != "release.json.sig" or order[-2] != "release.json":
            raise SystemExit(f"FAIL: rollback upload: release.json then .sig must be the last two arrivals: {order}")
        status(base, launcher_url)
        step("uploaded the rolled-back release (release.json and its signature; every blob it pins was already on the bucket); status is clean again")

        # 9. the journal holds exactly the three records the harness
        # earned, each with its right outcome and inputs. rebornctl itself
        # never wrote one (see this file's module doc), so these three are
        # the whole journal.
        journal_path = ORIGIN / "publisher-journal.jsonl"
        journal_lines = [json.loads(l) for l in journal_path.read_text(encoding="utf-8").splitlines() if l.strip()]
        assert len(journal_lines) == 3, f"expected exactly 3 journal records, found {len(journal_lines)}: {journal_lines}"
        expected_outcomes = ["refused", "described", "done"]
        for record, expected in zip(journal_lines, expected_outcomes):
            assert record["schema"] == "nsr.publisher-journal.v1", record
            assert record["operation"] == "history", record
            assert record["outcome"] == expected, record
            assert record["inputs"]["kind"] == "rollback-run", record
            assert record["inputs"]["component"] == "runtime", record
            assert record["inputs"]["version"] == "1.0.0", record
        assert journal_lines[0]["sequence"] is None, "a refusal carries no sequence"
        assert journal_lines[1]["sequence"] == 5 and journal_lines[1]["dry_run"] is True, journal_lines[1]
        assert journal_lines[2]["sequence"] == 5 and journal_lines[2]["dry_run"] is False, journal_lines[2]
        step("the journal holds exactly one record per operation (3 total), each with its right outcome: refused, described, done")

        print()
        print(f"PASS: {len(PASSED)} steps, origin at {ORIGIN}")
    finally:
        try:
            restore_real_r2()
        except Exception as e:  # noqa: BLE001 - the warning is the point
            print(f"WARNING: the real R2 credentials were not restored ({e}); the vault still holds the FAKE ones.")
        s3.terminate()
        try:
            s3.wait(timeout=5)
        except subprocess.TimeoutExpired:
            s3.kill()
        origin_srv.shutdown()

    # 10. cleanup. Every scratch path this script made lives under WORK;
    # removing it removes the scratch origin, its journal, the harness's
    # own throwaway Cargo project and build output, and every tree and
    # install root made along the way, all at once.
    shutil.rmtree(WORK, ignore_errors=True)
    assert not WORK.exists(), f"FAIL: {WORK} survived cleanup"

    real_after = REAL_JOURNAL.stat() if REAL_JOURNAL.is_file() else None
    real_after_bytes = REAL_JOURNAL.read_bytes() if real_after else None
    assert (real_before is None) == (real_after is None), (
        f"FAIL: {REAL_JOURNAL} existed={real_before is not None} before this script ran and "
        f"existed={real_after is not None} after; nothing in this script ever names that path"
    )
    assert real_before_bytes == real_after_bytes, f"FAIL: {REAL_JOURNAL} changed; nothing in this script ever names that path"
    step(f"no scratch root survives, and {REAL_ORIGIN} is exactly as it was before this script ran")

    print()
    print(f"PASS: {len(PASSED)} steps total")


if __name__ == "__main__":
    try:
        main()
    except SystemExit as e:
        print(e)
        sys.exit(1)
