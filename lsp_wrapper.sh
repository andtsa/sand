#!/usr/bin/env bash
set -euo pipefail

DEBUG_BIN="target/debug/lsp"
RELEASE_BIN="target/release/lsp"

LSP_ARGS=("$@")

# ----- helpers -----

latest_bin() {
  local d="$DEBUG_BIN" r="$RELEASE_BIN"
  local d_ok=0 r_ok=0
  [[ -x "$d" ]] && d_ok=1
  [[ -x "$r" ]] && r_ok=1

  if [[ $d_ok -eq 0 && $r_ok -eq 0 ]]; then
    echo ""
    return 0
  fi
  if [[ $d_ok -eq 1 && $r_ok -eq 0 ]]; then
    echo "$d"; return 0
  fi
  if [[ $d_ok -eq 0 && $r_ok -eq 1 ]]; then
    echo "$r"; return 0
  fi

  # Both exist: pick the newer mtime (BSD stat)
  local dt rt
  dt=$(stat -f %m "$d")
  rt=$(stat -f %m "$r")
  if (( rt >= dt )); then echo "$r"; else echo "$d"; fi
}

# Wait until file stops changing (mtime+size stable twice)
wait_stable() {
  local f="$1"
  local m1 s1 m2 s2
  while true; do
    [[ -x "$f" ]] || { sleep 0.1; continue; }
    m1=$(stat -f %m "$f") || { sleep 0.1; continue; }
    s1=$(stat -f %z "$f") || { sleep 0.1; continue; }
    sleep 0.12
    m2=$(stat -f %m "$f") || { sleep 0.1; continue; }
    s2=$(stat -f %z "$f") || { sleep 0.1; continue; }
    [[ "$m1" == "$m2" && "$s1" == "$s2" ]] && return 0
  done
}

kill_child() {
  local pid="${1:-}"
  [[ -n "${pid}" ]] || return 0
  kill -TERM "$pid" 2>/dev/null || true
  # give it a moment
  for _ in {1..20}; do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.05
  done
  kill -KILL "$pid" 2>/dev/null || true
}

# ----- main supervisor -----

if ! command -v fswatch >/dev/null 2>&1; then
  echo "error: fswatch not found. Install with: brew install fswatch" >&2
  exit 1
fi

child_pid=""

start_server() {
  local bin
  bin="$(latest_bin)"
  if [[ -z "$bin" ]]; then
    echo "wrapper: no executable at $DEBUG_BIN or $RELEASE_BIN yet" >&2
    return 1
  fi

  wait_stable "$bin"
  echo "wrapper: starting $bin" >&2

  # Start server with stdio connected to nvim
  "$bin" "${LSP_ARGS[@]}" &
  child_pid=$!
  return 0
}

# Start initial server (block until one exists)
until start_server; do
  sleep 0.2
done

# Watch for changes to either binary path (create/rename/write)
# -0 makes it NUL-delimited, safer for paths, but we don't need the path text anyway.
fswatch -0 "$DEBUG_BIN" "$RELEASE_BIN" 2>/dev/null | while IFS= read -r -d '' _; do
  # On any event, pick newest and restart
  newbin="$(latest_bin)"
  [[ -n "$newbin" ]] || continue

  # If the newest is still the same *file* and hasn't changed, this is harmless;
  # restart anyway to keep logic simple, but only after stable.
  wait_stable "$newbin"

  echo "wrapper: restart triggered; switching to $newbin" >&2
  kill_child "$child_pid"
  "$newbin" "${LSP_ARGS[@]}" &
  child_pid=$!
done
