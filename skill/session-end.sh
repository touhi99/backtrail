#!/bin/bash
[ -n "${BACKTRAIL_COMPILE:-}" ] && exit 0
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
sid="$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("session_id",""))' 2>/dev/null)"
[ -z "$sid" ] && exit 0
(
  "$TM" ingest >/dev/null 2>&1
  "$TM" embed >/dev/null 2>&1
  if [ "${BACKTRAIL_COMPILE_ON_END:-1}" != "0" ]; then
    "$DIR/extract.sh" --session "$sid" --limit 1 >> "${TMPDIR:-/tmp}/backtrail-extract.log" 2>&1
    "$DIR/resolve.sh" --limit 20 >> "${TMPDIR:-/tmp}/backtrail-extract.log" 2>&1
    "$DIR/finish.sh" --limit 15 >> "${TMPDIR:-/tmp}/backtrail-extract.log" 2>&1
    [ "${BACKTRAIL_REVERSE_ON_END:-0}" = "1" ] && "$DIR/resolve.sh" --reverses --limit 5 >> "${TMPDIR:-/tmp}/backtrail-extract.log" 2>&1
  fi
) >/dev/null 2>&1 &
disown 2>/dev/null
exit 0
