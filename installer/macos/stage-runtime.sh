#!/usr/bin/env bash
# Assemble runtime-macos: the emulator exactly as the content component
# installs it at <data root>/runtime. The macOS counterpart of the
# runtime-linux folder build-release.sh stages.
#
# Usage: stage-runtime.sh <built emulator .app> <runtime-data tar.gz> <out dir>
#
# The shipped Windows runtime's Sys, Languages and User, not the fork's
# Data/Sys: they differ in 1,650 files, and Sys/GameSettings/GN7.ini holds
# settings that override Dolphin.ini. A Mac player gets byte for byte the game
# settings a PC player has.
#
# Layout (Mac Dolphin finds Sys and translations through its bundle):
#   <out>/NFL Street Reborn Dolphin.app/Contents/Resources/Sys
#   <out>/NFL Street Reborn Dolphin.app/Contents/Resources/<lang>.lproj/dolphin-emu.mo
#   <out>/NFL Street Reborn Dolphin.app/Contents/Resources/User   (template)
set -euo pipefail

APP_IN=${1:?usage: stage-runtime.sh <emulator .app> <runtime-data tar.gz> <out dir>}
DATA_TAR=${2:?usage}
OUT=${3:?usage}

NAME="NFL Street Reborn Dolphin.app"
rm -rf "$OUT"
mkdir -p "$OUT"
ditto "$APP_IN" "$OUT/$NAME"
RES="$OUT/$NAME/Contents/Resources"

DATA=$(mktemp -d)
trap 'rm -rf "$DATA"' EXIT
tar -xzf "$DATA_TAR" -C "$DATA"
for d in Sys Languages User; do
  [ -d "$DATA/$d" ] || { echo "the runtime data has no $d"; exit 1; }
done

rm -rf "$RES/Sys" "$RES"/*.lproj "$RES/User"
cp -R "$DATA/Sys" "$RES/Sys"
cp -R "$DATA/User" "$RES/User"
for mo in "$DATA"/Languages/*.mo; do
  lang=$(basename "$mo" .mo)
  mkdir -p "$RES/$lang.lproj"
  cp "$mo" "$RES/$lang.lproj/dolphin-emu.mo"
done
# The Windows tree carries execute bits on plain data; nothing here runs.
find "$RES/Sys" "$RES/User" -type f -exec chmod 644 {} +

# Adding files to Resources breaks the bundle's seal.
codesign --force --deep --sign - "$OUT/$NAME"
codesign --verify --deep --strict "$OUT/$NAME"
echo "staged $OUT/$NAME ($(find "$RES/Sys" -type f | wc -l | tr -d ' ') Sys files)"
