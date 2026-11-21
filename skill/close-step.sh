#!/bin/bash
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
VMODEL="${BACKTRAIL_VERIFY_MODEL:-opus}"
id="$1"; src="$2"; quote="$3"; dry="${4:-}"
first="$("$TM" finish "$id" --source "$src" --quote "$quote" --verified --dry 2>&1)" || { echo "rejected: $first"; exit 0; }
verified=()
case "$first" in
  rejected*|*"not open"*) echo "$first"; exit 0 ;;
  *)
    packet="$("$TM" finish-evidence "$id" --source "$src" --quote "$quote" 2>/dev/null)"
    answer="$(cd "$(mktemp -d)" && printf '%s' "$packet" | "$DIR/ask.py" --text --tier large --model "$VMODEL" --budget 0.10 --system "$DIR/confirm-finished-prompt.md" 2>/dev/null | tr -d '[:space:][:punct:]' | tr '[:upper:]' '[:lower:]')"
    [ "$answer" = "yes" ] && verified=(--verified) ;;
esac
if [ "$dry" = "--dry" ]; then
  "$TM" finish "$id" --source "$src" --quote "$quote" "${verified[@]+"${verified[@]}"}" --dry 2>&1
else
  "$TM" finish "$id" --source "$src" --quote "$quote" "${verified[@]+"${verified[@]}"}" 2>&1
fi
