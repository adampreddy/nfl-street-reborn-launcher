"""End to end: publish an origin, serve it, update a client from it.

    py -3 Tools/e2e-update.py        (needs target/debug/rebornctl.exe)

Everything runs against a scratch data root, never the real one. The server
honors Range and logs every request, so the script can prove which blobs
were fetched and that a resume asked from the right offset.

What is proven, in order:
  1 keygen, publish component, publish release, and the release reads back
  2 a fresh client plans 3 fetches, installs, and is then up to date
  3 a damaged live file is planned as one repair fetch and repaired
  4 a second version with one changed and one new file publishes only two
    new blobs, and the client fetches exactly those two
  5 a seeded partial download resumes with a ranged request answered 206
  6 the pointer records current and previous, so rollback is possible
  7 a release with a lower sequence is refused by the publisher
  8 a tampered signature is refused by the client, and nothing changes
"""
import http.server
import os
import re
import shutil
import subprocess
import sys
import threading
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EXE = REPO / "target" / "debug" / "rebornctl.exe"
WORK = REPO / "target" / "e2e-update"
ORIGIN = WORK / "origin"
DATA = WORK / "data"
LOG = []
PASSED = []


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


# -- a static host that honors Range and remembers every request -------------
class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(ORIGIN), **k)

    def log_message(self, *a):
        pass

    def do_GET(self):
        path = self.translate_path(self.path)
        if not os.path.isfile(path):
            LOG.append((self.path, None, 404))
            self.send_error(404)
            return
        size = os.path.getsize(path)
        start, status = 0, 200
        rng = self.headers.get("Range")
        if rng and rng.startswith("bytes="):
            start = int(rng[6:].rstrip("-"))
            if start > size:
                LOG.append((self.path, start, 416))
                self.send_response(416)
                self.end_headers()
                return
            status = 206
        with open(path, "rb") as f:
            f.seek(start)
            body = f.read()
        self.send_response(status)
        self.send_header("Content-Length", str(len(body)))
        if status == 206:
            self.send_header("Content-Range", f"bytes {start}-{size - 1}/{size}")
        self.end_headers()
        self.wfile.write(body)
        LOG.append((self.path, start, status))


def serve():
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return f"http://127.0.0.1:{srv.server_address[1]}/"


def blob_gets():
    return [e for e in LOG if e[0].startswith("/blobs/")]


# -- the trees -----------------------------------------------------------------
def make_tree(root, files):
    if root.exists():
        shutil.rmtree(root)
    for rel, data in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)


def main():
    assert EXE.is_file(), f"build rebornctl first: {EXE}"
    if WORK.exists():
        shutil.rmtree(WORK)
    WORK.mkdir(parents=True)

    c_bytes = bytes(range(256)) * 800  # 204,800 bytes, not compressible-trivial
    v1 = {"a.txt": b"alpha\n", "b.txt": b"bravo\n", "sub/c.bin": c_bytes}
    make_tree(WORK / "tree1", v1)

    # 1. keys, component, release
    out = run("publish", "keygen", "--out", str(WORK / "keys"))
    m = re.search(r"// everyday\s+([0-9a-f]{64})", out)
    assert m, out
    pub_hex = m.group(1)
    must(out, "// rotation", "keygen prints the rotation key too")
    run("publish", "keygen", "--out", str(WORK / "keys"), expect=1)  # never overwrites
    step("keygen writes both keys and refuses to overwrite them")

    out = run("publish", "component", "--root", str(WORK / "tree1"), "--component", "runtime",
              "--version", "1.0.0", "--sequence", "1", "--origin", str(ORIGIN), "--apply")
    must(out, "3 new", "first publish: every blob is new")
    assert (ORIGIN / "components" / "runtime-1.0.0.json").is_file()
    step("publish component writes the manifest and 3 blobs")

    out = run("publish", "release", "--origin", str(ORIGIN), "--version", "1.0.0", "--sequence", "1",
              "--key", str(WORK / "keys" / "publisher.key"), "--component", "runtime=1.0.0", "--apply")
    must(out, "Read back and verified", "the signed release verifies off the disk")
    must(out, "not in TRUSTED_KEYS", "it warns that no shipped launcher trusts this key yet")
    step("publish release signs, and reads the signed file back through the verifier")

    base = serve()
    common = ["update", "content", "--origin", base, "--component", "runtime",
              "--trust", pub_hex, "--data-root", str(DATA)]

    # 2. fresh install
    out = run(*common)
    must(out, "Installed     nothing", "plan on an empty root")
    must(out, "Fetch         3 file(s)", "plan fetches all three")
    must(out, "Nothing was downloaded", "no --apply, no download")
    assert not DATA.exists() or not (DATA / "runtime").exists(), "plan must not create the live tree"
    out = run(*common, "--apply")
    must(out, "Live          runtime 1.0.0", "install went live")
    for rel, data in v1.items():
        got = (DATA / "runtime" / rel).read_bytes()
        assert got == data, f"{rel}: live bytes differ"
    pointer = (DATA / "versions" / "runtime" / "current.json").read_text()
    assert '"1.0.0"' in pointer, pointer
    out = run(*common)
    must(out, "Up to date: runtime 1.0.0", "second run is a no-op")
    step("fresh client installs 3 files, pointer says 1.0.0, second run is up to date")

    # 3. repair
    (DATA / "runtime" / "b.txt").write_bytes(b"broken")
    out = run(*common)
    must(out, "Fetch         1 file(s)", "one damaged file to fetch")
    must(out, "damaged  b.txt", "and it is b.txt, named as damaged")
    out = run(*common, "--apply")
    must(out, "Live          runtime 1.0.0", "repair went live")
    assert (DATA / "runtime" / "b.txt").read_bytes() == b"bravo\n"
    step("a damaged live file is planned as one repair fetch and repaired")

    # 4. a second version: one changed file, one new file
    v2 = dict(v1, **{"b.txt": b"bravo, revised\n", "d.txt": b"delta\n"})
    make_tree(WORK / "tree2", v2)
    out = run("publish", "component", "--root", str(WORK / "tree2"), "--component", "runtime",
              "--version", "1.0.1", "--sequence", "2", "--origin", str(ORIGIN), "--apply")
    must(out, "2 new", "only the changed and the new file are new blobs")
    must(out, "2 already in the origin", "the unchanged two are shared")
    run("publish", "release", "--origin", str(ORIGIN), "--version", "1.0.1", "--sequence", "2",
        "--key", str(WORK / "keys" / "publisher.key"), "--component", "runtime=1.0.1", "--apply")
    step("v1.0.1 publishes 2 new blobs and shares 2")

    # 5. seed a partial download of d.txt so the client must resume it
    part = DATA / "downloads" / "runtime" / "1.0.1" / "d.txt.part"
    part.parent.mkdir(parents=True, exist_ok=True)
    part.write_bytes(b"del")
    LOG.clear()
    out = run(*common, "--apply")
    must(out, "Live          runtime 1.0.1", "update went live")
    must(out, "Rollback to   1.0.0", "the previous version is retained")
    gets = blob_gets()
    assert len(gets) == 2, f"expected exactly 2 blob fetches (b and d), got {gets}"
    resumed = [g for g in gets if g[2] == 206 and g[1] == 3]
    assert resumed, f"expected a 206 from offset 3 for the seeded part, got {gets}"
    for rel, data in v2.items():
        assert (DATA / "runtime" / rel).read_bytes() == data, f"{rel}: v2 bytes differ"
    pointer = (DATA / "versions" / "runtime" / "current.json").read_text()
    assert '"1.0.1"' in pointer and '"1.0.0"' in pointer, pointer
    step("update fetched exactly the 2 changed blobs, resumed d.txt from byte 3 with a 206, pointer has current and previous")

    # 6b. a version that DROPS a file: it must leave the player's disk, but
    # into prior/, never deleted, so rollback can bring it back.
    v3 = {k: v for k, v in v2.items() if k != "a.txt"}
    make_tree(WORK / "tree3", v3)
    run("publish", "component", "--root", str(WORK / "tree3"), "--component", "runtime",
        "--version", "1.0.2", "--sequence", "3", "--origin", str(ORIGIN), "--apply")
    run("publish", "release", "--origin", str(ORIGIN), "--version", "1.0.2", "--sequence", "3",
        "--key", str(WORK / "keys" / "publisher.key"), "--component", "runtime=1.0.2", "--apply")
    LOG.clear()
    out = run(*common, "--apply")
    must(out, "Live          runtime 1.0.2", "update went live")
    must(out, "Removed       1 file(s)", "the dropped file is reported")
    must(out, "  a.txt", "by name")
    assert not blob_gets(), f"nothing changed but the drop, so no blob was fetched: {blob_gets()}"
    assert not (DATA / "runtime" / "a.txt").exists(), "a.txt is gone from the live tree"
    assert (DATA / "versions" / "runtime" / "prior" / "a.txt").read_bytes() == v2["a.txt"], "and sits in prior, not deleted"
    for rel, data in v3.items():
        assert (DATA / "runtime" / rel).read_bytes() == data, f"{rel}: v3 bytes differ"
    step("a version that drops a file retires it into prior, fetches nothing, and names it")

    # 7. replay at the publisher
    out = run("publish", "release", "--origin", str(ORIGIN), "--version", "1.0.0", "--sequence", "1",
              "--key", str(WORK / "keys" / "publisher.key"), "--component", "runtime=1.0.0", "--apply", expect=1)
    must(out, "strictly higher sequence", "publisher refuses a replay")
    step("publisher refuses a release whose sequence is not strictly higher")

    # 8. tampered signature at the client
    sig_path = ORIGIN / "release.json.sig"
    good = sig_path.read_text()
    last = good.strip()[-1]
    bad = good.strip()[:-1] + ("0" if last != "0" else "1") + "\n"
    sig_path.write_text(bad)
    before = (DATA / "runtime" / "b.txt").read_bytes()
    r = subprocess.run([str(EXE), *common, "--apply"], capture_output=True, text=True, encoding="utf-8", errors="replace")
    out = r.stdout + r.stderr
    assert r.returncode != 0, "a tampered signature must not exit 0"
    must(out, "did not verify", "tampered signature is named")
    assert (DATA / "runtime" / "b.txt").read_bytes() == before, "nothing may change under a bad signature"
    sig_path.write_text(good)
    out = run(*common)
    must(out, "Up to date: runtime 1.0.2", "with the real signature back, the client is up to date")
    step("a tampered signature is refused and the live install is untouched")

    print()
    print(f"PASS: {len(PASSED)} steps, origin at {ORIGIN}")


if __name__ == "__main__":
    try:
        main()
    except SystemExit as e:
        print(e)
        sys.exit(1)
