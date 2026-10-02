#!/usr/bin/env bash
# Does a release start on a system with none of the build machine's
# libraries? Runs it inside the clean Arch root (SteamOS's base) from
# setup-arch-check.sh: library resolution, the emulator booting the game,
# and the launcher window. Run as root.
#   check-on-arch.sh <release-folder> <NSR.iso>
set -uo pipefail
REL=${1:?release folder}; ISO=${2:?game image}
A=/srv/arch
rm -rf "$A/opt/reborn"; mkdir -p "$A/opt/reborn" "$A/opt/game"
cp -r "$REL/runtime-linux" "$A/opt/reborn/runtime"
cp "$REL"/*.AppImage "$A/opt/reborn/launcher.AppImage"
cp "$ISO" "$A/opt/game/NSR.iso"
mkdir -p "$A/opt/reborn/shots"
mount --bind "$A" "$A"
trap 'umount -lf "$A/proc" "$A/dev" "$A/sys" "$A" 2>/dev/null || true' EXIT
mount -t proc proc "$A/proc"
mount --rbind /dev "$A/dev"; mount --make-rslave "$A/dev"
mount --rbind /sys "$A/sys"; mount --make-rslave "$A/sys"
chroot "$A" /usr/bin/env -i HOME=/root PATH=/usr/bin LANG=C.UTF-8 /usr/bin/bash -uc '
R=/opt/reborn/runtime; S=/opt/reborn/shots
echo "== glibc here: $(ldd --version | head -1)"
echo "== unresolved libraries, emulator:"; ldd $R/dolphin-emu | grep "not found" || echo "   none"
for p in $R/plugins/platforms/*.so; do ldd "$p" | grep -q "not found" && echo "   plugin $(basename $p): $(ldd $p | grep "not found" | head -2 | tr -s " ")"; done
cd /opt/reborn && rm -rf squashfs-root && ./launcher.AppImage --appimage-extract > /dev/null
echo "== unresolved libraries, launcher:"; ldd squashfs-root/usr/bin/reborn-launcher | grep "not found" || echo "   none"
for t in DolphinTool xdelta3; do echo "== $t:"; ldd squashfs-root/usr/bin/$t | grep "not found" || echo "   none"; done
echo "== DolphinTool header:"; squashfs-root/usr/bin/DolphinTool header -i /opt/game/NSR.iso -j; echo
Xvfb :77 -screen 0 1280x800x24 -nolisten tcp > /dev/null 2>&1 & X=$!; sleep 2
export DISPLAY=:77 QT_QPA_PLATFORM=xcb
mkdir -p /root/u && cp -r $R/User/. /root/u/
$R/dolphin-emu -u /root/u -e /opt/game/NSR.iso > $S/dolphin.log 2>&1 & D=$!
sleep 40
echo "== emulator window: $(xwininfo -root -tree | grep -o "\"NFL Street Reborn Dolphin[^\"]*\"" | head -1)"
import -window root $S/dolphin.png
kill $D; sleep 2; kill -9 $D 2>/dev/null
cd /opt/reborn && GDK_BACKEND=x11 APPIMAGE=/opt/reborn/launcher.AppImage APPDIR=/opt/reborn/squashfs-root squashfs-root/AppRun > $S/launcher.log 2>&1 & L=$!
sleep 25
echo "== launcher window: $(xwininfo -root -tree | grep -o "\"NFL Street Reborn\"" | head -1)"
import -window root $S/launcher.png
kill -9 $L 2>/dev/null; kill $X
echo "== emulator stderr:"; head -5 $S/dolphin.log
echo "== launcher stderr:"; grep -v "DRI3\|accelerated rendering" $S/launcher.log | head -5
'
cp "$A/opt/reborn/shots/"*.png /mnt/c/Users/Adam/AppData/Local/Temp/claude/C--Users-Adam/09dee78c-39e2-4f50-a32f-60a2077b8356/scratchpad/ 2>/dev/null
rm -f "$A/opt/game/NSR.iso"
echo ARCH-CHECK-DONE
