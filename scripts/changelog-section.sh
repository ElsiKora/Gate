#!/bin/sh
# changelog-section.sh <changelog> <version>: prints the section of <version> in <changelog>, the
# release notes of its tag. The section starts at an `#` or `##` heading whose first word is
# <version> or `[<version>](<link>)` and ends before the next `#` or `##` heading; `###` headings
# (Features, Bug Fixes) belong to it, and so does a line inside a fenced code block (``` or ~~~),
# whatever it starts with. Blank lines around it are dropped. Fails when the changelog has no such
# section or it is empty.
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: scripts/changelog-section.sh <changelog> <version>" >&2
  exit 2
fi
changelog=$1
version=$2

if [ ! -f "$changelog" ]; then
  echo "changelog-section.sh: $changelog does not exist; add it with a section for $version" >&2
  exit 1
fi

status=0
section=$(awk -v version="$version" '
  /^ *(```|~~~)/ {
    marker = $0
    sub(/^ */, "", marker)
    marker = substr(marker, 1, 1)
    if (fence == "") { fence = marker } else if (fence == marker) { fence = "" }
    if (inside) { print }
    next
  }
  fence == "" && /^##? / {
    if (inside) { exit }
    word = $0
    sub(/^##? +/, "", word)
    sub(/[ \t].*$/, "", word)
    if (word == version || word == "[" version "]" || index(word, "[" version "](") == 1) {
      inside = 1
      found = 1
    }
    next
  }
  inside { print }
  END { if (!found) { exit 3 } }
' "$changelog") || status=$?

if [ "$status" -eq 3 ]; then
  echo "changelog-section.sh: $changelog has no section for $version; add a \"## $version\" heading (\"#\" for a minor or major release) with its changes" >&2
  exit 1
elif [ "$status" -ne 0 ]; then
  echo "changelog-section.sh: cannot read $changelog" >&2
  exit 1
fi

# Drops the blank lines before the first line of text; $(...) dropped the ones after the last.
section=$(printf '%s\n' "$section" | sed '/[^[:space:]]/,$!d')
if [ -z "$section" ]; then
  echo "changelog-section.sh: the section for $version in $changelog is empty; write its changes" >&2
  exit 1
fi
printf '%s\n' "$section"
