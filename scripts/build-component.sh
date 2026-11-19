#!/bin/sh
# build-component.sh <package> <target>: builds one released component, the core or a plugin, so that
# its sha256 depends only on the source, Cargo.lock, the toolchain of rust-toolchain.toml and the
# runner image (the C compiler and linker of a build are part of its bytes).
#
# - One package per cargo invocation: building several packages at once unifies the features of the
#   dependencies they share, and changes each package's output.
# - The checkout, the target directory and the cargo home are resolved to physical absolute paths
#   (no symbolic link, so macOS /tmp is /private/tmp), passed to cargo in that spelling, and remapped,
#   so no absolute path of this machine reaches the output. The target directory comes after the
#   checkout because rustc applies the last matching prefix: a target directory inside the checkout
#   still becomes /target.
# - The flags are set in CARGO_ENCODED_RUSTFLAGS (one flag per 0x1f-separated field, so a path may
#   hold spaces), which replaces every other source of rustflags; RUSTFLAGS and the caller's profile
#   overrides (CARGO_PROFILE_*) and CARGO_INCREMENTAL are removed from the environment.
# - Apple targets link without debug information (-Wl,-S): ld64 hashes the debug map, which holds the
#   absolute paths of C objects such as ring's, into LC_UUID before strip.
#
# Not covered: [profile] or rustc-wrapper settings in a cargo configuration file outside the checkout.
# The output is <target directory>/<target>/release/<name>. Development builds use cargo directly.
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: scripts/build-component.sh <package> <target>" >&2
  exit 2
fi
package=$1
target=$2

if ! command -v jq > /dev/null 2>&1; then
  echo "build-component.sh: jq is required to read the target directory from cargo metadata; install jq" >&2
  exit 1
fi

# physical <directory>: creates the directory if needed and prints its absolute path, relative to the
# caller's directory, with every symbolic link resolved.
physical() {
  mkdir -p "$1"
  (cd "$1" && pwd -P)
}

root=$(cd "$(dirname "$0")/.." && pwd -P)
CARGO_HOME=$(physical "${CARGO_HOME:-$HOME/.cargo}")
export CARGO_HOME
if [ -n "${CARGO_TARGET_DIR:-}" ]; then
  CARGO_TARGET_DIR=$(physical "$CARGO_TARGET_DIR")
  export CARGO_TARGET_DIR
fi

unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_INCREMENTAL
for name in $(env | sed -n 's/^\(CARGO_PROFILE_[A-Za-z0-9_]*\)=.*/\1/p'); do
  unset "$name"
done

cd "$root"
# The target directory as cargo resolves it, from CARGO_TARGET_DIR, a build.target-dir setting or the
# default.
metadata=$(cargo metadata --format-version 1 --no-deps --locked)
target_dir=$(printf '%s' "$metadata" | jq -r '.target_directory')
if [ -z "$target_dir" ] || [ "$target_dir" = "null" ]; then
  echo "build-component.sh: cargo metadata gave no target directory; run it in the checkout to see why" >&2
  exit 1
fi
CARGO_TARGET_DIR=$(physical "$target_dir")
export CARGO_TARGET_DIR

sep=$(printf '\037')
flags="--remap-path-prefix=$root=/build"
flags="$flags$sep--remap-path-prefix=$CARGO_TARGET_DIR=/target"
flags="$flags$sep--remap-path-prefix=$CARGO_HOME=/cargo"
case $target in
  *-apple-*) flags="$flags$sep-Clink-arg=-Wl,-S" ;;
esac
CARGO_ENCODED_RUSTFLAGS=$flags
export CARGO_ENCODED_RUSTFLAGS

exec cargo build --release --locked --target "$target" --package "$package"
