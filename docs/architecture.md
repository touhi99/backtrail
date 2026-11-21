# Architecture

Describes the code as built. Commands are listed in `reference.md`.

![Backend](architecture-backend.svg)

## Overview

One Rust binary, one SQLite file, a web app embedded in the binary and a SwiftUI Mac app (`mac/`) that reads the same
JSON API. The binary writes the database; sources (session logs, notes, git history, artifact snapshots) are read.

- The binary calls the embedding server, on by default at `127.0.0.1:11434`. When it does not answer, search is
  keyword only. `"embeddings": false` in `config.json` turns it off.
- The server listens on `127.0.0.1` and answers `403` to a request whose `Host` or `Origin` header names another
  machine, so a page on another site cannot read or write the store.
- Facts (decisions, steps, questions, claims) are produced by Claude Code sessions or `skill/extract.sh`, and written
  back through the binary.
- The knowledge graph is built from those records. Entities are named in a dictionary (`bt kg entity add`); each claim
  is linked to the entities its text names, and relations between threads come from `skill/extract.sh` or `bt relate`.
  `bt kg lint` lists probable duplicates.

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
| `mac/` | SwiftUI Mac app, a client of the `/api` routes on `127.0.0.1:8799`. Keeps the last answer per route in memory, so a page shows it at once and refreshes. Built by `mac/build.sh`. Has the same pages and actions as the web app; `scripts/check` fails when an `/api` route is called by only one of the two apps. |

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
- `claim_links` holds links from a decision (`a`) to an older decision or a reason (`b`): `changed` (the older one no
  longer holds), `restates` (the same decision in other words; both stay true) and `because` (a rationale claim that
  explains it). `resolve.sh --reverses` writes them; a `changed` link needs a second model's yes, as the reversal
  always did. Each rescan labels decisions marked replaced without a link: a resolver repeat or a retire whose reason
  says duplicate becomes `restates`, any other reversal or retire becomes `changed`.

## Processes

The server runs a timer thread every 5 minutes: rescan sources, write a daily backup, embed new passages up to the
configured batch size.

Hooks (diagram: README, "How it works"):

- SessionStart runs `bt brief`, which rescans and prints open questions and threads in motion; embedding is left
  to the timer and the session end. `bt brief --thread <name>` adds the project's latest settled decisions, settled
  ones only.
- UserPromptSubmit runs `bt recall` (`recall.rs`), with no rescan and no model call. It scores settled decisions in
  force by the rare words they share with the prompt (inverse frequency over decision titles) plus a bonus when the
  prompt names a project the decision is about. A decision outside the session's project needs that bonus. The
  session's project is its thread when that thread is a project, else the thread holding 80% of the sessions from
  the working directory. At most two decisions print, each once per session (ids in `recall/<session id>`).
  Paraphrases with no shared words are missed.
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

Routes: Home, Graph (decisions, mentions and over-time views), Items, Log, Search, Thread, Source. `/map?thread=` shows one
project's items and notes. Home reads `/api/today` and `/api/thread` (one call per project in motion, for its next step
and latest decisions); the Graph page reads `/api/kg/graph`, `/api/kg/claims`,
`/api/kg/lineage`, `/api/kg/suggest` (terms with example quotes) and `/api/kg/growth`. `POST /api/kg/entity` returns the claims linked and the unread sessions; `POST /api/kg/extract` starts `skill/extract.sh --entity` in the background. All writes from the app go through `POST /api/*` handlers in
`server.rs`, which call the same functions in `items.rs` and `kg/` as the command line.

Limits: README, section "Limits", and `reference.md`, section "Scale limits".

## Rules in the code

The source carries no comments. The reasons behind rules that the code alone does not show are listed here, by module.

- `db.rs`: the default data directory is made readable by its owner only (it holds session text); a folder named by
  `BACKTRAIL_HOME` may hold other files and is left as it is. Entity ids and pre-version-1 fact ids are 32-bit FNV-1a;
  new fact ids and passage hashes are 64-bit. A session can be named by a unique prefix of its ref, so short refs in
  notes and old exports resolve. Every fact is filed through the quote lookup (`find_quote`), and `relink` uses it
  to follow a quote when a source is read again.
- `config.rs`: a pattern that does not compile prints a warning and becomes `$^`, which matches nothing (an empty
  pattern would match everything). A `threads.json` array is read when there is no `config.json`.
- `ingest/sessions.rs`: the first chunk of a turn is exactly the first `n` characters, so quotes filed under the older
  cut still match; later cuts prefer a line break or a space. A last line without a newline counts only when it is
  valid JSON. When one session id exists in two project folders, the larger file is read, so runs do not swap the
  stored text and lose claim evidence. Modification times have whole-second resolution, so a file longer than the bytes
  read is read again even when its mtime is unchanged. Secrets are redacted per turn as well as per chunk, so a token
  cannot survive by straddling a chunk cut.
- `ingest/mod.rs`: commit matching (`bt match`) is not part of the scan, because a shared two-word phrase marked
  unrelated steps as done. The newest three daily backups are kept. Secret redaction is pattern based and misses
  secrets with no fixed shape.
- `ingest/threads.rs`: a project thread gets a project entity, because the extractor and resolver file an item under
  the thread of its main entity. A short untitled session stays unsorted. A session pinned with `bt assign` keeps its
  thread on every rescan.
- `items.rs`: the thread follows the first named project entity; mentions alone move an item only off a topic thread,
  so a passing mention leaves an item on its project. Each rescan runs `rehome`: an item on a discussion, idea or
  untyped thread whose entities name exactly one project with its own thread moves there (log action `rehome`), which
  covers items filed before that project's thread existed. An item the owner moved is left alone. When the keep rule keeps a fact, a Claude flag of "live"
  or "in effect" is dropped and a "superseded" flag stays.
- `kg/claims.rs`: the speaker and date of a claim come from its passage, not from the caller. A relation between the
  same two entities, in the same direction and of the same kind, is one claim however it is worded. A claim's id is a
  hash of its entities, type and text; a pre-version-1 claim with the same content is the same claim, and a new id
  taken by different content is refused. Relation entities are checked before any entity is created, so a refused
  relation leaves nothing behind.
- `kg/entities.rs`: a built-in topic becomes an entity only when owner turns in `TOPIC_MIN_SESSIONS` sessions mention
  it; ambiguous words (go, swift, bun, node) are left out. Aliases match longest first, so "marvel rivals" does not
  also count as "marvel". A suggested name is a word capitalised mid-sentence in assistant turns (the owner rarely
  capitalises); ties sort by sessions, then name, so the cut at 150 is stable. A model may copy the "Name (kind)"
  suffix from the packet; lookups strip it.
- `kg/query.rs`: a claim is in force on a day when it was said by then and has not ended. It ends on the earliest of
  its `valid_to`, the day its replacement was said, and its `retire` log entry; a superseded claim with none of these
  ends on its last update or the resolver's `stale` check, the day the system noticed. Rejected suggestions with no end
  count as not in force on any day. `BACKTRAIL_EXCLUDE_SESSION` hides one session's claims and passages, for evaluation. Neighbour
  strength is shared claims relative to the size of the entity reached, so a hub does not win by volume.
- `kg/extract.rs`: Unsorted sessions are left out of the general queue but kept when the hook names the session that
  just ended, since work run from a parent folder lands in Unsorted. The cost estimate is the logged read cost plus 0.011 USD per item filed, and falls back to 0.08 USD when no
  run is logged.
- `finish.rs`: open steps of every thread are listed, since a session is often filed under another thread than the
  step it finishes. A step closes only when a second model confirms the passage, because in a test run the first model
  named a step for an unrelated line. Commit subjects were tried as corroboration and dropped: on a busy thread a commit
  lands every day, and embeddings scored unrelated commits above the right one. An item judged by the sweep is skipped
  for a week.
- `resolve.sh` uses a larger model than extraction, because a small model left real matches unresolved.
- `resolve.rs`: word stems are the first five letters, so "removed" and "remove" match; a shared word counts for more
  when few items use it. Reversal candidates add the most specific shared entities, because word overlap misses a
  reversal in other words and a broad entity has hundreds of claims.
- `search.rs`: an FTS query with an alias group uses an explicit AND, since FTS5 rejects a bare space before a
  parenthesised group. When no passage holds every word, passages holding some of them are ranked. Equal scores keep
  passage order, so a query always returns the same list.
- `views.rs`: the short brief prints one line per project, so a session in a folder with many projects is not handed
  all their detail; questions the owner asked print in full, because Claude answers them. Mention edges cost one
  full-text query per thread and are cached until the store changes.
- `main.rs`: a closed stdout (`| head`) exits quietly instead of panicking. The headless extraction session gets no
  brief.
- `migrate.rs`: the keep rule reads tables only, because the views do not exist yet while a store is being opened.
- Mac app: day strings are UTC midnights and the trail uses a UTC calendar, since a local calendar drifts at daylight
  saving changes. Search shows the keyword answer first and replaces it when the embedding answer arrives.
- Web app: the map page keeps the last map and the measured node sizes while the next one loads, since React Flow
  hides an unmeasured node and that showed as flicker. Claims are written in the third person; the app speaks to the
  user.

## Decision graph

The Graph page opens on decisions (`app/src/pages/DecisionGraph.tsx`, data from `kg/decisions.rs`).

- At rest (`/api/kg/decisions`): one card per project with its newest settled decision. Lines: `changed` links whose
  two decisions sit in different projects, project relations (built on, spun out of), and, on hover, the latest
  decision two projects share.
- A project (`?project=`): its decisions plus any decision a link reaches in another project. The page puts them on a
  day axis, one lane per most common other subject, merges `restates` groups into one dot and draws `changed` links
  as arrows.
- A decision (`/api/kg/chain?id=`): every decision reachable through `changed` and `restates` links, grouped into
  steps by how many changes precede each, with quotes and `because` reasons.
