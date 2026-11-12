#!/bin/bash
# Extract claims from sessions with a small model, one single-shot call per session (tier 1; with --entity, tier 2).
# Usage: kg-extract.sh [--entity NAME] [--limit N] [--session ID] | kg-extract.sh <session ref>...
# The backtrail binary never calls a model: `kg extract-input` is piped to a headless model and the answer goes
# back through `kg claims`, so the verbatim-quote, known-entity and speaker rules gate every claim.
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
MODEL="${BACKTRAIL_COMPILE_MODEL:-haiku}"
export BACKTRAIL_COMPILE=1
ENTITY=""; LIMIT=3; SESSION=""; REFS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --entity) ENTITY="$2"; shift 2 ;;
    --limit) LIMIT="$2"; shift 2 ;;
    --session) SESSION="$2"; shift 2 ;;
    *) REFS+=("$1"); shift ;;
  esac
done
if [ ${#REFS[@]} -eq 0 ]; then
  args=(kg extract-queue --limit "$LIMIT")
  [ -n "$ENTITY" ] && args+=(--entity "$ENTITY")
  [ -n "$SESSION" ] && args+=(--session "$SESSION")
  while IFS= read -r r; do [ -n "$r" ] && REFS+=("$r"); done < <("$TM" "${args[@]}")
fi
cd "$(mktemp -d)" || exit 1
for ref in "${REFS[@]}"; do
  packet="$("$TM" kg extract-input "$ref")" || { echo "$ref: no packet"; continue; }
  raw="$(printf '%s' "$packet" | claude -p --model "$MODEL" --tools "" --no-session-persistence --output-format json \
        --setting-sources project --disable-slash-commands --strict-mcp-config --max-budget-usd 0.25 \
        --system-prompt "$(cat "$DIR/kg-extract-prompt.md")" 2>/dev/null)"
  REF="$ref" TM="$TM" RAW="$raw" python3 - <<'PY'
import json, os, re, subprocess, sys
ref, tm, raw = os.environ["REF"], os.environ["TM"], os.environ["RAW"]
def log(ok, detail):
    subprocess.run([tm, "kg", "log", ref, "--detail", detail] + ([] if ok else ["--failed", "1"]), capture_output=True)
try:
    d = json.loads(raw)
    u = d.get("usage", {})
    cost = f"usd={d.get('total_cost_usd', 0):.4f} in={u.get('input_tokens', 0) + u.get('cache_creation_input_tokens', 0) + u.get('cache_read_input_tokens', 0)} out={u.get('output_tokens', 0)}"
    text = re.sub(r"^```(?:json)?|```$", "", d["result"].strip(), flags=re.M).strip()
    items = json.loads(text)
    assert isinstance(items, list)
except Exception as e:
    log(False, f"unreadable answer: {type(e).__name__}")
    print(f"{ref}: failed ({type(e).__name__})"); sys.exit(0)
out = []
for i in items:
    if not isinstance(i, dict): continue
    t = i.get("type", "state")
    out.append({"type": "relation:" + "related to" if t == "relation" else t, "text": i.get("text", ""), "body": i.get("body", ""), "source": ref,
                "quote": i.get("quote", ""), "entities": i.get("entities", []), "stated": i.get("stated", "inferred")})
r = subprocess.run([tm, "kg", "claims"], input=json.dumps(out), capture_output=True, text=True)
log(True, f"{cost} claims={len(out)} {r.stdout.strip()}")
print(f"{ref}: {cost} · {r.stdout.strip()}"); sys.stderr.write(r.stderr)
PY
done
