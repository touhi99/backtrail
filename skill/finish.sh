#!/bin/bash
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
MODEL="${BACKTRAIL_FINISH_MODEL:-sonnet}"
export BACKTRAIL_COMPILE=1
export PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
command -v claude >/dev/null 2>&1 || { echo "finish.sh: the claude CLI is not on PATH" >&2; exit 1; }
DRY=""; LIMIT=50; IDS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --dry) DRY=1; shift ;;
    --limit) LIMIT="$2"; shift 2 ;;
    *) IDS+=("$1"); shift ;;
  esac
done
[ ${#IDS[@]} -eq 0 ] && while IFS= read -r id; do [ -n "$id" ] && IDS+=("$id"); done < <("$TM" finish-queue | head -n "$LIMIT")
cd "$(mktemp -d)" || exit 1
for id in "${IDS[@]+"${IDS[@]}"}"; do
  packet="$("$TM" finish-input "$id")" || continue
  raw="$(printf '%s' "$packet" | claude -p --model "$MODEL" --tools "" --no-session-persistence --output-format json \
        --setting-sources project --disable-slash-commands --strict-mcp-config --max-budget-usd 0.10 \
        --system-prompt "$(cat "$DIR/finish-prompt.md")" 2>/dev/null)"
  ID="$id" TM="$TM" RAW="$raw" DRY="$DRY" DIR="$DIR" python3 - <<'PY'
import json, os, re, subprocess
i, tm, raw, dry = os.environ["ID"], os.environ["TM"], os.environ["RAW"], os.environ["DRY"]
try:
    d = json.loads(raw)
    v = json.loads(re.sub(r"^```(?:json)?|```$", "", d["result"].strip(), flags=re.M).strip())
    cost = f"usd={d.get('total_cost_usd', 0):.4f}"
except Exception as e:
    print(f"{i}: unreadable answer ({type(e).__name__})"); raise SystemExit
if not v.get("finished"):
    print(f"{i}: still open ({cost})"); raise SystemExit
src = str(v.get("source", "")).strip("[] ").split(" | ")[0]
args = [os.path.join(os.environ["DIR"], "close-step.sh"), i, src, str(v.get("quote", ""))] + (["--dry"] if dry else [])
r = subprocess.run(args, capture_output=True, text=True)
print(f"{i}: {(r.stdout or r.stderr).strip()} ({cost})")
PY
done
