#!/usr/bin/env bash
# Ship the Steam Deck launcher for a release, from Git Bash on the Windows
# publishing PC. One command, every step checked before the next:
#
#   installer/linux/release-deck.sh <launcher-version> <git-ref> [deck-version]
#
#   launcher-version  what live latest.json says (the Windows release), e.g. 3.0.36
#   git-ref           the launcher commit/branch that release was built from,
#                     with the Deck port in it (port/linux-deck-on-<version>)
#   deck-version      only for a Deck-only fix on top of that release, e.g.
#                     3.0.36.1; written as the linux asset's own "version"
#
# Why it exists: 3.0.33 went live with no linux entry (Decks were offered
# nothing), and a Deck AppImage stamped older than latest.json's version
# downloads itself again forever. And on 10-07 a manifest edit whose check
# failed still uploaded a stale latest.json for two minutes, because the
# upload was on the next line instead of behind &&. Here the manifest is
# built only from a fresh fetch, and uploaded only if every check passed.
#
# Steps: build in WSL (installer/linux/build-release.sh) -> full first-run
# test (e2e-first-run.sh, detached so a memory reaper on this side cannot
# kill it) -> stage + upload the AppImage + read its hash back -> fetch
# latest.json, assert, set only the linux entry, upload, compare live.
set -euo pipefail
VERSION=${1:?launcher version, e.g. 3.0.36}
REF=${2:?git ref with the Deck port}
DECK=${3:-$VERSION}
DISTRO=Ubuntu-24.04
STAGE="/c/Reborn Origin-stage/launcher-linux"
RCLONE=/c/Users/Adam/AppData/Local/Microsoft/WinGet/Packages/Rclone.Rclone_Microsoft.Winget.Source_8wekyb3d8bbwe/rclone-v1.75.1-windows-amd64/rclone
RCONF='C:\Users\Adam\Reborn Keys\rclone.conf'
WIN_RUNTIME='/mnt/c/Users/Adam/AppData/Roaming/NFL Street Reborn/app/runtime'
NAME="NFL Street Reborn $DECK.AppImage"
wsl() { MSYS_NO_PATHCONV=1 wsl.exe -d "$DISTRO" -u root -- "$@" 2>&1 | tr -d '\r\0' | grep -v 'Failed to translate\|systemd' || true; }

echo "== 1. live latest.json is $VERSION"
LIVE=$(curl -sf "https://files.nflstreetreborn.com/launcher/latest.json?x=$(date +%s)" | python -c "import json,sys;print(json.load(sys.stdin)['version'])")
[ "$LIVE" = "$VERSION" ] || { echo "live latest.json is $LIVE, not $VERSION: refusing"; exit 1; }

echo "== 2. build $DECK from $REF in WSL (detached), then the full first-run test"
JOB=$(mktemp); cat > "$JOB" <<EOF
#!/bin/bash
exec > /root/release-deck.log 2>&1
su - reborn -c 'cd ~/src/launcher && git checkout -q -- . && git fetch -q origin "$REF" && git checkout -q -B release-deck FETCH_HEAD && git log --oneline -1' || { echo FAILED-FETCH; exit 1; }
cp /home/reborn/src/launcher/installer/linux/*.sh /home/reborn/src/launcher/installer/linux/*.py /opt/reborn-build/ && chmod +x /opt/reborn-build/*.sh
REBORN_BUILD_VERSION="$DECK" /opt/reborn-build/build-release.sh '$WIN_RUNTIME' > /root/release.log 2>&1 || { echo FAILED-BUILD; tail -20 /root/release.log; exit 1; }
E2E_PHASE2=0 /opt/reborn-build/e2e-first-run.sh "/home/reborn/release/$DECK" /home/reborn/src/e2e/game.iso > /root/e2e-deck.log 2>&1
# Passed = reached the Play page, the game is the newest build, nothing failed.
if grep -aq '^=== reached the Play page' /home/reborn/src/e2e/out/steps.log    && grep -aq '^=== game image is the newest build' /home/reborn/src/e2e/out/steps.log    && ! grep -aq '^!!!' /home/reborn/src/e2e/out/steps.log; then echo PASSED
else echo FAILED-E2E; grep -aE '^!!!|^text:|^=== game image' /home/reborn/src/e2e/out/steps.log | tail -4; exit 1; fi
EOF
sed -i 's/\r$//' "$JOB"
wsl bash -c "cp '$(cygpath -u "$JOB" | sed 's|^/c/|/mnt/c/|')' /root/release-deck.sh && chmod +x /root/release-deck.sh && rm -f /root/release-deck.log && setsid nohup /root/release-deck.sh >/dev/null 2>&1 </dev/null &"
rm -f "$JOB"
# The wsl wrapper keeps no exit code, so the log is read, not grep's status.
until wsl cat /root/release-deck.log | grep -qE '^(PASSED|FAILED)'; do sleep 60; done
wsl cat /root/release-deck.log
wsl cat /root/release-deck.log | grep -q '^PASSED' || { echo "not shipped"; exit 1; }

echo "== 3. stage, upload, read back"
wsl cp "/home/reborn/release/$DECK/NFL Street Reborn $DECK.AppImage" "/mnt/c/Reborn Origin-stage/launcher-linux/$NAME"
wsl cp "/home/reborn/release/$DECK/BUILD-INFO.txt" "/mnt/c/Reborn Origin-stage/launcher-linux/BUILD-INFO-$DECK.txt"
H=$(sha256sum "$STAGE/$NAME" | cut -c1-64); B=$(stat -c %s "$STAGE/$NAME")
"$RCLONE" --config "$RCONF" copyto "$STAGE/$NAME" "r2:reborn-content/launcher/$NAME"
URLNAME=$(python -c "import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1]))" "$NAME")
SERVED=$(curl -sf "https://files.nflstreetreborn.com/launcher/$URLNAME?v=${H:0:8}" | sha256sum | cut -c1-64)
[ "$SERVED" = "$H" ] || { echo "served $SERVED, built $H: refusing to point latest.json at it"; exit 1; }

echo "== 4. latest.json: fresh fetch, only the linux entry changes, upload, compare"
cd "$STAGE"
T=$(date +%s)
curl -sf "https://files.nflstreetreborn.com/launcher/latest.json?nocache=$T" -o "manifest-backup/latest.json.before-$DECK-linux"
python - "$VERSION" "$DECK" "$URLNAME" "$H" "$B" "$NAME" <<'PY'
import json, sys
version, deck, urlname, sha, size, name = sys.argv[1:]
src = f"manifest-backup/latest.json.before-{deck}-linux"
m = json.load(open(src, encoding="utf-8"))
assert m["version"] == version, f"live is {m['version']}, not {version}: refusing"
before = {k: v for k, v in m.items() if k != "linux"}
asset = {"url": f"https://files.nflstreetreborn.com/launcher/{urlname}?v={sha[:8]}", "sha256": sha, "bytes": int(size), "file_name": name}
if deck != version:
    asset["version"] = deck
m["linux"] = asset
assert {k: v for k, v in m.items() if k != "linux"} == before
open("latest.linux.json", "w", encoding="utf-8", newline="\n").write(json.dumps(m, indent=2) + "\n")
print("edit ok:", m["version"], "| linux", name, asset.get("version", ""))
PY
"$RCLONE" --config "$RCONF" copyto latest.linux.json r2:reborn-content/launcher/latest.json
curl -sf "https://files.nflstreetreborn.com/launcher/latest.json?x=$T" | cmp - latest.linux.json
echo "SHIPPED: $NAME ($H) is live in latest.json"
