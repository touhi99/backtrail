#!/bin/bash
# Compile sessions into proposed items with a small model, one single-shot call per session.
# Usage: compile.sh <session ref>...
# The backtrail binary never calls a model; this script pipes `compile-input` to a headless model and the
# answer back through `import -`, so the quote and length rule still gates every item.
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
MODEL="${BACKTRAIL_COMPILE_MODEL:-haiku}"
export BACKTRAIL_COMPILE=1   # the SessionStart and SessionEnd hooks exit at once while this is set
cd "$(mktemp -d)" || exit 1  # no project CLAUDE.md or session history is loaded here
for ref in "$@"; do
  packet="$("$TM" compile-input "$ref")" || { echo "$ref: no packet"; continue; }
  raw="$(printf '%s' "$packet" | claude -p --model "$MODEL" --tools "" --no-session-persistence --output-format json \
        --setting-sources project --disable-slash-commands --strict-mcp-config --max-budget-usd 0.25 \
        --system-prompt "$(cat "$DIR/compile-prompt.md")" 2>/dev/null)"
  REF="$ref" TM="$TM" RAW="$raw" python3 - <<'PY'
import json, os, re, subprocess, sys
ref, tm, raw = os.environ["REF"], os.environ["TM"], os.environ["RAW"]
def log(ok, detail):
    args = [tm, "compile-log", ref, "--detail", detail] + ([] if ok else ["--failed", "1"])
    subprocess.run(args, capture_output=True)
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
packet_thread = subprocess.run([tm, "compile-input", ref], capture_output=True, text=True).stdout
thread = re.search(r"^THREAD (.*)$", packet_thread, re.M).group(1)
out = [{"type": i.get("type"), "thread": thread, "title": i.get("title", ""), "body": i.get("body", ""), "source": ref,
        "quote": i.get("quote", ""), "state": "proposed", "origin": "suggested"} for i in items if isinstance(i, dict)]
r = subprocess.run([tm, "import", "-"], input=json.dumps(out), capture_output=True, text=True)
log(True, f"{cost} items={len(out)} {r.stdout.strip()}")
print(f"{ref}: {cost} · {r.stdout.strip()}" + (f" · {r.stderr.strip().count(chr(10)) + 1 if r.stderr.strip() else 0} rejected" if r.stderr.strip() else ""))
PY
done
