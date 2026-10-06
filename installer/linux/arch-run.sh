#!/usr/bin/env bash
# Run a command inside the Arch check root with /proc, /dev and /sys
# mounted (rslave: see in-builder.sh), keeping the command's exit status.
# Run as root.
set -uo pipefail
A=/srv/arch
mountpoint -q "$A" || mount --bind "$A" "$A"
mount -t proc proc "$A/proc"
mount --rbind /dev "$A/dev"; mount --make-rslave "$A/dev"
mount --rbind /sys "$A/sys"; mount --make-rslave "$A/sys"
mkdir -p "$A/src"; mount --bind /home/reborn "$A/src"; mount --make-private "$A/src"
cleanup() {
  local rc=$?
  for m in src sys dev proc; do umount -lf "$A/$m" 2>/dev/null || true; done
  umount -lf "$A" 2>/dev/null || true
  exit "$rc"
}
trap cleanup EXIT
chroot "$A" /usr/bin/env -i HOME=/root PATH=/usr/bin LANG=C.UTF-8 "$@"
