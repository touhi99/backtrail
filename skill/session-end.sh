#!/bin/bash
[ -n "${BACKTRAIL_COMPILE:-}" ] && exit 0
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
sid="$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("session_id",""))' 2>/dev/null)"
[ -z "$sid" ] && exit 0
(
  "$TM" ingest >/dev/null 2>&1
  "$TM" ended "$sid" >/dev/null 2>&1
  "$DIR/sweep.sh"
) >/dev/null 2>&1 &
disown 2>/dev/null
exit 0
