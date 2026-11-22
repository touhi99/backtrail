#!/bin/bash
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
MODEL="${BACKTRAIL_CORRECTIONS_MODEL:-haiku}"
LABEL="$MODEL@2"
export BACKTRAIL_COMPILE=1
export PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
BATCHES=5; START=40; DRY=
while [ $# -gt 0 ]; do
  case "$1" in
    --dry) DRY=1; shift ;;
    --batches) BATCHES="$2"; shift 2 ;;
    --size) START="$2"; shift 2 ;;
    *) echo "usage: corrections.sh [--dry] [--batches N] [--size N]" >&2; exit 2 ;;
  esac
done
"$DIR/ask.py" --check >/dev/null || exit 1
cd "$(mktemp -d)" || exit 1

pass() {
  local mode="$1" prompt="$2" size="$START" packet raw out id
  for _ in $(seq "$BATCHES"); do
    packet="$("$TM" corrections-packet --limit "$size" --model "$LABEL" $mode)"
    [ -z "$packet" ] && { echo "${mode:-tags}: nothing left"; return; }
    raw="$(printf '%s' "$packet" | "$DIR/ask.py" --tier small --model "$MODEL" --budget 0.10 --system "$DIR/$prompt")" || { echo "model call failed"; return; }
    out="$(DRY="$DRY" TM="$TM" RAW="$raw" PACKET="$packet" LABEL="$LABEL" MODE="$mode" python3 - <<'PY'
import json, os, re, subprocess, sys
raw, packet = os.environ["RAW"], os.environ["PACKET"]
turns = [int(m) for m in re.findall(r"^TURN (\d+)$", packet, flags=re.M)]
try:
    d = json.loads(raw)
    cost = f"usd={d.get('total_cost_usd', 0):.4f}"
    v = json.loads(re.sub(r"^```(?:json)?|```$", "", d["result"].strip(), flags=re.M).strip())
    if os.environ["MODE"]:
        body = {"kinds": {re.sub(r"\D", "", k): x for k, x in v["kinds"].items() if re.sub(r"\D", "", k)}}
    else:
        body = {"model": os.environ["LABEL"], "turns": turns, "corrections": [int(i) for i in v["corrections"] if int(i) in turns]}
except Exception as e:
    print(f"failed ({type(e).__name__})"); sys.exit(1)
if os.environ.get("DRY"):
    print(json.dumps(dict(body, _cost=cost))); sys.exit(1)
r = subprocess.run([os.environ["TM"], "corrections-apply"], input=json.dumps(body), capture_output=True, text=True)
print(f"{cost} · {(r.stdout or r.stderr).strip()}")
PY
)" && { echo "$out"; continue; }
    echo "$out"
    [ -n "$DRY" ] && return
    if [ "$size" -gt 1 ]; then size=$(( size > 5 ? 5 : 1 )); continue; fi
    id="$(printf '%s' "$packet" | sed -n 's/^TURN \([0-9]*\)$/\1/p' | head -1)"
    if [ -n "$mode" ]; then
      printf '{"kinds":{"%s":"unsorted"}}' "$id" | "$TM" corrections-apply >/dev/null
    else
      printf '{"model":"%s","turns":[%s],"corrections":[]}' "$LABEL" "$id" | "$TM" corrections-apply >/dev/null
    fi
    echo "turn $id skipped after repeated failures"
  done
}

pass "" corrections-prompt.md
pass --kinds corrections-kind-prompt.md
