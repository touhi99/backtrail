#!/bin/bash
set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
TM="$DIR/../target/release/backtrail"
MODEL="${BACKTRAIL_RESOLVE_MODEL:-sonnet}"
export BACKTRAIL_COMPILE=1
LIMIT=20; IDS=(); DRY=; REV=
while [ $# -gt 0 ]; do
  case "$1" in
    --dry) DRY=1; shift ;;
    --reverses) REV=1; shift ;;
    --limit) LIMIT="$2"; shift 2 ;;
    *) IDS+=("$1"); shift ;;
  esac
done
if [ ${#IDS[@]} -eq 0 ]; then
  while IFS= read -r i; do [ -n "$i" ] && IDS+=("$i"); done < <("$TM" resolve-queue ${REV:+--reverses} --limit "$LIMIT")
fi
if [ -n "$REV" ]; then PROMPT="$DIR/reverse-prompt.md"; else PROMPT="$DIR/resolve-prompt.md"; fi
cd "$(mktemp -d)" || exit 1
for id in "${IDS[@]+"${IDS[@]}"}"; do
  packet="$("$TM" resolve-packet "$id" ${REV:+--reverses})" || { echo "$id: no packet"; continue; }
  raw="$(printf '%s' "$packet" | claude -p --model "$MODEL" --tools "" --no-session-persistence --output-format json \
        --setting-sources project --disable-slash-commands --strict-mcp-config --max-budget-usd 0.25 \
        --system-prompt "$(cat "$PROMPT")" 2>/dev/null)"
  REV="$REV" DRY="$DRY" ID="$id" TM="$TM" RAW="$raw" PACKET="$packet" DIR="$DIR" VMODEL="${BACKTRAIL_VERIFY_MODEL:-opus}" python3 - <<'PY'
import json, os, re, subprocess, sys
id_, tm, raw = os.environ["ID"], os.environ["TM"], os.environ["RAW"]
try:
    d = json.loads(raw)
    u = d.get("usage", {})
    cost = f"usd={d.get('total_cost_usd', 0):.4f} out={u.get('output_tokens', 0)}"
    text = re.sub(r"^```(?:json)?|```$", "", d["result"].strip(), flags=re.M).strip()
    v = json.loads(text)
    assert isinstance(v, dict)
    v["id"] = id_
except Exception as e:
    print(f"{id_}: failed ({type(e).__name__})"); sys.exit(0)
def ask_verifier(ask, prompt_file):
    p = subprocess.run(["claude", "-p", "--model", os.environ["VMODEL"], "--tools", "", "--no-session-persistence",
        "--output-format", "json", "--setting-sources", "project", "--disable-slash-commands", "--strict-mcp-config",
        "--max-budget-usd", "0.25", "--system-prompt", open(os.environ["DIR"] + "/" + prompt_file).read()],
        input=ask, capture_output=True, text=True, timeout=300)
    d2 = json.loads(p.stdout)
    verdict = json.loads(re.sub(r"^```(?:json)?|```$", "", d2["result"].strip(), flags=re.M).strip())
    return verdict, d2.get("total_cost_usd", 0)
if os.environ.get("REV"):
    proposed = [r for r in (v.get("replaces") or []) if isinstance(r, str)]
    kept = []
    for old in proposed:
        ask = os.environ["PACKET"] + f"\n\nPROPOSED REVERSAL\nOLDER {old}\nREASON {v.get('reason', '')}\n"
        try:
            verdict, usd = ask_verifier(ask, "reverse-verify-prompt.md")
            cost += f" verify={usd:.4f}"
            if verdict.get("holds_no_longer") is True:
                kept.append(old)
        except Exception:
            pass
    v["_proposed"] = proposed
    v["replaces"] = kept
    if os.environ.get("DRY"):
        print(json.dumps(dict(v, _cost=cost))); sys.exit(0)
    r = subprocess.run([tm, "resolve-apply", "-"], input=json.dumps(v), capture_output=True, text=True)
    print(f"{id_}: {cost} · {(r.stdout or r.stderr).strip()}"); sys.exit(0)
if v.get("resolves_by"):
    ask = os.environ["PACKET"] + f"\n\nPROPOSED RESOLUTION\nCANDIDATE {v['resolves_by']}\nREASON {v.get('reason', '')}\n"
    try:
        p = subprocess.run(["claude", "-p", "--model", os.environ["VMODEL"], "--tools", "", "--no-session-persistence",
            "--output-format", "json", "--setting-sources", "project", "--disable-slash-commands", "--strict-mcp-config",
            "--max-budget-usd", "0.25", "--system-prompt", open(os.environ["DIR"] + "/resolve-verify-prompt.md").read()],
            input=ask, capture_output=True, text=True, timeout=300)
        d2 = json.loads(p.stdout)
        ok = json.loads(re.sub(r"^```(?:json)?|```$", "", d2["result"].strip(), flags=re.M).strip()).get("settled") is True
        cost += f" verify={d2.get('total_cost_usd', 0):.4f}"
    except Exception:
        ok = False
    v["_verified"] = ok
    if not ok:
        v["resolves_by"] = ""
if os.environ.get("DRY"):
    print(json.dumps(dict(v, _cost=cost))); sys.exit(0)
r = subprocess.run([tm, "resolve-apply", "-"], input=json.dumps(v), capture_output=True, text=True)
print(f"{id_}: {cost} · {(r.stdout or r.stderr).strip()}")
PY
done
