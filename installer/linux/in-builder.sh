#!/usr/bin/env bash
# Run a command inside the jammy build root, with /proc, /sys, /dev and the
# source tree mounted, and unmount on the way out. Run as root.
set -euo pipefail
ROOT=/srv/jammy
SRC=${REBORN_SRC:-/home/reborn/src}
mkdir -p "$ROOT/src"
mounts=(proc sys dev dev/pts src)
# WSL mounts /dev and /sys SHARED. A plain rbind joins their peer group, and
# unmounting it on the way out then unmounts the host's own /dev/pts too:
# WSL can no longer start a process ("Catastrophic failure", observed twice
# on 2026-10-02). rslave lets mounts flow in and never back out.
mount -t proc proc "$ROOT/proc"
mount --rbind /sys "$ROOT/sys"; mount --make-rslave "$ROOT/sys"
mount --rbind /dev "$ROOT/dev"; mount --make-rslave "$ROOT/dev"
mount --bind "$SRC" "$ROOT/src"; mount --make-private "$ROOT/src"
# Keeps the command's own exit status: without the explicit exit, the last
# `|| true` below became the script's status and a failed build read as done.
cleanup() {
  local rc=$?
  for m in src dev/pts dev sys proc; do umount -lf "$ROOT/$m" 2>/dev/null || true; done
  exit "$rc"
}
trap cleanup EXIT
# AppImages (linuxdeploy) cannot use FUSE in a chroot.
chroot "$ROOT" /usr/bin/env -i HOME=/root PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin \
  APPIMAGE_EXTRACT_AND_RUN=1 LANG=C.UTF-8 "$@"
