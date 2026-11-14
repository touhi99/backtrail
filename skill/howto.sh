#!/bin/bash
# Write "how to check" text for steps waiting on the owner, one single-shot call per step.
# Usage: howto.sh [--dry] [--again] [--limit N] | howto.sh [--dry] <step id>...
# --dry prints each verdict as a JSON line and applies nothing.
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
MODEL="${BACKTRAIL_HOWTO_MODEL:-haiku}"
export BACKTRAIL_COMPILE=1   # the SessionStart and SessionEnd hooks exit at once while this is set
LIMIT=30; IDS=(); DRY=; AGAIN=
while [ $# -gt 0 ]; do
  case "$1" in
    --dry) DRY=1; shift ;;
    --again) AGAIN=--again; shift ;;   # also steps an earlier run judged to need no hand check
    --limit) LIMIT="$2"; shift 2 ;;
    *) IDS+=("$1"); shift ;;
  esac
done
if [ ${#IDS[@]} -eq 0 ]; then
  while IFS= read -r i; do [ -n "$i" ] && IDS+=("$i"); done < <("$TM" howto-queue --limit "$LIMIT" $AGAIN)
fi
cd "$(mktemp -d)" || exit 1  # no project CLAUDE.md or session history is loaded here
for id in "${IDS[@]+"${IDS[@]}"}"; do
  packet="$("$TM" howto-packet "$id")" || { echo "$id: no packet"; continue; }
  raw="$(printf '%s' "$packet" | claude -p --model "$MODEL" --tools "" --no-session-persistence --output-format json \
        --setting-sources project --disable-slash-commands --strict-mcp-config --max-budget-usd 0.10 \
        --system-prompt "$(cat "$DIR/howto-prompt.md")" 2>/dev/null)"
  DRY="$DRY" ID="$id" TM="$TM" RAW="$raw" python3 - <<'PY'
import json, os, re, subprocess, sys
id_, tm, raw = os.environ["ID"], os.environ["TM"], os.environ["RAW"]
try:
    d = json.loads(raw)
    cost = f"usd={d.get('total_cost_usd', 0):.4f}"
    v = json.loads(re.sub(r"^```(?:json)?|```$", "", d["result"].strip(), flags=re.M).strip())
    assert isinstance(v, dict)
    v["id"] = id_
except Exception as e:
    print(f"{id_}: failed ({type(e).__name__})"); sys.exit(0)
if os.environ.get("DRY"):
    print(json.dumps(dict(v, _cost=cost))); sys.exit(0)
r = subprocess.run([tm, "howto-apply"], input=json.dumps(v), capture_output=True, text=True)
print(f"{id_}: {cost} · {(r.stdout or r.stderr).strip()}")
PY
done
