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
"$HERE/in-builder.sh" bash -uc '
export HOME=/src/e2e/home XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
export DISPLAY=:51 GDK_BACKEND=x11
export APPDIR=/src/e2e/app/squashfs-root APPIMAGE=/src/e2e/NFL-Street-Reborn.AppImage REBORN_E2E_PICK_GAME=/src/e2e/game.iso
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
