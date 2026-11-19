#!/usr/bin/env bash
# The T-001 measurement matrix. Every configuration runs RUNS times (default 5), each under
# `/usr/bin/time -l` inside a hard timeout, with the machine load (`uptime`) recorded before each run.
# The runs are single processes (three for the parallel rows); run them under `nice -n 5`.
#
#   run.sh <spike-host> <spike-plugin> <guest.wasm> <tree> <out-dir> [config-filter]
#
# With HOOK (a git checkout of the tree with one staged `.ts` change) and GATE (today's binary) set,
# the hook rows run too. A configuration whose arguments start with `@` is a shell command, not a
# spike-host call; today's binary exits non-zero on findings, so its exit code goes to `gate-exit`.
set -u
HOST=$1 PLUGIN=$2 COMPONENT=$3 TREE=$4 OUT=$5 FILTER=${6:-.}
RUNS=${RUNS:-5}
LIMIT=${LIMIT:-600}
mkdir -p "$OUT"
CWASM=$OUT/guest.cwasm
ALL=parse,walk,semantic,resolve

# name | arguments
CONFIGS=$(cat <<CONF
compile|compile --component $COMPONENT --out $CWASM
native-read|native --tree $TREE --tasks read
wasm-host-read|wasm --tree $TREE --tasks read --precompiled $CWASM --access host
wasm-fs-read|wasm --tree $TREE --tasks read --precompiled $CWASM --access fs
process-rpc-read|process --tree $TREE --tasks read --plugin $PLUGIN --access rpc
native-parse|native --tree $TREE --tasks parse
wasm-host-parse|wasm --tree $TREE --tasks parse --precompiled $CWASM --access host
wasm-fs-parse|wasm --tree $TREE --tasks parse --precompiled $CWASM --access fs
process-fs-parse|process --tree $TREE --tasks parse --plugin $PLUGIN --access fs
native-all|native --tree $TREE --tasks $ALL
native-walk|native --tree $TREE --tasks parse,walk
native-semantic|native --tree $TREE --tasks parse,semantic
native-resolve|native --tree $TREE --tasks parse,resolve
wasm-host-walk|wasm --tree $TREE --tasks parse,walk --precompiled $CWASM --access host
wasm-host-semantic|wasm --tree $TREE --tasks parse,semantic --precompiled $CWASM --access host
wasm-host-resolve|wasm --tree $TREE --tasks parse,resolve --precompiled $CWASM --access host
wasm-host-all|wasm --tree $TREE --tasks $ALL --precompiled $CWASM --access host
wasm-host-all-cold|wasm --tree $TREE --tasks $ALL --component $COMPONENT --access host
wasm-fs-all|wasm --tree $TREE --tasks $ALL --precompiled $CWASM --access fs
process-fs-all|process --tree $TREE --tasks $ALL --plugin $PLUGIN --access fs
process-rpc-all|process --tree $TREE --tasks $ALL --plugin $PLUGIN --access rpc
native-all-x3|native --tree $TREE --tasks $ALL --plugins 3
wasm-host-all-x3|wasm --tree $TREE --tasks $ALL --precompiled $CWASM --access host --plugins 3
process-fs-all-x3|process --tree $TREE --tasks $ALL --plugin $PLUGIN --access fs --plugins 3
table-native|table native --tree $TREE
table-wasm|table wasm --tree $TREE --precompiled $CWASM
table-process|table process --tree $TREE --plugin $PLUGIN
CONF
)
if [ -n "${HOOK:-}" ] && [ -n "${GATE:-}" ]; then
  CONFIGS="$CONFIGS
hook-native-all|native --tree $HOOK --tasks $ALL
hook-wasm-all|wasm --tree $HOOK --tasks $ALL --precompiled $CWASM --access host
hook-today-staged|@cd $HOOK && $GATE check --staged --no-history --timings --format text; echo \"gate-exit=\$?\" >&2"
fi

# One run of configuration `name` with arguments `args` as run number `i`.
one() {
  local name=$1 args=$2 i=$3 dir=$OUT/$1 code
  mkdir -p "$dir"
    uptime >"$dir/uptime.$i"
    # `timeout` outside `time`, so `time -l` measures the measured process itself (its
    # instructions retired and cycles too); `timeout` stops the whole process group.
    case $args in
      @*) timeout "$LIMIT" /usr/bin/time -l sh -c "${args#@}" >"$dir/out.$i" 2>"$dir/time.$i" ;;
      # shellcheck disable=SC2086
      *) timeout "$LIMIT" /usr/bin/time -l "$HOST" $args >"$dir/out.$i" 2>"$dir/time.$i" ;;
    esac
    code=$?
    echo "$code" >"$dir/exit.$i"
    echo "$(date -Iseconds) $name run $i exit $code $(sed 's/.*load averages*: //' "$dir/uptime.$i")"
}

SELECTED=$(while IFS='|' read -r name args; do
  [ -n "$name" ] && echo "$name" | grep -qE "$FILTER" && echo "$name|$args"
done <<<"$CONFIGS")

echo "matrix start $(date -Iseconds) runs=$RUNS interleave=${INTERLEAVE:-0}"
if [ -n "${INTERLEAVE:-}" ]; then
  # Round by round, every configuration once per round, so rows compared with each other share
  # the machine's load: instructions retired grow with load (the kernel side of file reads).
  for i in $(seq 1 "$RUNS"); do
    while IFS='|' read -r name args; do one "$name" "$args" "$i" </dev/null; done <<<"$SELECTED"
  done
else
  while IFS='|' read -r name args; do
    for i in $(seq 1 "$RUNS"); do one "$name" "$args" "$i" </dev/null; done
  done <<<"$SELECTED"
fi
echo "matrix done $(date -Iseconds)"
