# Backtrail

Backtrail indexes Claude Code sessions and Markdown notes into one SQLite file and shows them in a local web app,
grouped into threads, one per project. Decisions, steps and open questions are recorded on each thread, and each one
links to the session or note it came from with the exact quote.

Status: prototype. Tested on macOS only.

## How it works

![Overview](docs/architecture-overview.svg)

- Claude Code keeps one folder per project in `~/.claude/projects`. Each folder becomes a thread.
- Sessions and notes are read, never changed.
- Decisions, steps and questions are proposed from sessions and stay proposed until accepted in the app. A proposal is
  rejected if its quote is not found in the cited source.
- When a session ends, a hook indexes its new text.

![Session end](docs/architecture-session-end.svg)

More detail: `docs/architecture.md`.

What it reads and sends:

- Reads: Claude Code sessions and memory notes, plus note folders and git repositories listed in `config.json`.
- Writes: the data directory (database, backups, saved artifacts), and `~/.claude/settings.json` on `bt hooks install`.
- Network: passage and query text go to the local Ollama server. The `claude` CLI scripts in `skill/` send session text
  to Claude.

## Screenshots

| | |
| --- | --- |
| ![Today](docs/demo/today.png) | ![Graph](docs/demo/graph.png) |

## Requirements

- Rust 1.85 or later, and Node.js
- Ollama, for search by meaning. Other embedding servers (such as LM Studio) are not supported.
- The `claude` CLI, for building the graph and proposing decisions and steps

## Install

```
ollama pull embeddinggemma-2:270m
./install.sh
bt serve
```

`install.sh` builds the app and links it as `bt`. `bt serve` starts the app at `http://127.0.0.1:8799` and indexes
existing sessions on first start.

Optional setup:

- Custom threads, note folders and git repositories: `config.json` in the data directory (`docs/reference.md`).
- Moving to another machine: copy the data directory before the first `bt serve`.

Environment variables:

| Variable | Default |
| --- | --- |
| `BACKTRAIL_HOME` | macOS: `~/Library/Application Support/Backtrail`. Linux: `$XDG_DATA_HOME/backtrail`, else `~/.local/share/backtrail` |
| `BT_BIN_DIR` | `~/.local/bin` |

## Usage

| Command | Purpose |
| --- | --- |
| `bt serve [--port N]` | Run the app; it rescans every 5 minutes |
| `bt hooks install` | Index each session when it ends and show a brief when one starts (`bt hooks remove` undoes it) |
| `bt search <words>` | Search with cited sources |
| `bt check` | Report lost quotes, orphan threads and stale work |

Other commands: `docs/reference.md`.

## Limits

- Search compares vectors in memory, which suits a few hundred thousand passages at most.
- Search ranking is set by hand. Embedding models compared: `embeddinggemma-2:270m` and `nomic-embed-text`, on 519
  queries from one store (`docs/reference.md`).
- The quote check proves a quote exists in the source, not that the title is a fair reading of it.
- Automatic extraction of decisions and the matching of steps to commits have not been measured for accuracy.
