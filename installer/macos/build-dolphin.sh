#!/usr/bin/env bash
# Build Reborn Dolphin (the fork, Metal, arm64) on a Mac with Command Line
# Tools and Homebrew. The macOS counterpart of step 1 of
# installer/linux/build-release.sh; the recipe is the fork's
# .github/workflows/reborn-crossplatform.yml macOS job, which compiled on CI but
# was never run.
#
# Usage: build-dolphin.sh <fork checkout> [build dir]
# Output: <build dir>/Binaries/DolphinQt.app
set -euo pipefail

FORK=${1:?usage: build-dolphin.sh <fork checkout> [build dir]}
BUILD=${2:-$FORK/build-macos}

# The -Werror the fork's age trips on a modern clang, appended last.
NOWERROR="$BUILD/nowerror"
mkdir -p "$BUILD"
printf '#!/usr/bin/env bash\nexec "$@" -Wno-error -Wno-nonnull -Wno-unused-but-set-variable\n' > "$NOWERROR"
chmod +x "$NOWERROR"

# MainWindow.cpp includes <qpa/qplatformnativeinterface.h>, a Qt private
# header Homebrew keeps inside the framework. brew --prefix is a symlink, so
# resolve it before find.
QTBASE="$(brew --prefix qtbase)"
QTROOT="$(cd "$QTBASE" && pwd -P)"
QPA_HDR="$(find -L "$QTROOT" -name qplatformnativeinterface.h -print -quit)"
[ -n "$QPA_HDR" ] || { echo "Qt private headers not found under $QTROOT"; exit 1; }
QT_PRIVATE_INC="$(dirname "$(dirname "$QPA_HDR")")"

# Not fmt or pugixml from Homebrew: this snapshot expects the older fmt API and
# bundles both in Externals. Not installing them is not enough: Homebrew's node
# pulls in fmt 12, which CMake then prefers ("no matching function for call to
# 'MsgAlertFmtImpl'"), so the search is switched off. ENCODE_FRAMEDUMPS off
# drops ffmpeg (frame dumping is not a Reborn feature) and with it a pile of
# dylibs to bundle.
cmake -S "$FORK" -B "$BUILD" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_PREFIX_PATH="$QTBASE" \
  -DCMAKE_OSX_ARCHITECTURES=arm64 \
  -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0 \
  -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
  -DCMAKE_DISABLE_FIND_PACKAGE_SDL2=ON \
  -DCMAKE_DISABLE_FIND_PACKAGE_fmt=ON \
  -DCMAKE_DISABLE_FIND_PACKAGE_pugixml=ON \
  -DENABLE_TESTS=OFF \
  -DENABLE_AUTOUPDATE=OFF \
  -DENABLE_VULKAN=OFF \
  -DENCODE_FRAMEDUMPS=OFF \
  -DMACOS_CODE_SIGNING=OFF \
  -DSKIP_POSTPROCESS_BUNDLE=ON \
  -DCMAKE_C_COMPILER_LAUNCHER="$NOWERROR" \
  -DCMAKE_CXX_COMPILER_LAUNCHER="$NOWERROR" \
  -DCMAKE_CXX_FLAGS="-isystem $QT_PRIVATE_INC"

cmake --build "$BUILD" --parallel "$(sysctl -n hw.ncpu)" --target dolphin-emu

# The fork names the bundle after the product. CMake's own fixup_bundle
# (skipped above) fails on the spaces in it ("not a valid bundle");
# macdeployqt copies the Qt frameworks and plugins in and re-points them, so
# the app runs on a Mac with no Homebrew.
APP="$BUILD/Binaries/NFL Street Reborn Dolphin.app"
EXE="$APP/Contents/MacOS/NFL Street Reborn Dolphin"
[ -x "$EXE" ] || { echo "the emulator was not built"; exit 1; }
"$QTBASE/bin/macdeployqt" "$APP" -always-overwrite

# Homebrew keeps every Qt module's plugins in one folder, so macdeployqt also
# deploys PDF, SVG, virtual keyboard and image-format plugins, and the QML
# stack and image libraries behind them. Dolphin needs the Cocoa platform and
# the macOS style; it reads textures and art through its own libspng.
rm -rf "$APP"/Contents/PlugIns/{imageformats,iconengines,platforminputcontexts}

# Homebrew's libraries arrive read-only, and install_name_tool below fails on
# them inside a pipe, where set -e never hears of it.
FW="$APP/Contents/Frameworks"
chmod -R u+w "$APP"

# Every Mach-O in the code folders, found by content: Homebrew installs
# framework binaries r--r--r--, so a permission test missed QtGui and its
# libraries were pruned as unused. Resources is left out (Sys holds thousands
# of .ini files that carry an execute bit).
machos() {
  find "$APP/Contents/MacOS" "$FW" "$APP/Contents/PlugIns" -type f -print0 \
    | while IFS= read -r -d '' f; do file -b "$f" | grep -q '^Mach-O' && printf '%s\0' "$f"; done
}
# What a binary loads, without its own install name: `otool -L` lists a
# library's id first, which made every library look used by itself.
deps() { otool -L "$1" 2>/dev/null | awk '/^\t/ {print $1}' | grep -vxF "$(otool -D "$1" 2>/dev/null | tail -n +2)" || true; }

# Then drop every bundled library nothing references any more, until none is
# left to drop (removing one can orphan another).
shopt -s nullglob   # with no .dylib left, a literal glob "pruned" forever
while :; do
  USED=$(machos | while IFS= read -r -d '' bin; do deps "$bin"; done | xargs -n1 basename | sort -u)
  PRUNED=0
  for lib in "$FW"/*.dylib "$FW"/*.framework; do
    name=$(basename "$lib"); name=${name%.framework}
    echo "$USED" | grep -qxF "$name" || { rm -rf "$lib"; PRUNED=1; }
  done
  [ "$PRUNED" = 1 ] || break
done

# macdeployqt re-points only what it recognizes as Qt's; a second-level
# Homebrew dependency (freetype -> brotlidec -> brotlicommon) keeps its
# absolute path. Every such reference goes to the bundled copy.
machos | while IFS= read -r -d '' bin; do
  deps "$bin" | { grep '^/opt/homebrew' || true; } | while read -r dep; do
    rel=${dep#*/lib/}   # QtGui.framework/Versions/A/QtGui, or libfoo.1.dylib
    [ -e "$FW/$rel" ] || cp "$dep" "$FW/$(basename "$dep")"
    install_name_tool -change "$dep" "@executable_path/../Frameworks/$rel" "$bin"
  done
  # A library's own id too, for the ones macdeployqt did not process. Not
  # read at load time, but it is what the check below would flag.
  id=$(otool -D "$bin" 2>/dev/null | tail -n +2)
  case "$id" in /opt/homebrew/*) install_name_tool -id "@executable_path/../Frameworks/${id#*/lib/}" "$bin" ;; esac
done

# No symlinks: each Qt framework becomes a plain library file,
# Contents/Frameworks/QtGui. A framework's Versions/Current links do not
# survive the content system (it carries files) or a copy through Windows,
# and codesign refuses a framework that has lost them ("bundle format
# unrecognized"). Dyld never needed them: it loads Versions/A by path. Only
# bundle-relative references move; /System frameworks stay as they are.
# Measured 2026-10-08: the flattened bundle signs, verifies and renders the
# game (447 frames in 30 s).
for fw in "$FW"/*.framework; do
  n=$(basename "$fw" .framework)
  mv "$fw/Versions/A/$n" "$FW/$n.flat" && rm -rf "$fw" && mv "$FW/$n.flat" "$FW/$n"
done
machos | while IFS= read -r -d '' bin; do
  otool -L "$bin" | awk '/^\t@[a-z_]+\/.*\.framework\/Versions\/A\// {print $1}' | while read -r dep; do
    install_name_tool -change "$dep" "@executable_path/../Frameworks/$(basename "$dep")" "$bin"
  done
  id=$(otool -D "$bin" 2>/dev/null | tail -n +2)
  case "$id" in @*.framework/*) install_name_tool -id "@executable_path/../Frameworks/$(basename "$id")" "$bin" ;; esac
done
LINKS=$(find "$APP" -type l)
[ -z "$LINKS" ] || { echo "the bundle still holds symlinks:"; echo "$LINKS"; exit 1; }

# install_name_tool voids signatures, and Apple Silicon refuses to run
# unsigned code. Ad-hoc, as the launcher is: no Apple Developer account.
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"
# Anything in the bundle still pointing into Homebrew would work here and
# nowhere else. Collected into a variable first: `otool | grep -q` under
# pipefail reads a match as a failure (grep exits, otool dies of SIGPIPE) and
# passed a bundle full of Homebrew paths.
LEAKS=$(machos \
  | xargs -0 otool -L 2>/dev/null | grep /opt/homebrew || true)
[ -z "$LEAKS" ] || { echo "the bundle still links Homebrew:"; echo "$LEAKS"; exit 1; }
echo "built $APP"
