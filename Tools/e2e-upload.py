"""End to end: publish an origin, upload it to a loopback S3 stand-in with
the real `rebornctl`, and prove the whole pipeline against `Tools/fake-s3.py`
rather than the real bucket.

    py -3 Tools/e2e-upload.py        (needs target/debug/rebornctl.exe)

`rebornctl publish upload` and `rebornctl publish status` never take a
secret on the command line; both read the bucket's write credentials from
the platform vault. This script stores a fixed pair of fake credentials
there with `rebornctl publish token --r2-access-from-file/--r2-secret-from-
file`, the same command Adam uses for the real ones, so the CLI's own
vault-reading code path is exercised for real. Only the server on the other
end is fake.

What is proven, in order:
  1  fake-s3.py accepts a correctly signed request and serves it back
  2  a first upload sends every blob and manifest before release.json, and
     release.json.sig last
  3  status reports nothing pending once the bucket matches the local origin
  4  a second published version makes status report it pending, and
     uploading again clears it
  5  a part that fails every attempt aborts the multipart upload and leaves
     none open, and a retry with the failure lifted succeeds
  6  a wrong secret in the vault is rejected by the stand-in (a tampered
     signature never reaches the bucket, and nothing not already there
     appears in the arrival order); restoring the right secret recovers
"""
import hashlib
import json
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EXE = REPO / "target" / "debug" / "rebornctl.exe"
FAKE_S3 = Path(__file__).resolve().parent / "fake-s3.py"
WORK = REPO / "target" / "e2e-upload"
ORIGIN = WORK / "origin"
KEYS = WORK / "keys"
BUCKET_NAME = "test-bucket"
PASSED = []

# Must match the two constants of the same name in Tools/fake-s3.py: this
# script and the server it drives each hardcode their own copy rather than
# importing across a hyphenated filename, the same way every other script in
# this folder is a standalone file.
ACCESS_KEY_ID = "FAKE00000000000000TESTKEY"
SECRET_ACCESS_KEY = "FAKEsecretaccesskeyFORtestingONLYnotReal1234567890"
WRONG_SECRET_ACCESS_KEY = "WRONGsecretThatWillNeverVerifyAgainstTheFakeServer0"


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


# -- talking to the stand-in's debug routes -----------------------------------

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

    def open_uploads(self):
        return _get_json(f"{self.base_url}/__uploads")["open"]

    def fail_parts(self, n):
        _post_json(f"{self.base_url}/__control", {"fail_parts": n})

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
    `release.json.sig`, which must be the newest thing in `order` overall
    (nothing this script does afterward until the next upload appends past
    it)."""
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


# -- credentials, via the vault, the same way the real ones are stored -------

def store_token(flag: str, value: str):
    tmp = WORK / "token.tmp"
    tmp.write_text(value, encoding="utf-8")
    try:
        run("publish", "token", flag, str(tmp))
    finally:
        tmp.unlink(missing_ok=True)


# -- building the origin ------------------------------------------------------

def make_tree(root: Path, files: dict):
    if root.exists():
        shutil.rmtree(root)
    for rel, data in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)


def publish_version(tree: Path, version: str, sequence: int):
    out = run("publish", "component", "--root", str(tree), "--component", "runtime",
              "--version", version, "--sequence", str(sequence), "--origin", str(ORIGIN), "--apply")
    run("publish", "release", "--origin", str(ORIGIN), "--version", version, "--sequence", str(sequence),
        "--key", str(KEYS / "publisher.key"), "--component", f"runtime={version}", "--apply")
    return out


def upload(endpoint, public_base, expect=0):
    return run("publish", "upload", "--origin", str(ORIGIN), "--endpoint", endpoint,
               "--bucket", BUCKET_NAME, "--public-base", public_base, "--apply", expect=expect)


def status(origin_base, launcher_url):
    # exit 1: `publish keygen` above makes a throwaway key never listed in
    # signing.rs's TRUSTED_KEYS, so the hosted release's signature never
    # verifies and `status.problems` always carries exactly that one entry.
    # This test is about the upload/pending mechanics, not the release trust
    # chain, so every call here expects that one, constant, unrelated
    # problem rather than a clean exit.
    out = run("publish", "status", "--origin", str(ORIGIN), "--origin-base", origin_base, "--launcher", launcher_url, expect=1)
    must(out, "did not verify", "the only problem is the untrusted throwaway signing key")
    must_not(out, "could not be read", "the local origin, hosted release and launcher channel all read fine")
    return out


def main():
    assert EXE.is_file(), f"build rebornctl first: {EXE}"
    if WORK.exists():
        shutil.rmtree(WORK)
    WORK.mkdir(parents=True)
    KEYS.mkdir(parents=True, exist_ok=True)

    server = subprocess.Popen(
        [sys.executable, str(FAKE_S3), "--port", "0"],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
    try:
        first_line = server.stdout.readline().strip()
        if not first_line.startswith("PORT "):
            raise SystemExit(f"FAIL: fake-s3.py did not report its port: {first_line!r}")
        port = int(first_line.split()[1])
        endpoint = f"http://127.0.0.1:{port}"
        base = f"{endpoint}/{BUCKET_NAME}/"
        bucket = FakeBucket(endpoint)
        launcher_url = f"{base}launcher/latest.json"
        step(f"fake-s3.py listening on 127.0.0.1:{port}")

        # A launcher manifest fixture, planted directly (never through the
        # signed API: the CLI never uploads this file, `publish status`
        # only ever reads it), so the launcher-channel read has something
        # real to parse instead of a 404 cluttering every status check
        # below with an unrelated problem.
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
        step("stored the fake R2 credentials in the vault the same way the real ones are stored")

        # 1. keys, a first version, upload it.
        run("publish", "keygen", "--out", str(KEYS))
        v1 = {"a.txt": b"alpha\n", "b.txt": b"bravo\n"}
        tree1 = WORK / "tree1"
        make_tree(tree1, v1)
        publish_version(tree1, "1.0.0", 1)
        a_key, b_key = blob_key(tree1 / "a.txt"), blob_key(tree1 / "b.txt")
        m1_key = manifest_key("runtime", "1.0.0")
        step("published runtime 1.0.0 into the local origin")

        out = upload(endpoint, base)
        must(out, "Uploaded 5 object(s)", "the first upload sends 2 blobs, 1 manifest, 2 release files")
        order = bucket.order()
        assert_order_before_release(order, [a_key, b_key, m1_key], "first upload")
        step("every blob and the manifest arrived before release.json, and release.json.sig arrived last")

        out = status(base, launcher_url)
        must(out, "Pending         nothing", "status reports nothing pending once uploaded")
        step("publish status reports nothing pending")

        # 2. a second version, published but not yet uploaded: pending.
        v2 = dict(v1, **{"b.txt": b"bravo, revised\n", "c.txt": b"charlie\n"})
        tree2 = WORK / "tree2"
        make_tree(tree2, v2)
        publish_version(tree2, "1.0.1", 2)
        out = status(base, launcher_url)
        must(out, "Pending         1 component(s):", "a published-but-not-uploaded version is pending")
        must(out, "runtime", "the pending component is named")
        step("publishing a second version without uploading it makes status report it pending")

        b2_key, c_key = blob_key(tree2 / "b.txt"), blob_key(tree2 / "c.txt")
        m2_key = manifest_key("runtime", "1.0.1")
        out = upload(endpoint, base)
        order = bucket.order()
        assert_order_before_release(order, [b2_key, c_key, m2_key], "second upload")
        out = status(base, launcher_url)
        must(out, "Pending         nothing", "status is clean again after uploading the second version")
        step("uploading the second version clears pending, in the right order")

        # 3. a third version whose one new blob is at the multipart
        # threshold (256 MiB), so this exercises Create/Part/Complete for
        # real rather than only through Rust's own unit tests.
        big = bytes(range(256)) * ((256 * 1024 * 1024) // 256)
        assert len(big) == 256 * 1024 * 1024
        tree3 = WORK / "tree3"
        make_tree(tree3, dict(v2, **{"big.bin": big}))
        publish_version(tree3, "1.0.2", 3)
        big_key = blob_key(tree3 / "big.bin")
        m3_key = manifest_key("runtime", "1.0.2")
        step("published runtime 1.0.2, adding one 256 MiB blob")

        # Fail every attempt at every part: comfortably more than the
        # default 3 attempts, so the multipart upload cannot succeed no
        # matter how many parts it turns out to have.
        bucket.fail_parts(10)
        out = upload(endpoint, base, expect=1)
        must(out, "aborted the multipart upload", "a part that never succeeds triggers an abort")
        if bucket.open_uploads():
            raise SystemExit(f"FAIL: a multipart upload is still open after the abort: {bucket.open_uploads()}")
        must_not(bucket.order(), big_key, "the big blob never completed, so it never arrived")
        step("an interrupted multipart upload aborts and leaves none open")

        bucket.fail_parts(0)
        out = upload(endpoint, base)
        must(out, "multipart upload", "the retry actually goes multipart")
        order = bucket.order()
        assert_order_before_release(order, [big_key, m3_key], "third upload, after the interruption is lifted")
        out = status(base, launcher_url)
        must(out, "Pending         nothing", "status is clean after the retried multipart upload")
        step("retrying with the failure lifted completes the multipart upload and clears pending")

        # 4. a wrong secret in the vault is a tampered signature: the
        # stand-in must reject it and nothing may reach the bucket.
        before = len(bucket.order())
        store_token("--r2-secret-from-file", WRONG_SECRET_ACCESS_KEY)
        out = upload(endpoint, base, expect=1)
        must(out, "403", "the stand-in rejects the wrongly-signed request")
        if len(bucket.order()) != before:
            raise SystemExit(f"FAIL: something arrived under a wrong secret: {bucket.order()[before:]}")
        step("a wrong secret is rejected by the stand-in and nothing new arrives")

        store_token("--r2-secret-from-file", SECRET_ACCESS_KEY)
        out = upload(endpoint, base)
        must(out, "Uploaded 2 object(s)", "restoring the right secret uploads the two release files again")
        out = status(base, launcher_url)
        must(out, "Pending         nothing", "status is clean once the right secret is back")
        step("restoring the correct secret recovers; the run ends with nothing pending")

        print()
        print(f"PASS: {len(PASSED)} steps, origin at {ORIGIN}")
    finally:
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()


if __name__ == "__main__":
    try:
        main()
    except SystemExit as e:
        print(e)
        sys.exit(1)
