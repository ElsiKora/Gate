#!/bin/sh
# pack-core.sh <binary> <archive>: packs the core binary as <archive> (gate-<target>.tar.gz) with one
# member, "gate", whose bytes depend only on the binary: a fixed mtime, owner and group 0 without
# names, mode 0755, and no gzip name or timestamp. Works with GNU tar and bsdtar. It fails, and leaves
# no archive, unless the archive lists exactly that one member.
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: scripts/pack-core.sh <binary> <archive>" >&2
  exit 2
fi
binary=$1
archive=$2

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp "$binary" "$stage/gate"
chmod 0755 "$stage/gate"
TZ=UTC touch -t 197001010000.00 "$stage/gate"

# The tar is written to a file before gzip, so a failing tar stops the script under set -e.
if tar --version 2>/dev/null | grep -q 'GNU tar'; then
  tar --format=ustar --owner=0 --group=0 --numeric-owner --mtime=@0 -C "$stage" -cf "$stage/gate.tar" gate
else
  tar --format ustar --uid 0 --gid 0 --uname '' --gname '' --no-xattrs --no-mac-metadata -C "$stage" -cf "$stage/gate.tar" gate
fi
gzip -n -9 -c "$stage/gate.tar" > "$archive"

members=$(tar -tzf "$archive")
if [ "$members" != "gate" ]; then
  echo "pack-core.sh: $archive must hold the one member gate, but holds: $members" >&2
  rm -f "$archive"
  exit 1
fi
