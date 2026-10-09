#!/usr/bin/env bash
# Run e2e_first_run.py against a release's launcher in the jammy build root:
# fresh HOME, headless X, tauri-driver, the AppImage's own tools via APPDIR,
# the live content server. Run as root.
#   e2e-first-run.sh <release-folder> <game-file>
set -uo pipefail
REL=${1:?release folder}; GAME=${2:?game file}
HERE=$(cd "$(dirname "$0")" && pwd)
W=/home/reborn/src/e2e
rm -rf "$W/home" "$W/app" "$W/out"; mkdir -p "$W/home" "$W/app" "$W/out"
(cd "$W/app" && cp "$REL"/*.AppImage a.AppImage && ./a.AppImage --appimage-extract > /dev/null && rm a.AppImage)
[ -f "$W/game.iso" ] || cp "$GAME" "$W/game.iso"
cp "$HERE/e2e_first_run.py" "$W/"
# Phase 2 downloads the 8 GB content into this distro's disk, which grows the
# Windows vhdx: on 10-03 a run filled C: to 0.18 GB and WSL remounted its disk
# read-only. Run it only with room to spare; the flag goes through a file
# because in-builder.sh does not pass the environment into the build root.
PHASE2=${E2E_PHASE2:-1}
FREE_GB=$(df -BG --output=avail /mnt/c 2>/dev/null | tail -1 | tr -dc 0-9)
if [ "$PHASE2" != 0 ] && [ "${FREE_GB:-0}" -lt 25 ]; then
  echo "only ${FREE_GB} GB free on C: (phase 2 needs 25): stopping at the Play page"; PHASE2=0
fi
echo "$PHASE2" > "$W/phase2"
"$HERE/in-builder.sh" bash -uc '
export HOME=/src/e2e/home XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
# Leftovers from a run that was stopped (a driver still on :4444, a stale
# X lock for :51) failed the next run with "Connection refused" before it
# began (3.0.38, 2026-10-09). Cleared first, by EXACT name: "pkill -f" matched
# the command line of this very script and killed it.
pkill -x tauri-driver; pkill -x WebKitWebDriver; pkill -x reborn-launcher; pkill -x Xvfb; sleep 1
rm -f /tmp/.X51-lock /tmp/.X11-unix/X51
export DISPLAY=:51 GDK_BACKEND=x11
export APPDIR=/src/e2e/app/squashfs-root APPIMAGE=/src/e2e/NFL-Street-Reborn.AppImage REBORN_E2E_PICK_GAME=/src/e2e/game.iso E2E_PHASE2=$(cat /src/e2e/phase2)
Xvfb :51 -screen 0 1280x800x24 -nolisten tcp > /src/e2e/out/xvfb.log 2>&1 &
sleep 2
dbus-launch --exit-with-session > /dev/null 2>&1 || true
/root/.cargo/bin/tauri-driver --native-driver /usr/bin/WebKitWebDriver > /src/e2e/out/tauri-driver.log 2>&1 &
sleep 3
python3 /src/e2e/e2e_first_run.py $APPDIR/usr/bin/reborn-launcher /src/e2e/game.iso /src/e2e/out
pkill -f tauri-driver; pkill -f WebKitWebDriver; pkill -f reborn-launcher; pkill -x Xvfb; true
'
pkill -x Xvfb; pkill -f tauri-driver; true
echo "--- launcher.log:"; cat "$W/home/.local/share/NFL Street Reborn/logs/launcher.log" 2>/dev/null | tail -20
chown -R reborn:reborn "$W"
echo E2E-DONE
