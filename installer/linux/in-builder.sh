#!/usr/bin/env bash
# Run a command inside the jammy build root, with /proc, /sys, /dev and the
# source tree mounted, and unmount on the way out. Run as root.
set -euo pipefail
ROOT=/srv/jammy
SRC=${REBORN_SRC:-/home/reborn/src}
mkdir -p "$ROOT/src"
mounts=(proc sys dev dev/pts src)
mount -t proc proc "$ROOT/proc"
mount --rbind /sys "$ROOT/sys"
mount --rbind /dev "$ROOT/dev"
mountpoint -q "$ROOT/dev/pts" || mount -t devpts devpts "$ROOT/dev/pts"
mount --bind "$SRC" "$ROOT/src"
cleanup() {
  for m in src dev/pts dev sys proc; do umount -lf "$ROOT/$m" 2>/dev/null || true; done
}
trap cleanup EXIT
# AppImages (linuxdeploy) cannot use FUSE in a chroot.
chroot "$ROOT" /usr/bin/env -i HOME=/root PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin \
  APPIMAGE_EXTRACT_AND_RUN=1 LANG=C.UTF-8 "$@"
