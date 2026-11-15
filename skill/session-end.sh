#!/bin/bash
# SessionEnd hook: index the session that just ended, embed new passages, then extract and resolve it.
# Runs in the background. Extraction (extract.sh) and resolution (resolve.sh) each make small model calls, (extraction is capped by `daily_cap` sessions a day); set BACKTRAIL_COMPILE_ON_END=0 in the environment of the hook to skip both.
# Reads the hook JSON on stdin (needs session_id).
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
    # opt-in: judge whether new settled decisions reverse older ones (two model calls per reversal found)
    [ "${BACKTRAIL_REVERSE_ON_END:-0}" = "1" ] && "$DIR/resolve.sh" --reverses --limit 5 >> "${TMPDIR:-/tmp}/backtrail-extract.log" 2>&1
  fi
) >/dev/null 2>&1 &
disown 2>/dev/null
exit 0
