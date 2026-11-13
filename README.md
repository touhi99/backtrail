# Backtrail

Local index and web app that organise work done with Claude Code into threads. Session logs, Markdown notes and
artifact snapshots are read, never modified, and indexed into one SQLite file. Decisions, steps and questions are
filed against threads (one thread per project or topic), and each must cite an indexed source with a verbatim quote.

Status: prototype. Tested on macOS only.

## How it works

![Overview](docs/architecture-overview.svg)

Items are rejected unless the cited source is indexed and the quote is found in it. Claude Code sessions or
`skill/compile.sh` (see `skill/SKILL.md`) propose items; they stay `proposed` until accepted in the app.

When a Claude Code session ends, a SessionEnd hook indexes the new text and embeds it:

![Session end](docs/architecture-session-end.svg)

Modules, data model and processes: `docs/architecture.md`.

## What it reads and sends

- Reads: Claude Code sessions (`~/.claude/projects/*/*.jsonl`), Markdown notes (folders in the configuration and the
  Claude Code memory folders), artifact snapshots, and git history of repositories listed in the configuration.
- Writes: `<data dir>/backtrail.db`, artifact snapshots, and daily backups in `<data dir>/backups`.
- Network: passage and query text go to a local Ollama server (`embeddings.url` in `config.json`); `"embeddings": false`
  turns this off. `compile.sh` runs the `claude` CLI, which sends session text to the model provider.

## Screenshots

Taken from the demo store (`bt demo`), which holds invented content.

| | |
| --- | --- |
| ![Today](docs/demo/today.png) | ![Graph](docs/demo/graph.png) |

## Requirements

- Rust 1.85 or later, and Node.js
- Optional: Ollama with the `nomic-embed-text` model, for meaning-based search (setup: `docs/reference.md`)
- Optional: the `claude` CLI, used only by `skill/compile.sh`

## Install

```
./install.sh
bt serve
```

- `install.sh` builds the app and the program and links it as `bt` in `~/.local/bin` (override with `BT_BIN_DIR`).
- `bt serve` starts the app at `http://127.0.0.1:8799`.
- After start, it reads the Claude Code sessions in `~/.claude/projects` and the memory notes next to them.
- Each project folder becomes a thread. Thread rules and note folders can be added in an optional `config.json` (more
  in `docs/reference.md`).
- Data directory: `~/Library/Application Support/Backtrail` on macOS (override with `BACKTRAIL_HOME`).

## Usage

| Command | Purpose |
| --- | --- |
| `bt serve [--port N]` | Run the app; it also rescans every 5 minutes |
| `bt search <words>` | Search with cited sources |
| `bt hooks install` | Add the SessionStart and SessionEnd hooks to `~/.claude/settings.json` (`remove` undoes it) |
| `bt check` | Report lost quotes, orphan threads, stale work |

All other commands (`ingest`, `brief`, `add`, `import`, `done`, `backup`, `kg`) and the configuration keys are in
`docs/reference.md`.

## Limits

- Vectors are compared by brute force in memory, which suits a few hundred thousand passages at most. Other parts that
  do not scale unchanged are listed in `docs/reference.md`.
- Ranking weights are set by hand and not tuned. `nomic-embed-text` is the only embedding model tried.
- The quote rule proves a quote exists in the source, not that the title is a fair reading of it.
- Claim extraction ran on a handful of sessions; its accuracy and the commit matcher have not been measured.
