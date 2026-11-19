#!/bin/sh
# bin/gate: runs the Gate core pinned in gate.lock. Written by `gate init`; commit it.
#
# It reads the URL and sha256 of the core archive for this machine from gate.lock, downloads it
# once into the shared cache ($GATE_CACHE_DIR, by default the user cache directory), verifies the
# checksum, unpacks the core and runs it with the arguments given. Once cached, no network is
# needed. A failure exits with code 2 and installs nothing.
#
# Before each run the cached core is checked again: it must be a regular file, not a link, and
# hash to the sha256 recorded when it was installed; anything else is installed again.
#
# Every URL, and every redirect hop, must be https (plain http only to this machine) and name no
# user. A private release is fetched the way the core fetches a private plugin: anonymously
# first; when GitHub refuses, through the API with a token. github.com takes $GATE_GITHUB_TOKEN,
# then $GITHUB_TOKEN, then `gh auth token`; a GitHub Enterprise named by both $GATE_GITHUB_URL and
# $GATE_GITHUB_API_URL takes the variables instead, and github.com then only `gh auth token`. The
# token goes to curl in a file only this user can read, never on a command line, and only to the
# release assets of the repository on that GitHub's API; a redirect from the API is followed
# without it.
#
# Every download is bounded: 30 s to connect, it fails when no data arrives for 30 s, and the
# whole download, every redirect hop and request included, has 15 minutes (curl only: wget has
# no overall limit). It is made once; a failure says to try again, and nothing is retried.
set -eu

fail() {
	printf 'gate: environment error: %s\n' "$1" >&2
	exit 2
}

# fetch CURL-ARGUMENTS: one bounded curl request (see the header), https or http only, with
# what is left of the download's 15 minutes ($deadline, in seconds since the epoch).
fetch() {
	left=$((deadline - $(date +%s)))
	[ "$left" -gt 0 ] || fail "cannot download $asset from $url: not finished within 15 minutes; try again"
	curl --connect-timeout 30 --speed-limit 1 --speed-time 30 --max-time "$left" --silent --show-error --proto '=https,http' "$@"
}

# checked URL WHAT: fails, naming WHAT, unless URL is https, or plain http to 127.0.0.1,
# localhost or [::1] with a numeric port, and names no user.
checked() {
	authority=${1#*://}
	authority=${authority%%/*}
	authority=${authority%%\?*}
	authority=${authority%%#*}
	case $authority in
	'' | *[!]0-9A-Za-z.:[-]*) fail "$2 $1, which has no host, or names a user (@) or a character no host has; run gate lock" ;;
	esac
	case $1 in
	https://*) ;;
	http://*)
		case $authority in
		127.0.0.1 | localhost | "[::1]") ;;
		127.0.0.1:* | localhost:* | "[::1]":*)
			case ${authority##*:} in
			'' | *[!0-9]*) fail "$2 $1, whose port is not a number; run gate lock" ;;
			esac
			;;
		*) fail "$2 $1, which is not an https URL; bin/gate downloads over https only (plain http only to this machine)" ;;
		esac
		;;
	*) fail "$2 $1, which is not an https URL; bin/gate downloads over https only (plain http only to this machine)" ;;
	esac
}

# digest FILE: prints the sha256 of FILE with the tool found at the start.
digest() {
	sum=$($hasher "$1") || return 1
	printf '%s\n' "${sum%% *}"
}

# anonymous URL FILE: downloads URL into FILE without a credential and sets status to the
# final HTTP status. Follows up to 10 redirects itself, checking each hop like the first URL.
anonymous() {
	hop=$1
	hops=0
	while :; do
		checked "$hop" "the download of $asset leads to"
		answer=$(fetch --output "$2" --write-out '%{http_code} %{redirect_url}' "$hop") ||
			return 1
		status=${answer%% *}
		case $status in
		301 | 302 | 303 | 307 | 308) ;;
		*) return 0 ;;
		esac
		hops=$((hops + 1))
		[ "$hops" -le 10 ] || fail "cannot download $asset from $url: more than 10 redirects; check the URL gate.lock pins"
		hop=${answer#* }
		[ -n "$hop" ] || fail "cannot download $asset from $url: a redirect gives no Location; check the URL gate.lock pins"
	done
}

# credential: sets token, and from to where it came from. The variables are tried when they
# belong to this GitHub ($variables is yes), then `gh auth token` for its host. Fails when there
# is none.
credential() {
	if [ "$variables" = yes ] && [ -n "${GATE_GITHUB_TOKEN:-}" ]; then
		token=$GATE_GITHUB_TOKEN
		from=\$GATE_GITHUB_TOKEN
	elif [ "$variables" = yes ] && [ -n "${GITHUB_TOKEN:-}" ]; then
		token=$GITHUB_TOKEN
		from=\$GITHUB_TOKEN
	elif command -v gh >/dev/null 2>&1 && token=$(gh auth token --hostname "${web#*://}" 2>/dev/null </dev/null) && [ -n "$token" ]; then
		from='gh auth token'
	else
		return 1
	fi
	case $token in
	*[!A-Za-z0-9_.-]*) fail "$from holds a character no GitHub token has; set GATE_GITHUB_TOKEN to a token that can read $owner/$repo" ;;
	esac
}

# private STATUS: GitHub refused the anonymous download with STATUS. When the core is a release
# asset of github.com or of the Enterprise, downloads it through that GitHub's API with a
# credential into $work/$asset.
private() {
	if [ -n "$enterprise_web" ] && [ "${url#"$enterprise_web"/}" != "$url" ]; then
		web=$enterprise_web
		api=$enterprise_api
		variables=yes
	elif [ "${url#https://github.com/}" != "$url" ]; then
		web=https://github.com
		api=https://api.github.com
		if [ -n "$enterprise_web" ]; then variables=no; else variables=yes; fi
	else
		fail "cannot download $asset from $url: the server answered HTTP $1, and the URL is not a release of github.com${enterprise_web:+ or $enterprise_web}, so bin/gate has no credential to try"
	fi
	rest=${url#"$web"/}
	owner=${rest%%/*}
	rest=${rest#*/}
	repo=${rest%%/*}
	case $rest in
	"$repo"/releases/download/?*/?*) rest=${rest#"$repo"/releases/download/} ;;
	*) rest= ;;
	esac
	tag=${rest%%/*}
	name=${rest#*/}
	case "$owner/$repo/$tag/$name" in
	/* | */ | *//* | */*/*/*/*) fail "cannot download $asset from $url: the server answered HTTP $1, and the URL is not a release asset of $web" ;;
	esac
	if [ "$variables" = yes ]; then
		help="Set GATE_GITHUB_TOKEN (or GITHUB_TOKEN, or sign in with gh auth login)"
	else
		help="Sign in to ${web#*://} with gh auth login (GATE_GITHUB_TOKEN and GITHUB_TOKEN belong to $enterprise_web)"
	fi
	credential ||
		fail "cannot download $asset from $url: the release is private or missing (HTTP $1). $help to a token that can read $owner/$repo, then run bin/gate again"
	headers=$work/headers
	(umask 077 && printf 'Authorization: Bearer %s\nX-GitHub-Api-Version: 2022-11-28\n' "$token" >"$headers") ||
		fail "cannot write a private file in $work"
	release=$api/repos/$owner/$repo/releases/tags/$tag
	status=$(fetch -H "@$headers" -H 'Accept: application/vnd.github+json' \
		--output "$work/release.json" --write-out '%{http_code}' "$release") ||
		fail "cannot read $release; try again, and check the network (and HTTPS_PROXY, if one is needed)"
	[ "$status" = 200 ] ||
		fail "cannot download $asset: $api refused $from for release $tag of $owner/$repo (HTTP $status). $help to a token that can read $owner/$repo, then run bin/gate again"
	# The API URL of the asset named $name: its "url" and "name" sit in one stretch of the JSON
	# between braces (GitHub nests only the uploader, after both). GitHub pretty-prints the JSON
	# for curl, one key per line, so the line breaks go first (a JSON string holds none) and every
	# brace ends a record.
	prefix=$api/repos/$owner/$repo/releases/assets/
	located=$(tr -d '\r\n' <"$work/release.json" | tr '{}' '[\n*]' | awk -v name="$name" -v prefix="$prefix" '
		function value(field) {
			sub(/^"[a-z_]+"[ \t\r]*:[ \t\r]*"/, "", field)
			sub(/"[ \t\r]*$/, "", field)
			return field
		}
		{
			url = ""
			named = 0
			count = split($0, fields, ",")
			for (i = 1; i <= count; i++) {
				field = fields[i]
				sub(/^[ \t\r\[]+/, "", field)
				if (field ~ /^"url"[ \t\r]*:/) url = value(field)
				if (field ~ /^"name"[ \t\r]*:/ && value(field) == name) named = 1
			}
			if (named && index(url, prefix) == 1 && substr(url, length(prefix) + 1) ~ /^[0-9]+$/) { print url; exit }
		}
	') || located=
	[ -n "$located" ] ||
		fail "release $tag of $owner/$repo has no asset $name; run gate lock with a released core, then commit gate.lock"
	answer=$(fetch -H "@$headers" -H 'Accept: application/octet-stream' \
		--output "$work/$asset" --write-out '%{http_code} %{redirect_url}' "$located") ||
		fail "cannot download $asset from $located; try again, and check the network (and HTTPS_PROXY, if one is needed)"
	rm -f "$headers"
	status=${answer%% *}
	case $status in
	200) ;;
	301 | 302 | 303 | 307 | 308)
		# The storage behind the redirect gets no token.
		anonymous "${answer#* }" "$work/$asset" ||
			fail "cannot download $asset from the storage $located redirects to; try again, and check the network"
		[ "$status" = 200 ] || fail "cannot download $asset: the storage $located redirects to answered HTTP $status; try again"
		;;
	*) fail "cannot download $asset: $api refused $from for $located (HTTP $status). $help to a token that can read $owner/$repo, then run bin/gate again" ;;
	esac
}

# installed: the cached core is a regular file in a real directory, not a link, executable, and
# hashes to the sha256 recorded when it was installed.
installed() {
	[ -d "$directory" ] && [ ! -L "$directory" ] || return 1
	[ -f "$core" ] && [ ! -L "$core" ] && [ -x "$core" ] || return 1
	[ -f "$core.sha256" ] && [ ! -L "$core.sha256" ] || return 1
	recorded=$(cat "$core.sha256") || return 1
	[ -n "$recorded" ] && [ "$(digest "$core")" = "$recorded" ]
}

launcher=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P) || fail "cannot find the directory of $0"
root=$(dirname -- "$launcher")
lock=$root/gate.lock
[ -f "$lock" ] || fail "there is no gate.lock in $root; run gate lock with a downloaded core, then commit gate.lock"

system=$(uname -s)
machine=$(uname -m)
case "$system $machine" in
"Darwin arm64") target=aarch64-apple-darwin ;;
"Linux x86_64") target=x86_64-unknown-linux-musl ;;
*) fail "no Gate core is released for $system $machine; the released targets are aarch64-apple-darwin and x86_64-unknown-linux-musl" ;;
esac
asset=gate-$target.tar.gz

pin=$(awk -v section="[core.targets.$target]" '
	$0 == section { inside = 1; next }
	/^\[/ { inside = 0 }
	inside && $1 == "url" && $2 == "=" { url = $3 }
	inside && $1 == "sha256" && $2 == "=" { sum = $3 }
	END {
		gsub(/"/, "", url)
		gsub(/"/, "", sum)
		if (url != "" && sum != "") print url " " sum
	}
' "$lock") || fail "cannot read $lock"
[ -n "$pin" ] || fail "gate.lock pins no core for $target; run gate lock and commit gate.lock"
url=${pin% *}
sha256=${pin#* }

case $sha256 in
*[!0-9a-f]*) fail "gate.lock pins the core with sha256 $sha256, which is not lowercase hexadecimal; run gate lock" ;;
esac
[ ${#sha256} -eq 64 ] || fail "gate.lock pins the core with sha256 $sha256, which is not 64 digits; run gate lock"
checked "$url" "gate.lock pins the core at"

if command -v sha256sum >/dev/null 2>&1; then
	hasher=sha256sum
elif command -v shasum >/dev/null 2>&1; then
	hasher='shasum -a 256'
else
	fail "neither sha256sum nor shasum is installed; install one so bin/gate can verify the core"
fi

if [ -n "${GATE_CACHE_DIR:-}" ]; then
	cache=$GATE_CACHE_DIR
	case $cache in
	/*) ;;
	*) fail "GATE_CACHE_DIR=$cache is not an absolute path; the cache is shared by every worktree, so name it from the root" ;;
	esac
elif [ "$system" = Darwin ]; then
	[ -n "${HOME:-}" ] || fail "no cache directory: set GATE_CACHE_DIR to an absolute path, or HOME"
	cache=$HOME/Library/Caches/gate
else
	case ${XDG_CACHE_HOME:-} in
	/*) cache=$XDG_CACHE_HOME/gate ;;
	*)
		[ -n "${HOME:-}" ] || fail "no cache directory: set GATE_CACHE_DIR to an absolute path, or HOME"
		cache=$HOME/.cache/gate
		;;
	esac
fi

directory=$cache/core/$sha256
core=$directory/gate
if ! installed; then
	mkdir -p "$cache/tmp" || fail "cannot create $cache/tmp; check that the cache directory is writable, or set GATE_CACHE_DIR"
	if [ ! -f "$cache/CACHEDIR.TAG" ]; then
		printf '%s\n' 'Signature: 8a477f597d28d172789f06886806bc55' '# This file is a cache directory tag created by gate.' \
			'# For information about cache directory tags see https://bford.info/cachedir/' >"$cache/CACHEDIR.TAG" ||
			fail "cannot write $cache/CACHEDIR.TAG; check that the cache directory is writable, or set GATE_CACHE_DIR"
	fi
	work=$(mktemp -d "$cache/tmp/core.XXXXXX") || fail "cannot create a temporary directory in $cache/tmp"
	trap 'rm -rf "$work"' EXIT
	trap 'exit 2' HUP INT TERM

	enterprise_web=${GATE_GITHUB_URL:-}
	enterprise_api=${GATE_GITHUB_API_URL:-}
	enterprise_web=${enterprise_web%/}
	enterprise_api=${enterprise_api%/}
	if [ -n "$enterprise_web$enterprise_api" ] && { [ -z "$enterprise_web" ] || [ -z "$enterprise_api" ]; }; then
		fail "GATE_GITHUB_URL and GATE_GITHUB_API_URL name a GitHub Enterprise together, and only one is set; set both, or neither for github.com"
	fi
	deadline=$(($(date +%s) + 900))
	if command -v curl >/dev/null 2>&1; then
		anonymous "$url" "$work/$asset" ||
			fail "cannot download $asset from $url; try again, and check the network (and HTTPS_PROXY, if one is needed)"
		case $status in
		200) ;;
		401 | 403 | 404) private "$status" ;;
		*) fail "cannot download $asset from $url: the server answered HTTP $status; try again, or check the URL gate.lock pins" ;;
		esac
	elif command -v wget >/dev/null 2>&1; then
		# wget cannot check each redirect hop: it gets https only, the first URL included.
		case $url in
		https://*) ;;
		*) fail "cannot download $asset from $url with wget, which bin/gate uses for https only; install curl" ;;
		esac
		wget --quiet --https-only --max-redirect=10 --timeout=30 --tries=1 --output-document="$work/$asset" "$url" ||
			fail "cannot download $asset from $url; try again, and check the network (and HTTPS_PROXY, if one is needed). If the release is private, install curl: bin/gate sends a token with curl only"
	else
		fail "neither curl nor wget is installed; install one so bin/gate can download the core"
	fi

	actual=$(digest "$work/$asset") || fail "cannot hash $work/$asset"
	[ "$actual" = "$sha256" ] ||
		fail "checksum mismatch for $asset from $url: gate.lock pins sha256 $sha256, the download has sha256 $actual; nothing was installed. Do not use the download; if the release was replaced on purpose, pin it again with gate lock"

	mkdir "$work/unpacked" || fail "cannot create $work/unpacked"
	tar -xzf "$work/$asset" -C "$work/unpacked" || fail "cannot unpack $asset from $url"
	[ -f "$work/unpacked/gate" ] && [ ! -L "$work/unpacked/gate" ] || fail "$asset from $url has no gate binary at its top"
	chmod 755 "$work/unpacked/gate" || fail "cannot make $work/unpacked/gate executable"
	digest "$work/unpacked/gate" >"$work/unpacked/gate.sha256" || fail "cannot hash $work/unpacked/gate"
	if [ -L "$directory" ]; then
		rm -f "$directory" || fail "$directory is a symbolic link and cannot be removed; remove it by hand"
	fi
	mkdir -p "$directory" || fail "cannot create $directory"
	# Renames within the cache: a concurrent first run installs the same bytes.
	mv -f "$work/unpacked/gate.sha256" "$core.sha256" || fail "cannot install the core into $directory"
	mv -f "$work/unpacked/gate" "$core" || fail "cannot install the core into $directory"
	rm -rf "$work"
	trap - EXIT HUP INT TERM
	installed || fail "the core installed in $directory does not verify; remove $directory and run bin/gate again"
fi

exec "$core" "$@"
