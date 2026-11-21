# Backtrail

Backtrail builds a knowledge graph from coding-agent sessions, notes and git history. It holds claims: decisions,
steps, questions and facts, each citing the line that stated it. Decisions link to the projects, tools and models they
are about, to the reasons behind them and to the decisions they replace. A model proposes claims; a decision is
settled when the user states it in a session, accepts it in the UI or writes it in a note.

Agents read the graph through hooks. A session starts with the open steps of the projects in motion, and each prompt
receives the settled decisions that bear on it. The UI (web or Mac app) shows the graph and each project's decisions
over time.

## Problem

Coding agents make it easy to run more projects in parallel than one person can follow.

- Each agent keeps its own transcripts. Notes live in other files.
- A project left alone for a couple of weeks takes time to pick up again.
- Search returns matches, not what was decided, why, or what it replaced. Reading notes or session history instead
  does not scale across many long-running projects.
- A new session does not know what earlier sessions settled, so the agent asks again or undoes it.

| | |
| --- | --- |
| ![Home](docs/demo/home.png) | ![Project page](docs/demo/thread.png) |

## How it works

1. When a session ends, Backtrail indexes it and a model reads it. Agents without a session-end event (Codex,
   Antigravity) are read after 30 idle minutes.
2. The model proposes decisions, steps, questions and facts, each with the line it came from. A claim whose line is not
   in the session is refused.
3. A model links each new claim to earlier ones: repeats, answers to open questions, and decisions it reverses. A
   second model call must confirm each reversal.
4. The user settles decisions. Claims nobody confirms are parked after 14 days.
5. A step is marked done when a model finds it finished in a session and a second model call confirms it. The proof
   names that session's pull requests and pushes to `main`, with their state on GitHub.
6. The next session starts with the open steps of the projects in motion, and each prompt gets up to two settled
   decisions that match it.

Without `config.json`, each project folder becomes a project; `config.json` defines custom ones
([`docs/reference.md`](docs/reference.md#configuration)).

![Components](docs/architecture.svg)

Detail: [`docs/architecture.md`](docs/architecture.md). Per-agent setup: [`docs/harnesses.md`](docs/harnesses.md).

## Requirements

- Rust 1.88+ and Node.js 22+ to build.
- A coding agent CLI for the model that reads sessions: `claude`, `gemini`, `codex`, `agy`, or any command with a
  headless mode. Reading takes about 6 model calls per session, billed to that CLI's plan or API key.
- GitHub CLI (`gh`), logged in, to check the state of pull requests and pushes.
- Ollama with `embeddinggemma-2:270m`. Its embeddings drive semantic search, finding the earlier claims a new claim
  repeats, answers or reverses, and finding where an open step was finished. While Ollama is not running, all three
  fall back to keyword search.
- Swift, for the Mac app (macOS only).

## Install

```
./install.sh
bt build
```

`install.sh` checks the requirements, pulls the Ollama model, asks which agents to connect and adds Backtrail's hooks
and skill to each. On macOS it installs a LaunchAgent that runs the server, and builds the Mac app. Elsewhere, run
`bt serve`.

`bt build` indexes all sessions and notes. The server then has the model read them in the background, and the brief
fills as it does.

## Usage

- UI: `http://127.0.0.1:8799`, or `mac/Backtrail.app` on macOS.
- `bt search <words>` searches with sources. `bt brief` prints what was decided and what is next.
- `bt hooks install --harness <agent>` connects another agent. `bt agent <agent>` changes which CLI reads sessions.

## Uninstall

```
bt hooks remove --harness claude,gemini,codex,antigravity
launchctl bootout gui/$(id -u)/dev.backtrail.serve
rm ~/Library/LaunchAgents/dev.backtrail.serve.plist ~/.local/bin/bt ~/.local/bin/backtrail
```

The data directory (`bt home` prints it) holds the database and backups.

## Data

- Reads the session folders of Claude Code, Codex CLI and Gemini CLI, an inbox folder for other tools, and the notes
  folders and git repos listed in `config.json`.
- Writes the data directory (database, backups, logs), the hooks and skill of each connected agent, and on macOS a
  LaunchAgent.

## Limits

- Gemini CLI, Codex CLI and Antigravity support follows their documentation and has not been run on real sessions.
  Antigravity sessions need a converter ([`docs/harnesses.md`](docs/harnesses.md#adding-a-tool)).
- The quote check confirms the quoted line exists, not that the claim's title reads it correctly.

## License

MIT
