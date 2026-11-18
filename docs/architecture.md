# Architecture

Describes the code as built. The overview and session-end diagrams are in the README; the data diagram is below.
Commands are listed in `reference.md`.

## Overview

One Rust binary, one SQLite file, a web app embedded in the binary and a SwiftUI Mac app (`mac/`) that reads the same
JSON API. The binary writes the database; sources (session logs, notes, git history, artifact snapshots) are read.

- The binary calls the embedding server, on by default at `127.0.0.1:11434`. When it does not answer, search is
  keyword only. `"embeddings": false` in `config.json` turns it off.
- The server listens on `127.0.0.1` and answers `403` to a request whose `Host` or `Origin` header names another
  machine, so a page on another site cannot read or write the store.
- Facts (decisions, steps, questions, claims) are produced by Claude Code sessions or `skill/extract.sh`, and written
  back through the binary.

## Modules

| File | Role |
| --- | --- |
| `main.rs` | Command dispatch: parses the arguments of each command and calls one function. |
| `db.rs` | Opening the store, query helpers, source refs and the quote lookup that every write rule uses. |
| `migrate.rs` | Schema (`sql/schema.sql`) and store versions in `PRAGMA user_version`; each migration backs up the store and runs in one transaction. |
| `items.rs` | Work items: the write rule (type, length limits, a verbatim quote from an indexed source), the keep rule, and every change the app or the command line makes to an item. |
| `views.rs` | Read-only page data for the app (Home, thread, focus, items, source, trail, graph) and the session-start brief. `decided()` lists a project's latest settled decisions still in force, with who confirmed each; the brief and `/api/thread` use it. |
| `search.rs` | Hybrid passage search (BM25 and vectors, merged by reciprocal rank fusion) and the item index. |
| `check.rs` | `check`, `check --pairs`, `pending`, `match`, `artifacts`, `show` and `log`. |
| `server.rs` | Local HTTP server (`tiny_http`, loopback only): routes, request guard and the background scan. Each route calls one function. |
| `ingest/` | Readers for sessions, notes, artifact snapshots and git (`sessions.rs`, `notes.rs`, `git.rs`), thread assignment (`threads.rs`), relinking and the daily backup (`mod.rs`). |
| `resolve.rs` | Candidates for a proposed item (shared entities, rarity-weighted words, nearest passages), the model packet, applying a verdict (entities, thread, `valid_to`, settling item), `resolve`, `unresolve`, expiry. |
| `kg/` | Knowledge graph: claim write rule and relations (`claims.rs`), entities (`entities.rs`), history queries (`query.rs`), extraction queue and packet (`extract.rs`), graph pages (`screen.rs`), lint, export and restore. |
| `embed.rs` | Embedding client, vector table and brute-force nearest-neighbour lookup. |
| `config.rs`, `clock.rs` | `config.json`, read once per process; UTC timestamps. |
| `demo.rs` | The demo store. |
| `src/sql/` | `schema.sql` (tables), `views.sql` (the `item`, `fact`, `thread_stat`, `source_stat` and `entity_stat` read views that pages filter) and `migrate_unify.sql`. Other statements are inline next to the code that runs them. |
| `app/` | React and Vite web app, built to `app/dist/` and embedded into the binary. |
| `mac/` | SwiftUI Mac app, a client of the `/api` routes on `127.0.0.1:8799`. Keeps the last answer per route in memory, so a page shows it at once and refreshes. Built by `mac/build.sh`. |

## Data

![Tables and the keys that join them](architecture-data.svg)

- `sources` and `chunks` hold the indexed text; `fts` is an FTS5 index over chunks (Porter stemming). A session's ref is
  `session:<session id>`; commands also take a unique prefix such as `session:1a2b3c4d`. For a session, `bytes` is how
  far the file has been read, `tail` a hash of the 256 bytes before that point and `nturns` the turns stored.
- `vec` stores one embedding per distinct passage text, keyed by a hash, so a growing session embeds only new text.
- `claims` holds every fact in one table: decisions, steps, questions and turns (the work items) and `state`,
  `rationale` and `relation:<kind>` claims. A fact names its thread, cites one chunk and a quote, and carries a `state`
  (`proposed`, `settled`, `rejected`, `superseded`; `open`, `doing`, `looks_done`, `done`, `parked` for steps; `open`,
  `answered` for questions). The columns `text` and `status` are read-only names for `title` and `state` that the graph
  queries use. A relation between projects is a claim about two entities; a replaced item points at its replacement in
  `superseded_by`. `review` is `auto` for a fact the keep rule kept and `held|<reason>` for one it left for review.
  New fact ids carry a 64-bit hash (`d-` plus 16 hex digits); ids from before store version 1 keep their 32-bit hash.
- `threads` holds the projects (name, state, area, kind, summary).
- `entities`, `entity_aliases` and `claim_about` say what a claim is about. Decisions are linked to entities by whole-word
  alias match; `state`, `rationale` and `relation` claims name their entities when they are filed.
- `overrides` pins sessions to threads; `kg_overrides` keeps owner choices about the graph. `log` records who changed what.

## Processes

The server runs a timer thread every 5 minutes: rescan sources, write a daily backup, embed new passages up to the
configured batch size.

Hooks (diagram: README, "How it works"):

- SessionStart runs `bt brief`, which rescans and prints open questions and threads in motion. It never embeds.
  `bt brief --thread <name>` adds the project's latest settled decisions; proposed items are never listed.
- SessionEnd runs `skill/session-end.sh` in the background: `ingest`, `embed`, then extraction
  (`skill/extract.sh`, one small-model call per session) and resolution (`skill/resolve.sh`, one call per proposed
  item). `BACKTRAIL_COMPILE_ON_END=0` skips both.
- Finished steps: the extraction packet lists every open step with its id, and the model names the ones the session
  finished, with a quote. `skill/close-step.sh` runs `finish`: the quote must be in the session, and a second model
  (`BACKTRAIL_VERIFY_MODEL`, default opus) must confirm the passage shows that step finished before it closes;
  otherwise nothing changes. `bt match` (shared phrases between step and commit subject) runs only when called. `skill/finish.sh [--dry]` runs
  the same check over steps open for more than a day, using the passages most like each step. The sessions that finish
  a step are often filed under another thread, so the list covers all threads.

## Search

Keyword (BM25) and vector lists are merged by reciprocal rank fusion, then multiplied by factors for thread state and
citations. Thread names and aliases expand the query. Filed items and `kg find` claims list settled ones before
proposed and superseded ones. Weights and limits: `reference.md`, section "Search ranking".

`bt kg find` uses the same function for its last route: it takes the best passages from the hybrid search and prints the
claims that cite them (date, speaker, quote), or the passages when none does. Before that it tries lineage, the entity
page and a match of the question's words against claim text.

## Migration

A store from the earlier layout (items in `nodes`, graph in `claims` and `claim_evidence`, relations in `links`) is
converted the first time the binary opens it. The old file is copied to `backtrail.pre-unify.db` first. Item ids are
kept; an item and the claim copied from it become one row; the conversion runs in one transaction and aborts if the row
count differs from the expected total.

`PRAGMA user_version` holds the store version. Version 1 adds the `bytes`, `tail`, `nturns` and `review` columns,
rewrites session refs from the first 8 characters of the session id to the whole id (in `sources`, `overrides` and
`log`), and runs the keep rule once over every proposed fact, oldest first. The old file is copied to
`backtrail.pre-v1.db` first, and the change runs in one transaction.

## Reading sessions

Claude Code appends to a session file. A changed file is read from `bytes` onward when the 256 bytes before that point
still hash to `tail`; only new turns are added, numbered after the stored ones. A file that shrank, moved or changed
earlier is read again in full. Reading stops at the last complete line, so a line still being written is read on the
next scan. Notes and artifacts are still read again in full when they change; after a scan, `relink` points each fact
whose passage no longer holds its quote at the passage that does.

## App

Routes: Home, Graph (network and over-time views), Items, Log, Search, Thread, Source. `/map?thread=` shows one
project's items and notes. Home reads `/api/today` and `/api/thread` (one call per project in motion, for its next step
and latest decisions); the Graph page reads `/api/kg/graph`, `/api/kg/claims`,
`/api/kg/lineage`, `/api/kg/suggest` (terms with example quotes) and `/api/kg/growth`. `POST /api/kg/entity` returns the claims linked and the unread sessions; `POST /api/kg/extract` starts `skill/extract.sh --entity` in the background. All writes from the app go through `POST /api/*` handlers in
`server.rs`, which call the same functions in `items.rs` and `kg/` as the command line.

Limits: README, section "Limits", and `reference.md`, section "Scale limits".
