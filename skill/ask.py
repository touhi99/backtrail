#!/usr/bin/env python3
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
BIN = HERE.parent / "target" / "release" / "backtrail"
MARK = "BACKTRAIL-CALL\n"
CLIS = {"claude": "claude", "gemini": "gemini", "codex": "codex", "antigravity": "agy"}


def data_dir():
    return os.environ.get("BACKTRAIL_HOME") or subprocess.run(
        [str(BIN), "home"], capture_output=True, text=True).stdout.strip()


def run_dir():
    d = Path(data_dir(), "agent-run")
    d.mkdir(parents=True, exist_ok=True)
    return str(d)


def config():
    home = data_dir()
    try:
        agent = json.loads(Path(home, "config.json").read_text()).get("agent") or {}
    except (OSError, ValueError):
        agent = {}
    cli = os.environ.get("BACKTRAIL_AGENT") or agent.get("cli") or "claude"
    return cli, agent


def model_for(agent, tier):
    order = {"small": ["small", "medium", "large"], "medium": ["medium", "large", "small"],
             "large": ["large", "medium", "small"]}[tier]
    return next((agent[t] for t in order if agent.get(t)), "")


def run(cmd, prompt, timeout=600, shell=False, cwd=None):
    p = subprocess.run(cmd, input=prompt, capture_output=True, text=True, timeout=timeout, shell=shell, cwd=cwd)
    if p.returncode != 0:
        raise RuntimeError(f"{cmd if shell else cmd[0]} exited {p.returncode}: {p.stderr.strip()[-300:]}")
    return p.stdout


def claude(system, packet, model, budget):
    out = run(["claude", "-p", "--model", model, "--tools", "", "--no-session-persistence",
               "--output-format", "json", "--setting-sources", "project", "--disable-slash-commands",
               "--strict-mcp-config", "--max-budget-usd", budget, "--system-prompt", system], packet)
    return json.loads(out)


def gemini(system, packet, model):
    cmd = ["gemini", "--output-format", "json", "-p", system] + (["-m", model] if model else [])
    d = json.loads(run(cmd, MARK + packet, cwd=run_dir()))
    tokens = next(iter((d.get("stats", {}).get("models") or {}).values()), {}).get("tokens", {})
    return {"result": d.get("response", ""), "total_cost_usd": 0,
            "usage": {"input_tokens": tokens.get("prompt", 0), "output_tokens": tokens.get("candidates", 0)}}


def codex(system, packet, model):
    with tempfile.TemporaryDirectory() as tmp:
        last = Path(tmp, "answer.txt")
        cmd = ["codex", "exec", "--ephemeral", "--skip-git-repo-check", "--sandbox", "read-only",
               "--output-last-message", str(last)] + (["-m", model] if model else []) + ["-"]
        run(cmd, f"{MARK}{system}\n\n{packet}")
        return {"result": last.read_text(), "total_cost_usd": 0, "usage": {}}


def antigravity(system, packet, model):
    cmd = ["agy", "-p", f"{MARK}{system}\n\n{packet}", "--output-format", "json"] + (["--model", model] if model else [])
    d = json.loads(run(cmd, "", cwd=run_dir()))
    if d.get("error"):
        raise RuntimeError(f"agy: {d['error']}")
    return {"result": d.get("response", ""), "total_cost_usd": 0, "usage": d.get("usage") or {}}


def command(agent, system, packet):
    line = agent.get("command") or ""
    if not line:
        raise RuntimeError('agent.cli is "command" but agent.command is empty in config.json')
    return {"result": run(line, f"{MARK}{system}\n\n{packet}", shell=True), "total_cost_usd": 0, "usage": {}}


def main():
    ap = argparse.ArgumentParser(description="Send one Backtrail prompt to the configured coding agent CLI.")
    ap.add_argument("--tier", choices=["small", "medium", "large"], default="small")
    ap.add_argument("--model", default="haiku", help="Claude model name, used when the agent is claude")
    ap.add_argument("--budget", default="0.25")
    ap.add_argument("--system", help="file holding the instructions")
    ap.add_argument("--text", action="store_true", help="print only the answer text")
    ap.add_argument("--check", action="store_true", help="exit 1 when the configured CLI is missing")
    a = ap.parse_args()
    cli, agent = config()
    if a.check:
        exe = CLIS.get(cli)
        if cli != "command" and (exe is None or shutil.which(exe) is None):
            print(f"ask.py: agent CLI '{cli}' is not on PATH; set agent.cli in config.json", file=sys.stderr)
            sys.exit(1)
        print(cli)
        return
    system = Path(a.system).read_text()
    packet = sys.stdin.read()
    if cli == "claude":
        d = claude(system, packet, a.model, a.budget)
    elif cli == "gemini":
        d = gemini(system, packet, model_for(agent, a.tier))
    elif cli == "codex":
        d = codex(system, packet, model_for(agent, a.tier))
    elif cli == "antigravity":
        d = antigravity(system, packet, model_for(agent, a.tier))
    elif cli == "command":
        d = command(agent, system, packet)
    else:
        raise SystemExit(f"ask.py: unknown agent.cli '{cli}' (claude, gemini, codex, antigravity or command)")
    print(d["result"] if a.text else json.dumps(d))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.TimeoutExpired, ValueError, OSError) as e:
        print(f"ask.py: {e}", file=sys.stderr)
        sys.exit(1)
