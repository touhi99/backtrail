#!/bin/bash
# Extract decisions, steps, questions and claims from sessions with a small model, one single-shot call per session.
# Usage: extract.sh [--entity NAME] [--limit N] [--session ID] [--no-cap] | extract.sh <session ref>...
# The backtrail binary never calls a model: `extract-input` is piped to a headless model and the answer goes back
# through `import -`, so the verbatim-quote, known-entity, known-thread and length rules gate every fact.
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
MODEL="${BACKTRAIL_COMPILE_MODEL:-haiku}"
export BACKTRAIL_COMPILE=1   # the SessionStart and SessionEnd hooks exit at once while this is set
# a login agent starts with a minimal PATH; the claude CLI usually lives in one of these
export PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
# without the CLI every session would fail and be logged as failed, using up the daily cap and the retry limit
command -v claude >/dev/null 2>&1 || { echo "extract.sh: the claude CLI is not on PATH; nothing was run or logged" >&2; exit 1; }
ENTITY=""; LIMIT=3; SESSION=""; NOCAP=""; REFS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --entity) ENTITY="$2"; shift 2 ;;
    --limit) LIMIT="$2"; shift 2 ;;
    --session) SESSION="$2"; shift 2 ;;
    --no-cap) NOCAP=1; shift ;;
    *) REFS+=("$1"); shift ;;
  esac
done
if [ ${#REFS[@]} -eq 0 ]; then
  args=(extract-queue --limit "$LIMIT")
  [ -n "$ENTITY" ] && args+=(--entity "$ENTITY")
  [ -n "$SESSION" ] && args+=(--session "$SESSION")
  [ -n "$NOCAP" ] && args+=(--no-cap)
  while IFS= read -r r; do [ -n "$r" ] && REFS+=("$r"); done < <("$TM" "${args[@]}")
fi
cd "$(mktemp -d)" || exit 1  # no project CLAUDE.md or session history is loaded here
for ref in "${REFS[@]+"${REFS[@]}"}"; do
  packet="$("$TM" extract-input "$ref")" || { echo "$ref: no packet"; continue; }
  raw="$(printf '%s' "$packet" | claude -p --model "$MODEL" --tools "" --no-session-persistence --output-format json \
        --setting-sources project --disable-slash-commands --strict-mcp-config --max-budget-usd 0.25 \
        --system-prompt "$(cat "$DIR/extract-prompt.md")" 2>/dev/null)"
  REF="$ref" TM="$TM" RAW="$raw" DIR="$DIR" python3 - <<'PY'
import json, os, re, subprocess, sys
ref, tm, raw = os.environ["REF"], os.environ["TM"], os.environ["RAW"]
def log(ok, detail):
    subprocess.run([tm, "extract-log", ref, "--detail", detail] + ([] if ok else ["--failed", "1"]), capture_output=True)
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
out, closed = [], []
for i in items:
    if not isinstance(i, dict): continue
    if i.get("type") == "finished":
        # an open step this session finished: `finish` checks the quote and decides done or "built, check it"
        f = subprocess.run([os.path.join(os.environ["DIR"], "close-step.sh"), str(i.get("step", "")), ref, str(i.get("quote", ""))], capture_output=True, text=True)
        closed.append((f.stdout or f.stderr).strip().split("  <-")[0])
        continue
    out.append({"type": i.get("type"), "title": i.get("title") or i.get("text", ""), "body": i.get("body", ""), "source": ref,
                "quote": i.get("quote", ""), "entities": i.get("entities", []), "valid_to": i.get("valid_to", ""), "kind": i.get("kind", ""),
                "stated": i.get("stated", "inferred"), "state": "proposed", "origin": "suggested"})
r = subprocess.run([tm, "import", "-"], input=json.dumps(out), capture_output=True, text=True)
fin = f" finished={len(closed)}" if closed else ""
log(True, f"{cost} items={len(out)}{fin} {r.stdout.strip()}")
print(f"{ref}: {cost} · {r.stdout.strip()}")
for line in closed: print(f"  {line}")
sys.stderr.write(r.stderr)
PY
done
