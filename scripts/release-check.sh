#!/bin/sh
# release-check.sh <version> [--keep <plugin>]...: prints every place of the repository's tracked
# files where a version appears that must agree at a release, and exits 1 when one disagrees with
# <version>. It reads files only, with no network and no cargo. Run it from the repository root
# before tagging v<version> and the plugins.
#
# A shared plugin is released at <version> too, unless --keep <plugin> names it: a kept plugin keeps
# the version its Cargo.toml declares, and every place of that plugin is held to that version.
#
# The places, one line each, `ok` or `MISMATCH`, then the file, the line, what it is, the version
# found and, on a mismatch, the version wanted:
#
# - the workspace version and the version of every workspace member, from its Cargo.toml;
# - the version of every workspace member in Cargo.lock;
# - the top version heading of CHANGELOG.md and of each plugin's CHANGELOG.md: `# <v> (unreleased)`,
#   `# [<v>](<link>) (<date>)` or `## <v>`; a heading with no version (`## Unreleased`) is a mismatch;
# - in the documents (README.md, docs/ but not docs/tasks/ or docs/plan.md, and the plugins' rule and
#   generator pages): each pin `ElsiKora/Gate:<plugin>@<v>`, each release tag of the repository
#   (`releases/download/v<v>/`, `tag = "v<v>"`, `ElsiKora/Gate/v<v>/`, `plugin-<plugin>-v<v>`),
#   each core version an example pins (`version = "<v>"` under `[gate]`, `version=<v>` in an install
#   script) and the release README.md calls current;
# - the plugin interface, `package gate:plugin@<v>;` in wit/gate.wit, wanted at <major>.<minor>.0 of
#   <version>, and every `gate:plugin@<v>` the documents and the runtime's INTERFACE name, wanted
#   equal to it.
#
# The root gate.toml, with which Gate checks itself, pins released assets that gate.lock must hash,
# so its pins move after the release; they are printed as `later`, never a mismatch.
set -eu

usage() {
  echo "usage: scripts/release-check.sh <version> [--keep <plugin>]..." >&2
  exit 2
}

[ "$#" -ge 1 ] || usage
target=$1
shift
case $target in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "release-check.sh: $target is not a version <major>.<minor>.<patch>" >&2; exit 2 ;;
esac
keep=" "
while [ "$#" -gt 0 ]; do
  case $1 in
    --keep)
      [ "$#" -ge 2 ] || usage
      [ -f "plugins/$2/Cargo.toml" ] || { echo "release-check.sh: --keep $2: there is no plugins/$2/Cargo.toml" >&2; exit 2; }
      keep="$keep$2 "
      shift 2
      ;;
    *) usage ;;
  esac
done
[ -f Cargo.toml ] && [ -f wit/gate.wit ] || { echo "release-check.sh: run it from the repository root" >&2; exit 2; }

interface=$(printf '%s\n' "$target" | awk -F. '{ print $1 "." $2 ".0" }')
mismatches=0
places=0
tmp=$(mktemp "${TMPDIR:-/tmp}/release-check.XXXXXX")
trap 'rm -f "$tmp"' EXIT

# Prints the lines a pipeline wrote to $tmp (its reports ran in a subshell) and counts them.
tally() {
  cat "$tmp"
  places=$((places + $(grep -c . "$tmp" || true)))
  mismatches=$((mismatches + $(grep -c '^MISMATCH' "$tmp" || true)))
}

# report <found> <wanted> <file> <line> <what>
report() {
  places=$((places + 1))
  if [ "$1" = "$2" ]; then
    printf 'ok       %s:%s %s %s\n' "$3" "$4" "$5" "$1"
  else
    mismatches=$((mismatches + 1))
    printf 'MISMATCH %s:%s %s %s, want %s\n' "$3" "$4" "$5" "${1:-none}" "$2"
  fi
}

# The version a manifest declares: its own, or the workspace's.
workspace_version=$(awk '/^\[workspace.package\]/ { inside = 1; next } /^\[/ { inside = 0 } inside && /^version *=/ { gsub(/.*= *"|".*/, ""); print; exit }' Cargo.toml)
declared() {
  awk -v workspace="$workspace_version" '
    /^\[package\]/ { inside = 1; next } /^\[/ { inside = 0 }
    inside && /^version\.workspace *= *true/ { print workspace; exit }
    inside && /^version *=/ { gsub(/.*= *"|".*/, ""); print; exit }
  ' "$1"
}
name_of() {
  awk '/^\[package\]/ { inside = 1; next } /^\[/ { inside = 0 } inside && /^name *=/ { gsub(/.*= *"|".*/, ""); print; exit }' "$1"
}

# The version a plugin is released at.
plugin_version() {
  case $keep in
    *" $1 "*) declared "plugins/$1/Cargo.toml" ;;
    *) printf '%s\n' "$target" ;;
  esac
}

# The workspace and its members.
line=$(grep -n '^version *=' Cargo.toml | head -1 | cut -d: -f1)
report "$workspace_version" "$target" Cargo.toml "$line" "workspace version"
members=$(awk '/^members *=/ { gsub(/.*\[|\].*/, ""); gsub(/[", ]+/, " "); print }' Cargo.toml)
lock_names=""
for member in $members; do
  manifest=$member/Cargo.toml
  case $member in
    plugins/*/*) wanted=$target ;;
    plugins/*) wanted=$(plugin_version "${member#plugins/}") ;;
    *) wanted=$target ;;
  esac
  line=$(grep -n '^version' "$manifest" | head -1 | cut -d: -f1)
  what="$(name_of "$manifest") version"
  if grep -q '^version\.workspace *= *true' "$manifest"; then
    what="$what (the workspace's)"
  fi
  report "$(declared "$manifest")" "$wanted" "$manifest" "$line" "$what"
  lock_names="$lock_names$(name_of "$manifest")=$wanted "
done

# Cargo.lock: the members, which have no source.
awk '
  /^\[\[package\]\]/ { if (name != "" && !source) print name, version, start; name = ""; version = ""; source = 0; start = NR + 1 }
  /^name = / { gsub(/.*= "|"$/, ""); name = $0 }
  /^version = / { gsub(/.*= "|"$/, ""); version = $0; start = NR }
  /^source = / { source = 1 }
  END { if (name != "" && !source) print name, version, start }
' Cargo.lock | while read -r name version start; do
  wanted=""
  for pair in $lock_names; do
    [ "${pair%%=*}" = "$name" ] && wanted=${pair#*=}
  done
  [ -n "$wanted" ] || continue
  report "$version" "$wanted" Cargo.lock "$start" "$name locked"
done > "$tmp"
tally

# The top version heading of a changelog: the first `#` or `##` heading that is not its title.
heading() {
  awk '
    /^ *(```|~~~)/ { fence = !fence; next }
    fence { next }
    /^<!--/ { comment = 1 } comment { if (/-->/) comment = 0; next }
    /^##? / {
      word = $0; sub(/^##? +/, "", word)
      if (word ~ /^Changelog/) next
      split(word, parts, /[ \t]/); word = parts[1]
      sub(/^\[/, "", word); sub(/\].*$/, "", word)
      if (word !~ /^[0-9]+\.[0-9]+\.[0-9]+$/) word = ""
      print NR, word; exit
    }
  ' "$1"
}
set -- "CHANGELOG.md $target"
for changelog in plugins/*/CHANGELOG.md; do
  plugin=${changelog#plugins/}
  plugin=${plugin%%/*}
  set -- "$@" "$changelog $(plugin_version "$plugin")"
done
for entry in "$@"; do
  changelog=${entry% *}
  wanted=${entry##* }
  found=$(heading "$changelog")
  report "${found#* }" "$wanted" "$changelog" "${found%% *}" "top heading"
done

# The documents.
documents=$(git ls-files 'README.md' 'docs/*.md' 'plugins/*/rules/*.md' 'plugins/*/generators/*.md' \
  | grep -v -e '^docs/tasks/' -e '^docs/plan.md$')
for document in $documents; do
  awk '
    /^\[gate\]/ { gate = 1; next }
    /^\[/ { gate = 0 }
    {
      text = $0
      while (match(text, /ElsiKora\/Gate:[a-z0-9-]+@[0-9]+\.[0-9]+\.[0-9]+/)) {
        pin = substr(text, RSTART + 14, RLENGTH - 14); split(pin, parts, "@")
        print NR, "pin", parts[1], parts[2]; text = substr(text, RSTART + RLENGTH)
      }
      text = $0
      while (match(text, /plugin-[a-z0-9-]+-v[0-9]+\.[0-9]+\.[0-9]+/)) {
        tag = substr(text, RSTART + 7, RLENGTH - 7); text = substr(text, RSTART + RLENGTH)
        at = match(tag, /-v[0-9]/); print NR, "plugin-tag", substr(tag, 1, at - 1), substr(tag, at + 2)
      }
      if (match($0, /ElsiKora\/Gate\/releases\/download\/v[0-9]+\.[0-9]+\.[0-9]+\//)) {
        tag = substr($0, RSTART, RLENGTH); sub(/.*download\/v/, "", tag); sub(/\/$/, "", tag); print NR, "core-tag", "-", tag
      }
      if (match($0, /github.com\/ElsiKora\/Gate", tag = "v[0-9]+\.[0-9]+\.[0-9]+"/)) {
        tag = substr($0, RSTART, RLENGTH); sub(/.*"v/, "", tag); sub(/"$/, "", tag); print NR, "core-tag", "-", tag
      }
      if (match($0, /ElsiKora\/Gate\/v[0-9]+\.[0-9]+\.[0-9]+\//)) {
        tag = substr($0, RSTART, RLENGTH); sub(/.*Gate\/v/, "", tag); sub(/\/$/, "", tag); print NR, "core-tag", "-", tag
      }
      if (gate && match($0, /^version *= *"[0-9]+\.[0-9]+\.[0-9]+"/)) {
        v = $0; gsub(/^version *= *"|".*$/, "", v); print NR, "core-pin", "-", v
      }
      if (match($0, /^version=[0-9]+\.[0-9]+\.[0-9]+$/)) { v = $0; sub(/^version=/, "", v); print NR, "core-install", "-", v }
      if (match($0, /`v[0-9]+\.[0-9]+\.[0-9]+` is the current release/)) {
        v = substr($0, RSTART + 2); sub(/`.*$/, "", v); print NR, "current-release", "-", v
      }
      text = $0
      while (match(text, /gate:plugin@[0-9]+\.[0-9]+\.[0-9]+/)) {
        print NR, "interface", "-", substr(text, RSTART + 12, RLENGTH - 12); text = substr(text, RSTART + RLENGTH)
      }
    }
  ' "$document" | while read -r line kind plugin version; do
    case $kind in
      pin | plugin-tag)
        if [ -f "plugins/$plugin/Cargo.toml" ]; then
          report "$version" "$(plugin_version "$plugin")" "$document" "$line" "$kind $plugin"
        fi
        ;;
      interface) report "$version" "$interface" "$document" "$line" "plugin interface" ;;
      *) report "$version" "$target" "$document" "$line" "$kind" ;;
    esac
  done
done > "$tmp"
tally

# The plugin interface.
line=$(grep -n '^package gate:plugin@' wit/gate.wit | head -1 | cut -d: -f1)
found=$(sed -n "${line}p" wit/gate.wit | sed 's/.*@//; s/;.*//')
report "$found" "$interface" wit/gate.wit "$line" "plugin interface"
line=$(grep -n 'INTERFACE: &str = "gate:plugin@' crates/gate-runtime/src/runtime.rs | head -1 | cut -d: -f1)
found=$(sed -n "${line}p" crates/gate-runtime/src/runtime.rs | sed 's/.*@//; s/".*//')
report "$found" "$interface" crates/gate-runtime/src/runtime.rs "$line" "runtime INTERFACE"

# Gate's own gate.toml: moved after the release.
grep -n -E 'version = "[0-9]|ElsiKora/Gate:[a-z0-9-]+@' gate.toml | while IFS=: read -r line text; do
  printf 'later    gate.toml:%s %s\n' "$line" "$(printf '%s' "$text" | sed 's/^ *//')"
done

if [ "$mismatches" -eq 0 ]; then
  echo "release-check.sh: $places places agree with $target"
else
  echo "release-check.sh: $mismatches of $places places disagree with $target"
  exit 1
fi
