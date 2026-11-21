#!/bin/bash
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
export PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
[ "${BACKTRAIL_COMPILE_ON_END:-1}" = "0" ] && exit 0
HOME_DIR="$("$TM" home)" || exit 1
LOCK="$HOME_DIR/sweep.lock"
if ! mkdir "$LOCK" 2>/dev/null; then
  if [ -n "$(find "$LOCK" -maxdepth 0 -mmin +180 2>/dev/null)" ]; then
    rmdir "$LOCK" 2>/dev/null
    mkdir "$LOCK" 2>/dev/null || exit 0
  else
    exit 0
  fi
fi
trap 'rmdir "$LOCK" 2>/dev/null' EXIT
LOG="$HOME_DIR/logs/sweep.log"
mkdir -p "$HOME_DIR/logs"
{
  echo "== sweep $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  for conv in "$HOME_DIR"/converters/*; do
    [ -x "$conv" ] || continue
    "$conv" "$HOME_DIR/inbox" || echo "converter failed: $conv"
  done
  "$TM" ingest >/dev/null 2>&1
  "$TM" embed >/dev/null 2>&1
  for _ in 1 2 3; do
    refs=()
    while IFS= read -r r; do [ -n "$r" ] && refs+=("$r"); done < <("$TM" extract-queue --no-cap --limit 1000)
    [ ${#refs[@]} -eq 0 ] && break
    "$DIR/extract.sh" "${refs[@]}"
  done
  for _ in 1 2 3; do
    [ -z "$("$TM" resolve-queue --limit 1)" ] && break
    "$DIR/resolve.sh" --limit 1000
  done
  for _ in 1 2 3; do
    [ -z "$("$TM" resolve-queue --reverses --limit 1)" ] && break
    "$DIR/resolve.sh" --reverses --limit 1000
  done
  "$TM" prs >/dev/null 2>&1
  "$DIR/finish.sh" --limit 15
  "$TM" ingest >/dev/null 2>&1
  "$TM" status | head -1
} >> "$LOG" 2>&1
