#!/usr/bin/env bash
# One-time: a clean Arch Linux root to run release binaries in. SteamOS is
# Arch based, so a binary that starts here is not leaning on anything Ubuntu
# provides. Only what a SteamOS desktop also has is installed: X/Wayland
# client libraries, Mesa, ALSA/Pulse, GTK. Run as root.
set -euo pipefail
A=/srv/arch
free_gb=$(df -BG --output=avail /mnt/c | tail -1 | tr -dc 0-9)
[ "$free_gb" -ge 10 ] || { echo "Only ${free_gb} GB free on C:."; exit 1; }
if [ ! -x "$A/usr/bin/pacman" ]; then
  mkdir -p /srv && cd /srv
  curl -sSfL -o arch.tar.zst https://geo.mirror.pkgbuild.com/iso/latest/archlinux-bootstrap-x86_64.tar.zst
  apt-get install -y --no-install-recommends zstd > /dev/null
  tar --zstd -xf arch.tar.zst && rm arch.tar.zst
  mv root.x86_64 arch
fi
echo 'Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch' > "$A/etc/pacman.d/mirrorlist"
cp /etc/resolv.conf "$A/etc/resolv.conf"
mount --bind "$A" "$A"
trap 'umount -lf "$A/proc" "$A/dev" "$A/sys" "$A" 2>/dev/null || true' EXIT
# rslave: see in-builder.sh. A shared rbind's unmount takes the host's
# /dev/pts with it and WSL stops starting processes.
mount -t proc proc "$A/proc"
mount --rbind /dev "$A/dev"; mount --make-rslave "$A/dev"
mount --rbind /sys "$A/sys"; mount --make-rslave "$A/sys"
chroot "$A" /usr/bin/bash -euxc '
sed -i "s/^CheckSpace/#CheckSpace/" /etc/pacman.conf
pacman-key --init > /dev/null; pacman-key --populate archlinux > /dev/null
pacman -Syu --noconfirm --needed file binutils xorg-server-xvfb xorg-xwininfo imagemagick \
  mesa vulkan-swrast libx11 libxkbcommon-x11 libxi libxrandr alsa-lib libpulse gtk3 \
  libevdev hidapi libusb bluez-libs curl fuse2 > /dev/null
ldd --version | head -1
'
echo ARCH-READY
