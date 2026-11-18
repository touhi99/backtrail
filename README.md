# Backtrail

Backtrail reads Claude Code sessions and Markdown notes into one SQLite file and records the decisions, steps and
questions in them. Each record links to its source with the exact quote. A local web app shows the work by project, and
a session-start hook prints a brief of what was already decided.

| | |
| --- | --- |
| ![Home](docs/demo/home.png) | ![Project page](docs/demo/thread.png) |

## How it works

![Backend](docs/architecture-backend.svg)

- Each working directory becomes a thread. Sessions and notes are read, never changed.
- Decisions, steps and questions are proposed from sessions. A proposal is rejected when its quote is not in the source.
- A decision is settled when the owner typed it, accepts it in the app, or it comes from a note. Otherwise it stays proposed.
- Unconfirmed proposals and open questions are parked after 14 days.
- The knowledge graph is built from those records. Entities are named in a dictionary (`bt kg entity add`); each
  claim is linked to the entities its text names, and relations between threads come from `skill/extract.sh` or
  `bt relate`. Each claim keeps its quote and source. `bt kg lint` lists probable duplicates.
- The binary calls no model. The model passes are scripts in `skill/`.

Detail: `docs/architecture.md`, `docs/reference.md`.

## Install

Requires Rust 1.85+, Node.js, Ollama (`ollama pull embeddinggemma-2:270m`) and the `claude` CLI. The Mac app needs
Swift (Xcode command line tools).

```
./install.sh
bt hooks install
bt build
```

`install.sh` builds the binary and links it as `bt`. On macOS it also installs the login agent `dev.backtrail.serve`,
which runs the server at `http://127.0.0.1:8799` from login, and builds `mac/Backtrail.app`. `BT_NO_AGENT=1` and
`BT_NO_MAC_APP=1` skip those; elsewhere, run `bt serve`. `bt build` indexes the existing sessions and notes (first run:
`docs/reference.md`). `BACKTRAIL_HOME` sets the data directory (default `~/Library/Application Support/Backtrail` on
macOS, `~/.local/share/backtrail` elsewhere).

## Usage

```
bt hooks install     # index each session at its end, print a brief at its start
bt search <words>    # search with cited sources
```

`mac/Backtrail.app` is a SwiftUI client for the same API: Home, project pages, Search, sources and the Graph. Rebuild
it with `mac/build.sh`.

Claude runs the other commands (`brief`, `answer`, `retire`, `kg`) from the hook and `skill/SKILL.md`. List:
`docs/reference.md`.

## Data

Reads Claude Code sessions, note folders and git repositories from `config.json`. Writes the data directory and, on
`bt hooks install`, `~/.claude/settings.json`. Text goes to a local Ollama server; the `claude` CLI scripts in `skill/`
send session text to Claude. The database is not encrypted; common token formats are redacted.

## Limits

- Search compares vectors in memory: a few hundred thousand passages at most.
- The quote check shows the quote exists, not that the title reads it fairly.
- Hedged statements can be stored as decisions, and a decision's thread can be wrong.
- Extraction accuracy is not measured.
