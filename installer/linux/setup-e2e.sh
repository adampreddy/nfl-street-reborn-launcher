#!/usr/bin/env bash
# One-time: what the end-to-end first-run test needs inside the jammy build
# root, so the launcher runs against the SAME WebKitGTK it bundles: a headless
# X server, WebKitGTK's WebDriver, and tauri-driver in front of it. Run as root.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
"$HERE/in-builder.sh" bash -euxc '
export DEBIAN_FRONTEND=noninteractive
apt-get install -y --no-install-recommends webkit2gtk-driver xvfb x11-utils imagemagick dbus-x11 > /dev/null
which WebKitWebDriver Xvfb import
[ -x /root/.cargo/bin/tauri-driver ] || /root/.cargo/bin/cargo install tauri-driver --locked
/root/.cargo/bin/tauri-driver --help | head -3
'
echo E2E-READY
