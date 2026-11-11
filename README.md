# Backtrail

Local index and web app that organise work done with Claude Code into threads. Session logs, Markdown notes and
artifact snapshots are read, never modified, and indexed into one SQLite file. Decisions, steps and questions are
filed against threads, and each must cite an indexed source with a verbatim quote.

Status: prototype. Tested on macOS only.

## How it works

- `backtrail ingest` indexes Claude Code sessions (`~/.claude/projects/*/*.jsonl`), Markdown notes (folders in the
  configuration and the Claude memory folder) and artifact snapshots, using SQLite FTS5.
- Sources are assigned to threads by regular-expression rules in `config.json`. Unmatched sources stay in `Unsorted`.
- Items (decision, step, question, turn) are filed with `add` or `import`. An item is rejected if its source is not
  indexed, its quote is not found in that source, or its title (110 characters) or body (320) is too long.
- Search is keyword ranking (BM25), merged with vector similarity when embeddings are configured.
- `backtrail serve` runs the app at `http://127.0.0.1:8799` (loopback only) and rescans every 5 minutes.
- The binary never calls a model. Compiling sessions into proposed items is done by a Claude Code session following
  `skill/SKILL.md`, or by `skill/compile.sh`. Suggested items stay `proposed` until accepted in the app.

## What it reads and sends

- Reads: the session, note and memory paths above, and git history of repositories listed in the configuration.
- Writes: `<data dir>/backtrail.db`, artifact snapshots, and daily backups in `BacktrailBackups`.
- Network: none from the binary unless `embeddings` is configured, which sends passage and query text to the
  configured URL. `compile.sh` runs the `claude` CLI, which sends session text to the model provider.

## Requirements

- Rust toolchain (edition 2024) and Node.js
- Optional: Ollama with an embedding model (for example `nomic-embed-text`) for meaning-based search
- Optional: the `claude` CLI, for compiling sessions

## Install

```
(cd app && npm install && npm run build)   # the build output is embedded into the binary
cargo build --release
./target/release/backtrail ingest
./target/release/backtrail serve
```

Data directory: `~/Library/Application Support/Backtrail` (override with `BACKTRAIL_HOME`). On a new machine, create
`<data dir>/config.json` with `workspace` and `note_dirs` first; without it every source lands in `Unsorted`.
Not yet run on a second machine. Hooks and embeddings setup: `docs/reference.md`.

## Usage

| Command | Purpose |
| --- | --- |
| `ingest` | Read new or changed sources |
| `serve [--port N]` | Run the app |
| `brief [--thread T]` | Print open questions, threads in motion and stale items |
| `search <words>` | Search with cited sources |
| `add <type> --thread --title --source --quote` | File one item |
| `import <items.json>` | File many items through the same rule |
| `done <step id> --by owner\|claude\|evidence --proof "..."` | Close a step |
| `check` | Report lost quotes, orphan threads, stale work |
| `backup` | Copy the store to `BacktrailBackups` |

All commands and the configuration keys are in `docs/reference.md`.

## Limits

- Vectors are compared by brute force in memory, which suits a few hundred thousand passages at most. Other parts that
  do not scale unchanged are listed in `docs/reference.md`. Nothing has been measured beyond the present store.
- Ranking weights are set by hand and not tuned. `nomic-embed-text` is the only embedding model tried.
- The quote rule proves a quote exists in the source, not that the title is a fair reading of it.
- Extraction accuracy and the commit matcher have not been measured.
- macOS paths are assumed.
