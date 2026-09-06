"""End to end: a roster applied to the real game image by the real Builder.

    py -3 Tools/e2e-roster.py        (needs target/debug/rebornctl.exe)

Uses the real inputs on this machine and a scratch data root that gets a
COPY of the image, so nothing real is written. Exits 0 with SKIP when an
input is absent, because this proof is machine-specific by nature.

What is proven:
  1 the roster applies: exit 0, the image hash changes, the rebuild is
    recorded so verification will keep the image rather than re-download it
  2 applying the same roster again is reported Unchanged and swaps nothing
  3 a missing Builder is refused up front and the image is untouched
"""
import hashlib
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
# REBORNCTL=path overrides, so the timing can be taken on a release build:
# hashing a 1.46 GB image twice is what a debug build spends most of its
# time on, and that number is not the one a player sees.
EXE = Path(os.environ.get("REBORNCTL") or (REPO / "target" / "debug" / "rebornctl.exe"))
WORK = REPO / "target" / "e2e-roster"

APPDATA = Path(os.environ.get("APPDATA", ""))
ISO = APPDATA / "NFL Street Reborn" / "Games" / "NFL Street 2 FIELDS 12-19.iso"
BUILDER = APPDATA / "NFL Street Reborn" / "Tools" / "reborn-builder-cli.exe"
ROSTERS = sorted((Path.home() / "Downloads").glob("reborn-*.rebornroster"), key=lambda p: p.stat().st_size)


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


def main():
    for what, p in (("rebornctl", EXE), ("game image", ISO), ("Reborn Builder", BUILDER)):
        if not p.is_file():
            print(f"SKIP: no {what} at {p}")
            return
    if not ROSTERS:
        print("SKIP: no .rebornroster in Downloads")
        return
    if WORK.exists():
        shutil.rmtree(WORK)
    data = WORK / "data"
    offline = data / "library" / "offline"
    offline.mkdir(parents=True)
    t = time.time()
    shutil.copyfile(ISO, offline / "NSR.iso")
    print(f"  copied the image into a scratch data root in {time.time() - t:.1f} s")
    before = sha256(offline / "NSR.iso")

    # 1. apply. Newest roster first: an older export can name a player this
    # image does not have, and the Builder refuses that, correctly, with the
    # player's id. That refusal is relayed by the launcher and is worth seeing
    # once, but it is not what this proves, so fall through to the next.
    roster = None
    out = ""
    took = 0.0
    for candidate in sorted(ROSTERS, key=lambda p: p.name, reverse=True):
        args = ["roster", "apply", "--data-root", str(data), "--roster", str(candidate),
                "--builder", str(BUILDER), "--yes"]
        t = time.time()
        r = subprocess.run([str(EXE), *args], capture_output=True, text=True, encoding="utf-8", errors="replace")
        took = time.time() - t
        out = r.stdout + r.stderr
        if r.returncode == 0:
            roster = candidate
            break
        if "Match roster" in out:
            reason = next((l.strip() for l in out.splitlines() if "Stopped at Match roster" in l), "refused")
            print(f"  ..  {candidate.name} does not match this image ({reason}); trying an older one")
            continue
        print(out)
        raise SystemExit(f"FAIL: {candidate.name} was refused for a reason other than a roster mismatch")
    if roster is None:
        raise SystemExit("FAIL: no roster in Downloads matches this image")
    common = ["roster", "apply", "--data-root", str(data), "--roster", str(roster), "--builder", str(BUILDER), "--yes"]
    must(out, "Applied       ", "the roster applied")
    must(out, "Recorded as a rebuilt image", "the rebuild is recorded")
    after = sha256(offline / "NSR.iso")
    assert after != before, "the image must change"
    assert (offline / "NSR.iso").stat().st_size == ISO.stat().st_size, "a range patch never changes the length"
    state = (data / "music" / "state.json").read_text(encoding="utf-8")
    assert "NSR.iso" in state and after in state, f"state.json must record the new hash: {state}"
    print(f"  ok  applied {roster.name} in {took:.1f} s (Builder included); image hash changed; rebuild recorded")

    # 2. same roster again
    out = run(*common)
    must(out, "Unchanged:", "a second apply of the same roster is a no-op")
    assert sha256(offline / "NSR.iso") == after, "nothing may change on an unchanged apply"
    print("  ok  applying the same roster again is Unchanged and swaps nothing")

    # 3. missing builder
    out = run("roster", "apply", "--data-root", str(data), "--roster", str(roster),
              "--builder", str(WORK / "no-such-builder.exe"), "--yes", expect=1)
    must(out, "Refused:", "a missing Builder is refused")
    must(out, "as it was", "and says the game is as it was")
    assert sha256(offline / "NSR.iso") == after
    print("  ok  a missing Builder is refused up front and the image is untouched")

    print()
    print(f"PASS: 3 steps against {ISO.name} with {BUILDER.name}")


if __name__ == "__main__":
    try:
        main()
    except SystemExit as e:
        if e.code not in (0, None):
            print(e)
            sys.exit(1)
