#!/usr/bin/env bash
# Build the macOS release (Apple Silicon) from fixed commits: the Reborn
# Dolphin runtime and the launcher .app, packaged as a .dmg and a .app.zip.
# The macOS counterpart of installer/linux/build-release.sh. Runs on a Mac
# with Command Line Tools and Homebrew (see installer/macos/HANDOFF.md).
#
# Usage:
#   REBORN_BUILD_VERSION=3.0.36 build-release.sh <fork> <upstream dolphin> <out dir>
#
# REBORN_BUILD_VERSION is the version players see and self-update compares
# against latest.json. Required, and it must equal the live launcher
# version: an older stamp offers the player the live one forever.
#
# Inputs:
#   <fork>              the Reborn Dolphin fork at the shipped runtime commit
#                       plus macOS build fixes only (cross-play needs it)
#   <upstream dolphin>  upstream Dolphin, for dolphin-tool (`header`, `extract`;
#                       the fork's own tool lacks them)
#   installer/macos/runtime-data-<RUNTIME_VERSION>.tar.gz
#                       Sys, Languages and User of the shipped Windows runtime
#   installer/macos/reborn-builder-src.tar.gz
#                       the roster Builder's source (pure Python)
#
# Output, in <out dir>:
#   NFL Street Reborn <v>.dmg        what a player downloads
#   NFL Street Reborn <v>.app.zip    the self-update asset (latest.json "macos")
#   runtime-macos-<runtime>/         publishable as `runtime-macos`
#   BUILD-INFO.txt                   commits, sha256, bytes
#
# Releases are published from the Windows PC, never from here: the signing
# keys and upload config live only there.
set -euo pipefail

VERSION=${REBORN_BUILD_VERSION:?set REBORN_BUILD_VERSION to the live launcher version}
FORK=${1:?usage: REBORN_BUILD_VERSION=<v> build-release.sh <fork> <upstream dolphin> <out dir>}
UPSTREAM=${2:?usage}
OUT=${3:?usage}
RUNTIME_VERSION=${RUNTIME_VERSION:-1.2.12}

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$REPO/target}
TRIPLE=aarch64-apple-darwin
BIN="$REPO/crates/reborn-ui/binaries"
WORK="$OUT/work"
mkdir -p "$OUT" "$WORK" "$BIN"

FORK_COMMIT=$(git -C "$FORK" rev-parse HEAD)
UPSTREAM_COMMIT=$(git -C "$UPSTREAM" rev-parse HEAD)
LAUNCHER_COMMIT=$(git -C "$REPO" rev-parse HEAD)
[ -z "$(git -C "$REPO" status --porcelain -- crates)" ] || { echo "the launcher has uncommitted changes under crates/"; exit 1; }
echo "fork $FORK_COMMIT  upstream $UPSTREAM_COMMIT  launcher $LAUNCHER_COMMIT  version $VERSION"

# Every Mach-O in a folder, found by content (Homebrew installs some r--r--r--).
machos() { find "$@" -type f -print0 | while IFS= read -r -d '' f; do file -b "$f" | grep -q '^Mach-O' && printf '%s\0' "$f"; done; }
# Fail when anything in the folders still loads from Homebrew. Collected
# first: `otool | grep -q` under pipefail reads a match as a failure.
no_homebrew() {
  local leaks
  leaks=$(machos "$@" | xargs -0 otool -L 2>/dev/null | grep /opt/homebrew || true)
  [ -z "$leaks" ] || { echo "still links Homebrew:"; echo "$leaks"; exit 1; }
}

# --- 1. Reborn Dolphin, staged as runtime-macos ------------------------------
"$HERE/build-dolphin.sh" "$FORK" "$WORK/dolphin-build"
RUNTIME="$OUT/runtime-macos-$RUNTIME_VERSION"
"$HERE/stage-runtime.sh" "$WORK/dolphin-build/Binaries/NFL Street Reborn Dolphin.app" \
  "$HERE/runtime-data-$RUNTIME_VERSION.tar.gz" "$RUNTIME"

# --- 2. dolphin-tool, upstream, bundled libraries only -----------------------
cmake -S "$UPSTREAM" -B "$WORK/tool-build" -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0 -DUSE_SYSTEM_LIBS=OFF \
  -DENABLE_QT=OFF -DENABLE_NOGUI=OFF -DENABLE_TESTS=OFF -DENABLE_AUTOUPDATE=OFF \
  -DENABLE_ANALYTICS=OFF -DENCODE_FRAMEDUMPS=OFF -DENABLE_LLVM=OFF -DUSE_DISCORD_PRESENCE=OFF \
  -DENABLE_VULKAN=OFF -DMACOS_CODE_SIGNING=OFF
cmake --build "$WORK/tool-build" --parallel "$(sysctl -n hw.ncpu)" --target dolphin-tool
cp "$WORK/tool-build/Binaries/dolphin-tool" "$BIN/DolphinTool-$TRIPLE"
no_homebrew "$BIN/DolphinTool-$TRIPLE"

# --- 3. xdelta3 -----------------------------------------------------------------
# Homebrew's, with LZMA: the patches are made with xdelta3's default secondary
# compression. Its two libraries are bundled and re-pointed in step 6.
cp "$(brew --prefix xdelta)/bin/xdelta3" "$BIN/xdelta3-$TRIPLE"
chmod u+w "$BIN/xdelta3-$TRIPLE"

# --- 4. The roster Builder ------------------------------------------------------
# Frozen from its own source (pure standard-library Python) with PyInstaller,
# as on Linux, so a Mac applies roster updates to its game the way Windows does.
rm -rf "$WORK/builder"; mkdir -p "$WORK/builder"
tar -xzf "$HERE/reborn-builder-src.tar.gz" -C "$WORK/builder"
[ -d "$WORK/venv" ] || python3 -m venv "$WORK/venv"
"$WORK/venv/bin/pip" install -q "pyinstaller==6.*"
(cd "$WORK/builder" && "$WORK/venv/bin/python" -m PyInstaller --noconfirm --clean reborn-builder-cli.spec > "$WORK/builder-freeze.log")
"$WORK/builder/dist/reborn-builder-cli" --help > /dev/null
cp "$WORK/builder/dist/reborn-builder-cli" "$BIN/reborn-builder-cli-$TRIPLE"

# --- 5. The launcher .app -------------------------------------------------------
# bundle.active stays false in tauri.conf.json because the Windows build does
# not bundle; the macOS settings are passed here, as Linux passes its own.
(cd "$REPO/crates/reborn-ui" && REBORN_BUILD_VERSION="$VERSION" cargo tauri build --bundles app --config "{
  \"version\": \"$VERSION\",
  \"bundle\": {
    \"active\": true,
    \"targets\": [\"app\"],
    \"externalBin\": [\"binaries/DolphinTool\", \"binaries/xdelta3\", \"binaries/reborn-builder-cli\"],
    \"macOS\": { \"minimumSystemVersion\": \"11.0\" }
  }
}")
BUILT=$(find "$CARGO_TARGET_DIR/release/bundle/macos" -maxdepth 1 -name '*.app' | head -1)
[ -d "$BUILT" ] || { echo "the launcher .app was not built"; exit 1; }
APP_NAME=$(basename "$BUILT")
APP="$OUT/$APP_NAME"
rm -rf "$APP"
ditto "$BUILT" "$APP"

# --- 6. The emulator and the tools' libraries inside ----------------------------
# The first content check adopts the bundled emulator from the install root
# (Contents/Resources/runtime), so a fresh install fetches no runtime.
mkdir -p "$APP/Contents/Resources/runtime" "$APP/Contents/Frameworks"
ditto "$RUNTIME" "$APP/Contents/Resources/runtime"
X="$APP/Contents/MacOS/xdelta3"
otool -L "$X" | awk '/^\t\/opt\/homebrew/ {print $1}' | while read -r dep; do
  name=$(basename "$dep")
  cp "$dep" "$APP/Contents/Frameworks/$name"
  chmod u+w "$APP/Contents/Frameworks/$name"
  install_name_tool -id "@executable_path/../Frameworks/$name" "$APP/Contents/Frameworks/$name"
  install_name_tool -change "$dep" "@executable_path/../Frameworks/$name" "$X"
done

LINKS=$(find "$APP" -type l)
[ -z "$LINKS" ] || { echo "the app holds symlinks:"; echo "$LINKS"; exit 1; }
no_homebrew "$APP"
xattr -cr "$APP"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"
"$APP/Contents/MacOS/xdelta3" -V > /dev/null 2>&1 || "$APP/Contents/MacOS/xdelta3" -h > /dev/null 2>&1 \
  || { echo "the bundled xdelta3 does not run"; exit 1; }

# --- 7. The .dmg and the update zip ---------------------------------------------
DMG="$OUT/NFL Street Reborn $VERSION.dmg"
ZIP="$OUT/NFL Street Reborn $VERSION.app.zip"
STAGE="$WORK/dmg"
rm -rf "$STAGE"; mkdir -p "$STAGE"
ditto "$APP" "$STAGE/$APP_NAME"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG" "$ZIP"
hdiutil create -volname "NFL Street Reborn" -srcfolder "$STAGE" -format UDZO -ov "$DMG" > /dev/null
ditto -c -k --keepParent "$APP" "$ZIP"

# --- 8. BUILD-INFO --------------------------------------------------------------
sum() { shasum -a 256 "$1" | cut -d' ' -f1; }
bytes() { stat -f %z "$1"; }
{
  echo "NFL Street Reborn for macOS (Apple Silicon), launcher $VERSION"
  echo "built             $(date -u '+%Y-%m-%d %H:%M UTC')"
  echo "launcher          $LAUNCHER_COMMIT"
  echo "Reborn Dolphin    fork $FORK_COMMIT, runtime $RUNTIME_VERSION, Metal"
  echo "dolphin-tool      upstream $UPSTREAM_COMMIT"
  echo "xdelta3           $("$X" -V 2>&1 | head -1)"
  echo "Builder           $("$APP/Contents/MacOS/reborn-builder-cli" --version 2>&1 | head -1)"
  echo "minimum macOS     11.0, arm64 only, ad-hoc signed (no Apple Developer account)"
  echo
  echo "$(basename "$DMG")      sha256 $(sum "$DMG")  bytes $(bytes "$DMG")"
  echo "$(basename "$ZIP")  sha256 $(sum "$ZIP")  bytes $(bytes "$ZIP")"
} > "$OUT/BUILD-INFO.txt"
cat "$OUT/BUILD-INFO.txt"
