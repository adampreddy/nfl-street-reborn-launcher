#!/usr/bin/env bash
# The Steam Deck launcher, at the same version as Windows, from the same
# commit. Git Bash on the publishing PC:
#
#   installer/linux/release-deck.sh build <version> [git-ref]
#       Build in WSL, run the full first-run test (the player's game must end
#       on the newest build), and stage the AppImage + BUILD-INFO where
#       installer/windows/build.ps1 requires them. Run this BEFORE the Windows
#       launcher build of the same version: a stable build.ps1 refuses to
#       write latest.json without it, and the Publisher refuses to upload
#       without it. git-ref defaults to this repo's HEAD, which is the commit
#       build.ps1 checks the Deck build against.
#
#   installer/linux/release-deck.sh ship <version> [git-ref]
#       The same, then upload it and add it to a latest.json that is ALREADY
#       live at <version> (a release that went out without its Deck build).
#
# Every edition runs one version: there is no Deck-only version number.
# Why it exists: 3.0.33 went live with no linux entry (Decks were offered
# nothing), and a Deck AppImage stamped older than latest.json's version
# downloads itself again forever. On 10-07 a manifest edit whose check had
# failed still uploaded a stale latest.json for two minutes, because the
# upload was on the next line instead of behind &&; here every step gates
# the next.
set -euo pipefail
MODE=${1:?build or ship}
VERSION=${2:?launcher version, e.g. 3.0.36}
REF=${3:-$(git -C "$(dirname "$0")/../.." rev-parse HEAD)}
DECK=$VERSION
case "$MODE" in build|ship) ;; *) echo "mode must be build or ship"; exit 1;; esac
DISTRO=Ubuntu-24.04
STAGE="/c/Reborn Origin-stage/launcher-linux"
RCLONE=/c/Users/Adam/AppData/Local/Microsoft/WinGet/Packages/Rclone.Rclone_Microsoft.Winget.Source_8wekyb3d8bbwe/rclone-v1.75.1-windows-amd64/rclone
RCONF='C:\Users\Adam\Reborn Keys\rclone.conf'
WIN_RUNTIME='/mnt/c/Users/Adam/AppData/Roaming/NFL Street Reborn/app/runtime'
NAME="NFL Street Reborn $DECK.AppImage"
wsl() { MSYS_NO_PATHCONV=1 wsl.exe -d "$DISTRO" -u root -- "$@" 2>&1 | tr -d '\r\0' | grep -v 'Failed to translate\|systemd' || true; }

if [ "$MODE" = ship ]; then
  echo "== 1. live latest.json is $VERSION"
  LIVE=$(curl -sf "https://files.nflstreetreborn.com/launcher/latest.json?x=$(date +%s)" | python -c "import json,sys;print(json.load(sys.stdin)['version'])")
  [ "$LIVE" = "$VERSION" ] || { echo "live latest.json is $LIVE, not $VERSION: refusing"; exit 1; }
fi

echo "== 2. build $DECK from $REF in WSL (detached), then the full first-run test"
# The job file sits on C: at a fixed place WSL reaches as /mnt/c/... (a Git
# Bash mktemp path under /tmp has no /mnt/c twin, and the job silently never
# started on the first 3.0.37 run).
JOB="$STAGE/release-deck-job.sh"; cat > "$JOB" <<EOF
#!/bin/bash
exec > /root/release-deck.log 2>&1
su - reborn -c 'cd ~/src/launcher && git checkout -q -- . && git fetch -q origin "+refs/heads/*:refs/remotes/origin/*" && git checkout -q -B release-deck "$REF" && git log --oneline -1' || { echo FAILED-FETCH; exit 1; }
cp /home/reborn/src/launcher/installer/linux/*.sh /home/reborn/src/launcher/installer/linux/*.py /opt/reborn-build/ && chmod +x /opt/reborn-build/*.sh
REBORN_BUILD_VERSION="$DECK" /opt/reborn-build/build-release.sh '$WIN_RUNTIME' > /root/release.log 2>&1 || { echo FAILED-BUILD; tail -20 /root/release.log; exit 1; }
E2E_PHASE2=0 /opt/reborn-build/e2e-first-run.sh "/home/reborn/release/$DECK" /home/reborn/src/e2e/game.iso > /root/e2e-deck.log 2>&1
# Passed = reached the Play page, the game is the newest build, nothing failed.
if grep -aq '^=== reached the Play page' /home/reborn/src/e2e/out/steps.log    && grep -aq '^=== game image is the newest build' /home/reborn/src/e2e/out/steps.log    && ! grep -aq '^!!!' /home/reborn/src/e2e/out/steps.log; then echo PASSED
else echo FAILED-E2E; grep -aE '^!!!|^text:|^=== game image' /home/reborn/src/e2e/out/steps.log | tail -4; exit 1; fi
EOF
sed -i 's/\r$//' "$JOB"
# Only the job goes to the background: "a && b && c &" backgrounds the whole
# chain, and WSL ended the session before the copy even ran. The sleep keeps
# the session open while setsid detaches the job.
wsl -u root bash -c "rm -f /root/release-deck.log && cp '/mnt/c/Reborn Origin-stage/launcher-linux/release-deck-job.sh' /root/release-deck.sh && chmod +x /root/release-deck.sh && { setsid nohup /root/release-deck.sh >/dev/null 2>&1 </dev/null & } ; sleep 3"
rm -f "$JOB"
# Started = the job's log exists (cat's own "No such file" is not output).
wsl -u root bash -c "test -e /root/release-deck.log && echo JOB-STARTED" | grep -q JOB-STARTED || { echo "the WSL job did not start: refusing"; exit 1; }
# The wsl wrapper keeps no exit code, so the log is read, not grep's status.
until wsl -u root cat /root/release-deck.log | grep -qE '^(PASSED|FAILED)'; do sleep 60; done
wsl -u root cat /root/release-deck.log
wsl -u root cat /root/release-deck.log | grep -q '^PASSED' || { echo "not shipped"; exit 1; }

echo "== 3. stage"
wsl -u root cp "/home/reborn/release/$DECK/NFL Street Reborn $DECK.AppImage" "/mnt/c/Reborn Origin-stage/launcher-linux/$NAME"
wsl -u root cp "/home/reborn/release/$DECK/BUILD-INFO.txt" "/mnt/c/Reborn Origin-stage/launcher-linux/BUILD-INFO-$DECK.txt"
grep -q "launcher commit   $(git -C "$(dirname "$0")/../.." rev-parse "$REF")" "$STAGE/BUILD-INFO-$DECK.txt" || { echo "the staged build is not from $REF: refusing"; exit 1; }
if [ "$MODE" = build ]; then
  echo "STAGED: $NAME from $REF. Now build the Windows launcher $VERSION from the same commit (Publisher > Launcher)."
  exit 0
fi
echo "== 3b. upload, read back"
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
m["linux"] = asset
assert {k: v for k, v in m.items() if k != "linux"} == before
open("latest.linux.json", "w", encoding="utf-8", newline="\n").write(json.dumps(m, indent=2) + "\n")
print("edit ok:", m["version"], "| linux", name, asset.get("version", ""))
PY
"$RCLONE" --config "$RCONF" copyto latest.linux.json r2:reborn-content/launcher/latest.json
curl -sf "https://files.nflstreetreborn.com/launcher/latest.json?x=$T" | cmp - latest.linux.json
echo "SHIPPED: $NAME ($H) is live in latest.json"
