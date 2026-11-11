#!/bin/bash
# SessionEnd hook: index the session that just ended, then embed new passages when embeddings are configured.
# Runs in the background and never calls a model for extraction. To also compile the session with a small
# model (costs tokens), set BACKTRAIL_COMPILE_ON_END=1 in the environment of the hook.
# Reads the hook JSON on stdin (needs session_id).
[ -n "${BACKTRAIL_COMPILE:-}" ] && exit 0
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
sid="$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("session_id",""))' 2>/dev/null)"
[ -z "$sid" ] && exit 0
(
  "$TM" ingest >/dev/null 2>&1
  "$TM" embed >/dev/null 2>&1
  if [ "${BACKTRAIL_COMPILE_ON_END:-}" = "1" ]; then
    refs="$("$TM" compile-queue --session "$sid" --limit 1)"
    [ -n "$refs" ] && "$DIR/compile.sh" $refs >> "${TMPDIR:-/tmp}/backtrail-compile.log" 2>&1
  fi
) >/dev/null 2>&1 &
disown 2>/dev/null
exit 0
