#!/usr/bin/env bash
# Build the Linux (Steam Deck) release from fixed commits: the Reborn Dolphin
# runtime and the launcher AppImage. Stands in for CI, which GitHub refuses to
# run on this account (billing), and is what CI should run when it can.
#
# Usage (as root inside the WSL distro, after setup-builder.sh):
#   build-release.sh <windows-runtime-folder>
#
# Inputs:
#   /home/reborn/src/dolphin    the fork, checked out at the runtime commit
#                               (the SAME commit as the Windows runtime, plus
#                               only Linux build fixes: cross-play needs it)
#   /home/reborn/src/dolphin-upstream   upstream Dolphin for dolphin-tool
#   /home/reborn/src/launcher   the launcher, at the release commit
#   <windows-runtime-folder>    the shipped Windows runtime. Its Sys,
#                               Languages and User template are copied as is,
#                               so a Deck player has byte-for-byte the game
#                               settings a PC player has.
#
# Output: /home/reborn/release/<launcher version>/
#   NFL Street Reborn_<v>_amd64.AppImage   the launcher, with DolphinTool and
#                                          xdelta3 inside
#   runtime-linux/                         publishable as `runtime-linux`
#   BUILD-INFO.txt                         commits, glibc floor, sizes
set -euo pipefail
WIN_RUNTIME=${1:?usage: build-release.sh <windows-runtime-folder>}
HERE=$(cd "$(dirname "$0")" && pwd)
SRC=/home/reborn/src
J=/srv/jammy

free_gb=$(df -BG --output=avail /mnt/c | tail -1 | tr -dc 0-9)
[ "$free_gb" -ge 10 ] || { echo "Only ${free_gb} GB free on C:. Free at least 10 GB first."; exit 1; }
for d in "$SRC/dolphin" "$SRC/dolphin-upstream" "$SRC/launcher" "$WIN_RUNTIME/Sys" "$WIN_RUNTIME/Languages" "$WIN_RUNTIME/User"; do
  [ -e "$d" ] || { echo "missing input: $d"; exit 1; }
done
# Root reads repositories the build user owns; say they are trusted rather
# than let git refuse and leave an empty answer that reads as "clean".
g() { git -c safe.directory='*' "$@"; }
for repo in dolphin launcher; do
  changes=$(g -C "$SRC/$repo" status --porcelain --untracked-files=no) || { echo "git could not read $SRC/$repo"; exit 1; }
  [ -z "$changes" ] || { echo "$SRC/$repo has uncommitted changes"; exit 1; }
done

FORK_COMMIT=$(g -C "$SRC/dolphin" rev-parse HEAD)
LAUNCHER_COMMIT=$(g -C "$SRC/launcher" rev-parse HEAD)
UPSTREAM_COMMIT=$(g -C "$SRC/dolphin-upstream" rev-parse HEAD 2>/dev/null || echo unknown)
VERSION=$(grep -m1 '^version' "$SRC/launcher/Cargo.toml" | cut -d'"' -f2)
OUT=/home/reborn/release/$VERSION
rm -rf "$OUT"; mkdir -p "$OUT"
echo "fork $FORK_COMMIT  launcher $LAUNCHER_COMMIT  version $VERSION"

# The -Werror the fork's age trips on modern compilers, appended last.
printf '#!/usr/bin/env bash\nexec "$@" -Wno-error -Wno-nonnull -Wno-unused-but-set-variable\n' > "$J/usr/local/bin/nowerror"
chmod +x "$J/usr/local/bin/nowerror"

# --- 1. Reborn Dolphin -------------------------------------------------------
# ENCODE_FRAMEDUMPS off drops ffmpeg and its ~120 libraries (frame dumping is
# not a Reborn feature). Libraries jammy has no -dev package for here (fmt,
# pugixml, zstd, lzo, minizip, spng, sfml, mbedtls, enet...) come from the
# fork's own Externals and are linked in statically.
"$HERE/in-builder.sh" bash -euxc '
cd /src/dolphin
cmake -S . -B build-release -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DLINUX_LOCAL_DEV=ON -DENCODE_FRAMEDUMPS=OFF -DENABLE_TESTS=OFF -DENABLE_AUTOUPDATE=OFF \
  -DENABLE_ANALYTICS=OFF -DENABLE_LLVM=OFF -DENABLE_NOGUI=OFF -DUSE_DISCORD_PRESENCE=OFF \
  -DCMAKE_C_COMPILER=gcc-12 -DCMAKE_CXX_COMPILER=g++-12 \
  -DCMAKE_C_COMPILER_LAUNCHER=nowerror -DCMAKE_CXX_COMPILER_LAUNCHER=nowerror
cmake --build build-release --parallel 10 --target dolphin-emu
rm -rf /tmp/dolphin.AppDir
# Ubuntu keeps Qt 6 qmake outside PATH; the Qt plugin asks qmake where Qt is.
export QMAKE=/usr/lib/qt6/bin/qmake
/opt/linuxdeploy/linuxdeploy-x86_64.AppImage --appdir /tmp/dolphin.AppDir \
  --executable build-release/Binaries/dolphin-emu --plugin qt \
  --exclude-library "libvulkan.so*"
# Flatten into the layout the launcher expects: the emulator at the root of
# runtime/, libraries in lib/, Qt plugins in plugins/.
R=/src/release-runtime; rm -rf "$R"; mkdir -p "$R"
mv /tmp/dolphin.AppDir/usr/bin/dolphin-emu "$R/"
mv /tmp/dolphin.AppDir/usr/lib "$R/lib"
mv /tmp/dolphin.AppDir/usr/plugins "$R/plugins"
patchelf --set-rpath "\$ORIGIN/lib" "$R/dolphin-emu"
printf "[Paths]\nPrefix = .\nPlugins = plugins\nLibraries = lib\n" > "$R/qt.conf"
'
[ -x "$SRC/release-runtime/dolphin-emu" ] || { echo "the runtime was not built"; exit 1; }
mv "$SRC/release-runtime" "$OUT/runtime-linux"
cp -r "$WIN_RUNTIME/Sys" "$WIN_RUNTIME/Languages" "$OUT/runtime-linux/"
mkdir -p "$OUT/runtime-linux/User"
cp -r "$WIN_RUNTIME/User/Config" "$WIN_RUNTIME/User/GameSettings" "$OUT/runtime-linux/User/"
find "$OUT/runtime-linux" -name '*.bak_*' -delete

# --- 2. dolphin-tool (upstream: it has `extract`) ---------------------------
"$HERE/in-builder.sh" bash -euxc '
cd /src/dolphin-upstream
cmake -S . -B build-release -G Ninja -DCMAKE_BUILD_TYPE=Release -DENABLE_QT=OFF \
  -DENABLE_NOGUI=OFF -DENABLE_TESTS=OFF -DENABLE_AUTOUPDATE=OFF -DENABLE_ANALYTICS=OFF \
  -DENCODE_FRAMEDUMPS=OFF -DENABLE_LLVM=OFF -DUSE_DISCORD_PRESENCE=OFF -DLINUX_LOCAL_DEV=ON \
  -DCMAKE_C_COMPILER=gcc-12 -DCMAKE_CXX_COMPILER=g++-12
cmake --build build-release --parallel 10 --target dolphin-tool
mkdir -p /src/launcher/crates/reborn-ui/binaries
cp build-release/Binaries/dolphin-tool /src/launcher/crates/reborn-ui/binaries/DolphinTool-x86_64-unknown-linux-gnu
cp /usr/bin/xdelta3 /src/launcher/crates/reborn-ui/binaries/xdelta3-x86_64-unknown-linux-gnu
'

# --- 3. The launcher AppImage -------------------------------------------------
"$HERE/in-builder.sh" bash -euxc '
export CARGO_TARGET_DIR=/src/launcher/target-jammy
bash /src/launcher/installer/linux/build-appimage.sh
'
cp "$SRC/launcher/target-jammy/release/bundle/appimage/"*.AppImage "$OUT/"

# --- 4. What was built ---------------------------------------------------------
glibc_floor() { find "$@" -type f \( -perm -u+x -o -name '*.so*' \) -exec objdump -T {} + 2>/dev/null \
  | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1; }
EX=$(mktemp -d); (cd "$EX" && "$OUT"/*.AppImage --appimage-extract >/dev/null)
{
  echo "NFL Street Reborn Linux release $VERSION, built $(date -u +%FT%TZ)"
  echo "launcher commit   $LAUNCHER_COMMIT"
  echo "runtime commit    $FORK_COMMIT  (Reborn Dolphin fork)"
  echo "dolphin-tool      upstream $UPSTREAM_COMMIT"
  echo "Sys/Languages/User from $WIN_RUNTIME"
  echo "glibc floor       runtime $(glibc_floor "$OUT/runtime-linux")  AppImage $(glibc_floor "$EX/squashfs-root")"
  echo "sizes             $(du -sh "$OUT"/*.AppImage | cut -f1) AppImage, $(du -sh "$OUT/runtime-linux" | cut -f1) runtime"
  echo "runtime libs      $(ls "$OUT/runtime-linux/lib" | wc -l) bundled"
  echo "AppImage usr/bin  $(ls "$EX/squashfs-root/usr/bin" | tr '\n' ' ')"
} | tee "$OUT/BUILD-INFO.txt"
rm -rf "$EX"
chown -R reborn:reborn "$SRC" /home/reborn/release
echo RELEASE-DONE "$OUT"
