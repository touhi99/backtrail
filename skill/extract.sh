#!/bin/bash
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
MODEL="${BACKTRAIL_COMPILE_MODEL:-haiku}"
export BACKTRAIL_COMPILE=1
export PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
"$DIR/ask.py" --check >/dev/null || { echo "extract.sh: nothing was run or logged" >&2; exit 1; }
ENTITY=""; LIMIT=3; SESSION=""; NOCAP=""; FULL=""; REFS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --entity) ENTITY="$2"; shift 2 ;;
    --limit) LIMIT="$2"; shift 2 ;;
    --session) SESSION="$2"; shift 2 ;;
    --no-cap) NOCAP=1; shift ;;
    --full) FULL=1; shift ;;
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
cd "$(mktemp -d)" || exit 1
for ref in "${REFS[@]+"${REFS[@]}"}"; do
  packet="$("$TM" extract-input "$ref" ${FULL:+--full})" || { echo "$ref: no packet"; continue; }
  raw="$(printf '%s' "$packet" | "$DIR/ask.py" --tier small --model "$MODEL" --budget 0.25 --system "$DIR/extract-prompt.md")"
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
