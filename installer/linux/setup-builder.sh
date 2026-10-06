#!/usr/bin/env bash
# One-time: an Ubuntu 22.04 (jammy) build root for Linux RELEASE builds.
#
# Why not the machine's own distro: binaries link against the libraries of
# the system they were built on. Built on Ubuntu 24.04 they need glibc 2.39
# and ~180 Ubuntu-named libraries, and a Steam Deck (SteamOS, Arch based)
# would refuse to start them. Built on 22.04 the floor is glibc 2.35, and
# build-release.sh bundles every non-system library beside the binary, the
# way Dolphin's own Linux builds and every AppImage do.
#
# Why a chroot and not a container or a second WSL distro: no daemon, no
# second virtual disk on a C: drive that has run out of room twice.
#
# Run as root inside the WSL distro: sudo installer/linux/setup-builder.sh
set -euo pipefail
ROOT=/srv/jammy

free_gb=$(df -BG --output=avail /mnt/c | tail -1 | tr -dc 0-9)
if [ "$free_gb" -lt 10 ]; then
  echo "Only ${free_gb} GB free on C:. Free at least 10 GB first."; exit 1
fi

apt-get install -y --no-install-recommends debootstrap
if [ ! -x "$ROOT/bin/bash" ]; then
  debootstrap --variant=minbase --components=main,universe jammy "$ROOT" http://archive.ubuntu.com/ubuntu
fi
cat > "$ROOT/etc/apt/sources.list" <<'EOF'
deb http://archive.ubuntu.com/ubuntu jammy main universe
deb http://archive.ubuntu.com/ubuntu jammy-updates main universe
deb http://security.ubuntu.com/ubuntu jammy-security main universe
EOF
echo 'Acquire::ForceIPv4 "true";' > "$ROOT/etc/apt/apt.conf.d/99force-ipv4"
cp /etc/resolv.conf "$ROOT/etc/resolv.conf"

"$(dirname "$0")/in-builder.sh" bash -euxc '
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  build-essential gcc-12 g++-12 git curl ca-certificates file patchelf pkg-config \
  ninja-build python3 xz-utils squashfs-tools desktop-file-utils \
  libssl-dev libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
  libxdo-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
  libbluetooth-dev libasound2-dev libpulse-dev libgl1-mesa-dev libegl-dev libudev-dev \
  libusb-1.0-0-dev libxi-dev libxrandr-dev libcurl4-openssl-dev libsystemd-dev \
  libhidapi-dev libevdev-dev liblzma-dev libvulkan-dev \
  qt6-base-dev qt6-base-private-dev libqt6svg6-dev qt6-wayland libgl-dev \
  libxkbcommon-x11-0 libxcb-cursor0 xdelta3
# CMake from Kitware: jammy ships 3.22, Dolphin externals want newer.
curl -sSfL https://github.com/Kitware/CMake/releases/download/v3.30.5/cmake-3.30.5-linux-x86_64.tar.gz \
  | tar -xz -C /opt && ln -sf /opt/cmake-3.30.5-linux-x86_64/bin/* /usr/local/bin/
cmake --version | head -1
# Rust, for the launcher.
[ -x /root/.cargo/bin/cargo ] || curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
/root/.cargo/bin/cargo install tauri-cli --version "^2" --locked
# linuxdeploy + Qt plugin, to bundle libraries beside Dolphin.
mkdir -p /opt/linuxdeploy && cd /opt/linuxdeploy
for f in linuxdeploy-x86_64.AppImage linuxdeploy-plugin-qt-x86_64.AppImage; do
  [ -x "$f" ] || { curl -sSfLO "https://github.com/linuxdeploy/${f%%-x86_64*}/releases/download/continuous/$f"; chmod +x "$f"; }
done
ls -la /opt/linuxdeploy
ldd --version | head -1
'
echo BUILDER-READY
