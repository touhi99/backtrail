# Architecture

Describes the code as built. The overview and session-end diagrams are in the README; the data diagram is below.
Commands are listed in `reference.md`.

## Overview

One Rust binary, one SQLite file and a web app embedded in the binary. The binary is the only process that writes
the database. Sources (session logs, notes, git history, artifact snapshots) are read and never modified.

- The only calls the binary makes go to the embedding server, on by default at `127.0.0.1:11434`. If it does not
  answer, search is keyword only. `"embeddings": false` in `config.json` turns it off.
- Items and claims are produced by Claude Code sessions or `skill/compile.sh` and `skill/kg-extract.sh`, and written
  back through the binary.

## Modules

| File | Role |
| --- | --- |
| `main.rs` | Command dispatch: parses the subcommand, opens the store, calls one function. |
| `server.rs` | Local HTTP server (`tiny_http`, loopback only), JSON API, embedded app files, timer thread, brief, check and match. |
| `ingest.rs` | Readers for sessions, notes, memory, artifact snapshots and git; thread assignment by rules in `config.json`. |
| `db.rs` | Schema and the write rule for items: type, length limits, and a verbatim quote that must exist in an indexed source. |
| `compile.rs` | Queue of sessions worth compiling, cost log, retire and assign. No model call. |
| `embed.rs` | Embedding client, vector table and brute-force nearest-neighbour lookup. |
| `kg.rs` | Entities and claims: import, extraction queue, queries (`find`, `why`, `trace`, `neighbors`, `page`), lint, merge, export and restore. |
| `src/sql/*.sql` | Longer SQL statements, loaded with `include_str!`; the first line names the function that uses each. |
| `app/` | React and Vite web app, built to `app/dist/` and embedded into the binary. |

## Data

![Tables and the keys that join them](architecture-data.svg)

- `sources` and `chunks` hold the indexed text; `fts` is an FTS5 index over chunks (Porter stemming).
- `vec` stores one embedding per distinct passage text, keyed by a hash, so a growing session embeds only new text.
- `nodes` holds threads and items (decision, step, question, turn). An item names its thread, cites a source and a quote, and carries a state. `links` holds relations between nodes (`a`, `b`, `kind`).
- `entities`, `entity_aliases`, `claims`, `claim_about` and `claim_evidence` form the knowledge graph. A claim has a
  type, speaker, date and status (`active`, `proposed`, `superseded`, `retracted`) and cites a quote in a chunk.
- `overrides` and `kg_overrides` keep owner decisions across rescans and rebuilds. `log` records who changed what.

## Processes

The server runs a timer thread every 5 minutes: rescan sources, write a daily backup, embed new passages up to the
configured batch size.

Hooks (diagram: README, "How it works"):

- SessionStart runs `bt brief`, which rescans and prints open questions and threads in motion. It never embeds.
- SessionEnd runs `skill/session-end.sh` in the background: `ingest`, `embed`, `kg import`, `kg migrate`. Compiling the
  session and claim extraction run only when `BACKTRAIL_COMPILE_ON_END=1`, because they cost tokens.

## Search

Keyword (BM25) and vector lists are merged by reciprocal rank fusion, then multiplied by factors for thread state and
citations. Thread names and aliases expand the query. Weights and limits: `reference.md`, section "Search ranking".

## App

Routes: Today, Graph (network and over-time views), Items, Log, Search, Thread, Source. `/map?thread=` shows one
project's items and notes. Today reads `/api/today`; the Graph page reads `/api/kg/graph`, `/api/kg/claims`,
`/api/kg/lineage`, `/api/kg/suggest` and `/api/kg/growth`. All writes from the app go through `POST /api/*` handlers in
`server.rs`, which call the same functions as the command line.

Limits: README, section "Limits", and `reference.md`, section "Scale limits".
