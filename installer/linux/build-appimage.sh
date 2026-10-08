#!/usr/bin/env bash
# Build the Linux launcher as one AppImage.
#
# What goes in, and why it is exactly this:
#   - reborn-launcher, built by the Tauri CLI from the same source as Windows.
#   - DolphinTool and xdelta3 as external binaries under usr/bin. The game
#     import needs both; an AppImage is read only and nothing can be put
#     beside it, so they ride inside (Adam, 2026-10-01: "tools in the
#     AppImage, patch as content"). The launcher finds them through $APPDIR.
#   - NOT the game patch bundle (manifest + reborn.xdelta, 184 MB): that is
#     the `game-import` content component, fetched into the data root.
#   - Dolphin is added by build-release.sh afterwards (usr/bin/runtime); it
#     is also the `runtime-linux` content component, for updates.
#
# Inputs, placed by CI or by hand before running:
#   crates/reborn-ui/binaries/DolphinTool-x86_64-unknown-linux-gnu
#       upstream Dolphin's dolphin-tool (`header --json`, `extract`); the
#       Reborn fork's own tool lacks `extract`, so this is the upstream one.
#   crates/reborn-ui/binaries/xdelta3-x86_64-unknown-linux-gnu
#       xdelta3 3.0.x.
# The Tauri CLI names external binaries by target triple and strips the
# suffix when it installs them, so the launcher sees `usr/bin/DolphinTool`
# and `usr/bin/xdelta3`.
#
# bundle.active stays false in tauri.conf.json because the Windows build does
# not bundle; the AppImage settings are passed here instead. NO_STRIP keeps
# linuxdeploy from stripping the launcher a second time.
set -euo pipefail
cd "$(dirname "$0")/../../crates/reborn-ui"
for tool in DolphinTool xdelta3; do
  f="binaries/$tool-x86_64-unknown-linux-gnu"
  [ -x "$f" ] || { echo "missing $f (see the header of this script)"; exit 1; }
done
NO_STRIP=true cargo tauri build --bundles appimage --config '{
  "bundle": {
    "active": true,
    "targets": ["appimage"],
    "externalBin": ["binaries/DolphinTool", "binaries/xdelta3"],
    "linux": { "appimage": { "bundleMediaFramework": false } }
  }
}'
ls -la "${CARGO_TARGET_DIR:-../../target}"/release/bundle/appimage/*.AppImage
