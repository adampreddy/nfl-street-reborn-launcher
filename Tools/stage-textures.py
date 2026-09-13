r"""Stage the live texture folder for publishing as a textures component.

    py -3 Tools\stage-textures.py 1.2.0
    -> C:\Reborn Origin-stage\textures-1.2.0\GN7E69

The live folder Dolphin reads (User\Load\Textures\GN7E69) also collects
things that are not textures: a texconv log, .blanked_off backups. Textures
1.0.0 shipped the log to every player. Publishing from a stage without them
costs nothing on the player side: a file a version drops is retired, never
downloaded. Hardlinks, so 6 GB takes seconds and no disk; the Publisher only
reads the tree. The stage is rebuilt from scratch each run.

Point the Reborn Publisher's Content page at the folder this prints.
"""

import os
import shutil
import sys

SRC = os.path.join(os.environ["APPDATA"], "NFL Street Reborn", "User", "Load", "Textures", "GN7E69")
SKIP_SUFFIXES = (".log", ".blanked_off", ".txt", ".bak", ".tmp")

if len(sys.argv) != 2:
    sys.exit("usage: stage-textures.py <version>   e.g. 1.2.0")
version = sys.argv[1]
stage = os.path.join(r"C:\Reborn Origin-stage", f"textures-{version}")
dst = os.path.join(stage, "GN7E69")

if not os.path.isdir(SRC):
    sys.exit(f"no live texture folder at {SRC}")
if os.path.exists(stage):
    shutil.rmtree(stage)

linked = copied = 0
skipped = []
for root, dirs, files in os.walk(SRC):
    rel = os.path.relpath(root, SRC)
    out = dst if rel == "." else os.path.join(dst, rel)
    os.makedirs(out, exist_ok=True)
    for name in files:
        src = os.path.join(root, name)
        if name.lower().endswith(SKIP_SUFFIXES):
            skipped.append(name if rel == "." else os.path.join(rel, name))
            continue
        try:
            os.link(src, os.path.join(out, name))
            linked += 1
        except OSError:
            shutil.copy2(src, os.path.join(out, name))
            copied += 1

print(f"staged {linked + copied:,} files into {dst}")
if skipped:
    print("left out (not textures):")
    for s in skipped:
        print("  ", s)
print()
print(f"In the Reborn Publisher: Content -> Textures -> folder {dst} -> version {version} -> Plan, Run, Verify.")
