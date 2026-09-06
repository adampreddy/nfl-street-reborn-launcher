"""End to end: a game update as a delta chain, over the real image with the real xdelta3.

    py -3 Tools/e2e-game-patch.py        (needs target/release/rebornctl.exe; REBORNCTL=path overrides)

Uses the release candidate image on this machine as "build A" and makes
builds B, C and D from it by changing a few bytes, so the deltas are small
and real. Everything runs against a scratch data root; the real image is
only read. Exits 0 with SKIP when an input is absent.

What is proven, in order:
  1 publish game-patch encodes A->B, verifies it, appends it to the chain,
    publishes the component and re-signs the release
  2 a client at A takes the component and walks the chain: pristine becomes
    B under its own hash, the booted image is B, the old folder is gone
  3 a second publish (B->C) is one more blob; the client fetches only that
    one and walks one step
  4 a fresh client still at A walks both steps in one update
  5 the pristine image stays pristine under a roster: the roster goes into
    the booted copy, the pristine hash does not move (needs the Builder)
  6 a later game update (C->D) rebuilds the booted image AND re-applies the
    roster to it (needs the Builder)
  7 a client whose image is not in the chain is refused and untouched
"""
import hashlib
import http.server
import os
import re
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
_release = REPO / "target" / "release" / "rebornctl.exe"
EXE = Path(os.environ.get("REBORNCTL") or (_release if _release.is_file() else REPO / "target" / "debug" / "rebornctl.exe"))
XDELTA = REPO / "target" / "installer-payload" / "game-import" / "xdelta3.exe"
WORK = REPO / "target" / "e2e-game-patch"
ORIGIN = WORK / "origin"
TREE = WORK / "tree"
LOG = []
PASSED = []

APPDATA = Path(os.environ.get("APPDATA", ""))
ISO = APPDATA / "NFL Street Reborn" / "Games" / "NFL Street 2 FIELDS 12-19.iso"
BUILDER = APPDATA / "NFL Street Reborn" / "Tools" / "reborn-builder-cli.exe"
ROSTERS = sorted((Path.home() / "Downloads").glob("reborn-*.rebornroster"), key=lambda p: p.stat().st_mtime, reverse=True)


def step(name):
    PASSED.append(name)
    print(f"  ok  {name}")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


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


def derive(src, dst, offset, marker):
    """A new build: the same image with a few bytes changed at `offset`."""
    shutil.copyfile(src, dst)
    with open(dst, "r+b") as f:
        f.seek(offset)
        f.write(marker)


def fresh_client(name, pristine_iso, pristine_sha):
    """A data root holding one pristine image and a booted link to it, the
    way the launcher's import leaves it."""
    data = WORK / name
    if data.exists():
        shutil.rmtree(data)
    folder = data / "library" / "imported" / pristine_sha
    folder.mkdir(parents=True)
    # A copy, never a link to the input: the real image on this machine is
    # only ever read by this script. The booted image below IS a link, like
    # the import leaves it.
    shutil.copyfile(pristine_iso, folder / "NSR.iso")
    (data / "library" / "offline").mkdir(parents=True)
    os.link(folder / "NSR.iso", data / "library" / "offline" / "NSR.iso")
    return data


def pristine_of(data):
    folders = [p for p in (data / "library" / "imported").iterdir() if p.is_dir() and (p / "NSR.iso").is_file()]
    assert len(folders) == 1, f"expected exactly one pristine folder, found {[f.name for f in folders]}"
    return folders[0]


def main():
    for what, p in (("rebornctl", EXE), ("game image", ISO), ("xdelta3", XDELTA)):
        if not p.is_file():
            print(f"SKIP: no {what} at {p}")
            return
    have_builder = BUILDER.is_file() and bool(ROSTERS)
    print(f"  rebornctl: {EXE}")
    print(f"  image:     {ISO.name} ({ISO.stat().st_size:,} bytes)")
    print(f"  Builder:   {'yes' if have_builder else 'absent, steps 5 and 6 will be skipped'}")

    if WORK.exists():
        shutil.rmtree(WORK)
    WORK.mkdir(parents=True)

    t0 = time.time()
    sha_a = sha256(ISO)
    build_b = WORK / "build-b.iso"
    build_c = WORK / "build-c.iso"
    build_d = WORK / "build-d.iso"
    derive(ISO, build_b, 100 * 1024 * 1024, b"REBORN BUILD B, A FIX IN THE FIELD DATA")
    derive(build_b, build_c, 700 * 1024 * 1024, b"REBORN BUILD C, ANOTHER FIX")
    derive(build_c, build_d, 1200 * 1024 * 1024, b"REBORN BUILD D, AND ONE MORE")
    sha_b, sha_c, sha_d = sha256(build_b), sha256(build_c), sha256(build_d)
    print(f"  builds:    A {sha_a[:12]}  B {sha_b[:12]}  C {sha_c[:12]}  D {sha_d[:12]}  ({time.time() - t0:.0f} s to derive and hash)")

    # keys and an origin with a runtime, so the game patch has something to carry forward
    out = run("publish", "keygen", "--out", str(WORK / "keys"))
    pub_hex = re.search(r"// everyday\s+([0-9a-f]{64})", out).group(1)
    key = str(WORK / "keys" / "publisher.key")
    tree1 = WORK / "runtime-tree"
    tree1.mkdir()
    (tree1 / "a.txt").write_bytes(b"alpha\n")
    run("publish", "component", "--root", str(tree1), "--component", "runtime", "--version", "1.0.0",
        "--sequence", "1", "--origin", str(ORIGIN), "--apply")
    run("publish", "release", "--origin", str(ORIGIN), "--version", "3.0.0", "--sequence", "1", "--key", key,
        "--component", "runtime=1.0.0", "--apply")

    # 1. publish A -> B
    t1 = time.time()
    out = run("publish", "game-patch", "--from", str(ISO), "--to", str(build_b), "--version", "2026.09.07",
              "--tree", str(TREE), "--xdelta", str(XDELTA), "--origin", str(ORIGIN), "--key", key, "--apply")
    must(out, "Verified", "the delta was decoded back and hashed before publishing")
    must(out, "Chain         now 1 step(s)", "the chain has one step")
    must(out, "pinning game-patch=2026.09.07, runtime=1.0.0", "the runtime pin is carried forward")
    must(out, "Read back and verified", "the new release verifies")
    delta1 = re.search(r"Delta\s+([\d,]+) bytes in ([\d.]+) s", out)
    assert delta1, out
    delta1_bytes = int(delta1.group(1).replace(",", ""))
    assert delta1_bytes < 5 * 1024 * 1024, f"a few changed bytes should be a small delta, got {delta1_bytes:,}"
    assert (TREE / "chain.json").is_file() and (ORIGIN / "components" / "game-patch-2026.09.07.json").is_file()
    step(f"publish game-patch: A->B delta is {delta1_bytes:,} bytes, verified, chained, released ({time.time() - t1:.0f} s)")

    base = serve()
    common = ["update", "content", "--origin", base, "--component", "game-patch", "--trust", pub_hex, "--xdelta", str(XDELTA)]

    # 2. a client at A walks to B
    data = fresh_client("client-a", ISO, sha_a)
    t2 = time.time()
    out = run(*common, "--data-root", str(data), "--apply")
    must(out, "Live          game-patch 2026.09.07", "the component went live")
    must(out, "Patched       1 step(s): 2026.09.07", "one step was applied")
    must(out, "No roster is installed, so there was nothing to apply", "an install with no roster ends there, and that is fine")
    must(out, "(linked, no extra disk)", "the booted image is a link to the new pristine one")
    assert pristine_of(data).name == sha_b, "the pristine folder is named by the new hash"
    assert sha256(pristine_of(data) / "NSR.iso") == sha_b
    assert sha256(data / "library" / "offline" / "NSR.iso") == sha_b, "the booted image is build B"
    assert not (data / "library" / "imported" / sha_a).exists(), "the old pristine folder is retired"
    staging = data / "staging" / "game-patch"
    assert not staging.exists() or not any(staging.rglob("*.iso")), "no intermediate image left in staging"
    out = run(*common, "--data-root", str(data), "--apply")
    must(out, "Up to date: game-patch 2026.09.07", "nothing to fetch on a second run")
    step(f"client at A walked to B: pristine and booted are B, old folder gone ({time.time() - t2:.0f} s)")

    # 3. publish B -> C; the client at B fetches one blob and walks one step
    out = run("publish", "game-patch", "--from", str(build_b), "--to", str(build_c), "--version", "2026.09.08",
              "--tree", str(TREE), "--xdelta", str(XDELTA), "--origin", str(ORIGIN), "--key", key, "--apply")
    must(out, "Chain         now 2 step(s)", "the chain grew by one")
    must(out, "Sequence      2", "the component sequence advanced")
    LOG.clear()
    out = run(*common, "--data-root", str(data), "--apply")
    must(out, "Fetch         2 file(s)", "chain.json and the one new delta")
    must(out, "Patched       1 step(s): 2026.09.08", "only the new step was applied")
    gets = [e[0] for e in blob_gets()]
    assert len(gets) == 2, f"expected exactly two blob fetches (chain.json and one delta), got {gets}"
    assert sha256(data / "library" / "offline" / "NSR.iso") == sha_c
    assert pristine_of(data).name == sha_c
    step("a second game patch is one new blob, and the client walks exactly one step")

    # 4. a fresh client still at A walks two steps in one update
    data_a = fresh_client("client-a2", ISO, sha_a)
    out = run(*common, "--data-root", str(data_a), "--apply")
    must(out, "Patched       2 step(s): 2026.09.07, 2026.09.08", "both steps in order")
    assert sha256(data_a / "library" / "offline" / "NSR.iso") == sha_c
    assert pristine_of(data_a).name == sha_c
    step("a client still at A walks both steps in one update")

    # 7 (early, needs no Builder). a client whose image is not in the chain
    stranger = WORK / "stranger.iso"
    derive(ISO, stranger, 50 * 1024 * 1024, b"SOMEBODY ELSES BUILD")
    sha_s = sha256(stranger)
    data_s = fresh_client("client-stranger", stranger, sha_s)
    r = subprocess.run([str(EXE), *common, "--data-root", str(data_s), "--apply"],
                       capture_output=True, text=True, encoding="utf-8", errors="replace")
    out = r.stdout + r.stderr
    must(out, "Live          game-patch", "the component itself lands")
    must(out, "not a build this update knows", "the refusal says why")
    assert r.returncode == 1
    assert pristine_of(data_s).name == sha_s and sha256(data_s / "library" / "offline" / "NSR.iso") == sha_s
    step("an image the chain does not know is refused by name and untouched")

    if not have_builder:
        print()
        print(f"PASS: {len(PASSED)} steps (Builder absent, roster layering not exercised)")
        return

    # 5. a roster goes into the booted copy; the pristine hash does not move.
    # The roster is installed where the roster component puts it, so the
    # pipeline finds it again after the next game update.
    roster = None
    installed = data / "library" / "roster" / "roster.rebornroster"
    installed.parent.mkdir(parents=True, exist_ok=True)
    for candidate in ROSTERS:
        shutil.copyfile(candidate, installed)
        r = subprocess.run([str(EXE), "roster", "apply", "--data-root", str(data),
                            "--builder", str(BUILDER), "--yes"], capture_output=True, text=True, encoding="utf-8", errors="replace")
        if r.returncode == 0:
            roster = candidate
            out = r.stdout + r.stderr
            break
    assert roster, "no roster in Downloads applied cleanly: " + (r.stdout + r.stderr)
    must(out, "Applied", "the Builder ran")
    booted_with_roster = sha256(data / "library" / "offline" / "NSR.iso")
    assert booted_with_roster != sha_c, "the booted image now carries the roster"
    assert sha256(pristine_of(data) / "NSR.iso") == sha_c, "the pristine image did not move"
    assert pristine_of(data).name == sha_c
    step(f"roster {roster.name} went into the booted copy; the pristine hash did not move")

    # 6. C -> D rebuilds the booted image and re-applies the roster
    out = run("publish", "game-patch", "--from", str(build_c), "--to", str(build_d), "--version", "2026.09.09",
              "--tree", str(TREE), "--xdelta", str(XDELTA), "--origin", str(ORIGIN), "--key", key, "--apply")
    must(out, "Chain         now 3 step(s)", "the chain grew by one")
    t6 = time.time()
    out = run(*common, "--data-root", str(data), "--builder", str(BUILDER), "--apply")
    must(out, "Patched       1 step(s): 2026.09.09", "one step")
    must(out, "Applying the roster to the game image", "the roster was re-applied")
    must(out, "Applied", "and the Builder ran")
    assert sha256(pristine_of(data) / "NSR.iso") == sha_d and pristine_of(data).name == sha_d
    booted = sha256(data / "library" / "offline" / "NSR.iso")
    assert booted not in (sha_d, booted_with_roster), "the booted image is D plus the roster, a new file"
    # The state record names the booted image's hash, so verification keeps it.
    state = (data / "music" / "state.json").read_text(encoding="utf-8")
    assert booted in state, state
    step(f"game update C->D rebuilt the booted image and re-applied the roster ({time.time() - t6:.0f} s)")

    print()
    print(f"PASS: {len(PASSED)} steps, origin at {ORIGIN}")


if __name__ == "__main__":
    try:
        main()
    except SystemExit as e:
        if e.code:
            print(e)
            sys.exit(1)
        raise
