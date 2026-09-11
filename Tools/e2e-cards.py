"""End to end: a real card copy, the real Builder, a real roster with history, the real verb.

Skips (exit 0, says why) when any input is absent, like e2e-roster.py. Never touches the
real card: it copies the 2026-09-06 backup of Card B into a scratch user folder, and it
points the verb at a scratch data root, so nothing under the real data root moves.

    py -3 Tools/e2e-cards.py <path to a .rebornroster that carries history.json>

What it proves, in order: a dry run writes neither the card nor a recovery copy; a full
run bootstraps every profile from the history, writes the state file and one recovery
copy of the untouched card; a second full run finds the card up to date without starting
the Builder.
"""
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

APPDATA = Path(os.environ["APPDATA"])
BUILDER = APPDATA / "NFL Street Reborn" / "Tools" / "reborn-builder-cli.exe"
CARD = APPDATA / "NFL Street Reborn" / "User" / "GC" / "MemoryCardB.USA.raw.bak_pre_gearsync_probe_20260906_141745"
CLI = Path(__file__).resolve().parent.parent / "target" / "debug" / "rebornctl.exe"


def skip(why):
    print("SKIP:", why)
    sys.exit(0)


def run(args):
    r = subprocess.run(args, capture_output=True, text=True)
    print(r.stdout)
    if r.stderr.strip():
        print(r.stderr)
    return r


def main():
    if len(sys.argv) != 2:
        skip("no roster file given")
    roster = Path(sys.argv[1])
    for p, what in [
        (BUILDER, "builder"),
        (CARD, "card backup"),
        (CLI, "rebornctl (cargo build -p reborn-cli)"),
        (roster, "roster"),
    ]:
        if not p.is_file():
            skip("%s missing: %s" % (what, p))
    if "history.json" not in zipfile.ZipFile(roster).namelist():
        skip("that roster carries no history.json; export one after the site change is deployed")
    with tempfile.TemporaryDirectory() as td:
        root = Path(td) / "data"
        user = Path(td) / "User"
        (user / "GC").mkdir(parents=True)
        card = user / "GC" / "MemoryCardB.USA.raw"
        shutil.copy2(CARD, card)
        before = card.read_bytes()
        common = [
            str(CLI), "cards", "sync",
            "--data-root", str(root),
            "--roster", str(roster),
            "--builder", str(BUILDER),
            "--user-dir", str(user),
        ]

        r = run(common + ["--dry-run"])
        assert r.returncode == 0, "dry run exit %d" % r.returncode
        assert card.read_bytes() == before, "dry run wrote the card"
        assert not (root / "recovery").exists(), "dry run took a recovery copy"
        assert "would change" in r.stdout or "left as they were" in r.stdout, "dry run said nothing about the card"

        r = run(common)
        assert r.returncode == 0, "full run exit %d" % r.returncode
        state = root / "library" / "roster" / "cards" / "MemoryCardB.USA.raw.json"
        assert state.is_file(), "no state file"
        copies = list((root / "recovery" / "cards").glob("MemoryCardB.USA.raw.*.bak"))
        assert len(copies) == 1 and copies[0].read_bytes() == before, "no recovery copy of the untouched card"
        after = card.read_bytes()
        assert after != before, "the full run changed nothing on a 2.1-era card; the history did not reach the rows"
        assert len(after) == len(before), "the card changed size"

        r2 = run(common)
        assert r2.returncode == 0, "second run exit %d" % r2.returncode
        assert "up to date" in r2.stdout, "the second run did not find the card up to date"
        assert card.read_bytes() == after, "the second run moved the card"
        copies = list((root / "recovery" / "cards").glob("MemoryCardB.USA.raw.*.bak"))
        assert len(copies) == 1, "an up-to-date card took another recovery copy"
        print("OK")


if __name__ == "__main__":
    main()
