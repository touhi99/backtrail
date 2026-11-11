#!/bin/bash
# Propose lineage between threads from sessions, one single-shot call per session.
# Usage: relate.sh <session ref>...
# The binary never calls a model; this pipes `relate-input` to a headless model and files each answer through
# `relate`, so the quote rule still gates every edge. Edges arrive as proposed.
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
MODEL="${BACKTRAIL_COMPILE_MODEL:-haiku}"
export BACKTRAIL_COMPILE=1
cd "$(mktemp -d)" || exit 1
for ref in "$@"; do
  packet="$("$TM" relate-input "$ref")" || { echo "$ref: no packet"; continue; }
  raw="$(printf '%s' "$packet" | claude -p --model "$MODEL" --tools "" --no-session-persistence --output-format json \
        --setting-sources project --disable-slash-commands --strict-mcp-config --max-budget-usd 0.25 \
        --system-prompt "$(cat "$DIR/relate-prompt.md")" 2>/dev/null)"
  REF="$ref" TM="$TM" RAW="$raw" python3 - <<'PY'
import json, os, re, subprocess
ref, tm, raw = os.environ["REF"], os.environ["TM"], os.environ["RAW"]
try:
    d = json.loads(raw)
    u = d.get("usage", {})
    cost = f"usd={d.get('total_cost_usd', 0):.4f} in={u.get('input_tokens', 0) + u.get('cache_creation_input_tokens', 0) + u.get('cache_read_input_tokens', 0)} out={u.get('output_tokens', 0)}"
    edges = json.loads(re.sub(r"^```(?:json)?|```$", "", d["result"].strip(), flags=re.M).strip())
    assert isinstance(edges, list)
except Exception as e:
    print(f"{ref}: failed ({type(e).__name__})"); raise SystemExit
ok = bad = 0
for e in edges:
    if not isinstance(e, dict): continue
    r = subprocess.run([tm, "relate", e.get("a", ""), e.get("b", ""), e.get("kind", ""), "--source", ref, "--quote", e.get("quote", ""), "--state", "proposed"], capture_output=True, text=True)
    if r.returncode == 0: ok += 1
    else: bad += 1; print("  " + r.stderr.strip()[:140])
print(f"{ref}: {cost} · {ok} filed, {bad} rejected")
PY
done
