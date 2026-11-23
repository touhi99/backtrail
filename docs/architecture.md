# Architecture

Describes the code as built. The full command list is in `commands.md`; everyday commands are in `reference.md`.

![Components](architecture.svg)

## Overview

One Rust binary, one SQLite file, a web app embedded in the binary and a SwiftUI Mac app (`mac/`) that reads the same
JSON API. The binary writes the database; sources (session logs, notes, git history, artifact snapshots) are read.

- The binary calls the embedding server (Ollama, `127.0.0.1:11434` unless `config.json` names another). Search,
  resolution candidates and the finish sweep use its vectors. When it does not answer, they fall back to word
  matching, and search results carry a note.
- The server listens on `127.0.0.1` and answers `403` to a request whose `Host` or `Origin` header names another
  machine, so a page on another site cannot read or write the database.
- Claims are produced by agent sessions or `skill/extract.sh` and written back through the binary. Every model call in
  `skill/` goes through `skill/ask.py`, which runs the extraction model's CLI (`claude`, `gemini`, `codex`, `agy` or a
  configured command; `bt agent` sets it) and returns one JSON shape.
- Entities are named in a dictionary (`bt kg entity add`); each claim links to the entities its text names, and
  relations between threads come from `skill/extract.sh` or `bt relate`.
  `bt kg lint` lists probable duplicates.

## Terms

| Term | Meaning |
| --- | --- |
| thread | A project. Sessions, notes and claims are filed under one thread; a sub-thread sits under a parent thread, one level deep, and a parent with its sub-threads is a family |
| source | One session, note, artifact snapshot or repo history that Backtrail reads |
| chunk | A piece of a source's text, at most 2,400 characters; the unit that search ranks and a claim quotes |
| claim | One statement in the graph, citing a chunk and a quote from it |
| item | A claim the user works with: a decision, step, question or turn (a change of direction, with the old and new state in its body) |
| graph claim | A `state` (how something is), `rationale` (why a decision was made) or `relation` (between two threads) claim |
| entity | A project, tool, model or topic that claims are about |
| proposed, settled | A model files claims as proposed; the user settles a decision by stating it, accepting it in an app or writing it in a note |
| keep rule | Settles a proposed decision, state or rationale claim automatically when the user typed it in a session, unless its title shares three or more words with an earlier claim of the same type in the same thread; then it is held for review |
| extraction | A model reading a session and proposing claims |
| resolve | A model linking a new claim to earlier ones: a repeat, an answer to an open question, or a decision it reverses |
| sweep | The background job that runs extraction, resolve, the GitHub check and step closing |
| brief | The text a session starts with: threads in motion and their next steps |
| recall | The settled decisions added to a prompt |
| mention edge | A graph link between two threads whose sources name each other |
| user | The person whose sessions are read |

## Modules

| File | Role |
| --- | --- |
| `main.rs` | Module declarations and `cli::run()`. |
| `cli/` | Argument parsing (`mod.rs`), command dispatch (`dispatch.rs`), setup commands (`setup.rs`) and the `kg` command family (`kg.rs`). |
| `db.rs` | Opening the database, query helpers, source refs and the quote lookup that every write rule uses. |
| `migrate.rs` | Schema (`sql/schema.sql`) and store versions in `PRAGMA user_version`; each migration backs up the database and runs in one transaction. |
| `items/` | Work items: the write rule and creation (`mod.rs`), review (`review.rs`), keep/skip curation (`curate.rs`) and state changes (`state.rs`). |
| `views/` | Read-only page data (`brief.rs`, `today.rs`, `thread.rs`, `browse.rs`, `graph.rs`, `focus.rs`), with shared queries in `mod.rs`. `decided()` lists a project's latest settled decisions still in force, with who confirmed each; the brief and `/api/thread` use it. |
| `search.rs` | Hybrid chunk search (BM25 and vectors, merged by reciprocal rank fusion) and the item index. |
| `check.rs` | `check`, `check --pairs`, `match`, `artifacts`, `show` and `log`. |
| `prs/` | `bt prs`: GitHub state of linked PRs through `gh pr list`, one call per repo (`mod.rs`), and the PR part of `bt check` (`audit.rs`). |
| `coverage.rs` | Session states (`complete`, `running`, `unread`, `behind`, `failed`, `excluded`) for `status`, the brief and the extraction queue. |
| `server.rs` | Local HTTP server (`tiny_http`, loopback only): routes, request guard and the background scan. Each route calls one function. |
| `ingest/` | Readers for notes, artifact snapshots and git (`notes.rs`, `git.rs`), thread assignment (`threads.rs`), relinking and the daily backup (`mod.rs`). |
| `ingest/sessions/` | Incremental session store (`mod.rs`), parsing of turns, failed tool calls and PR links (`parse.rs`), PR storage and the one-time backfill (`prs.rs`). |
| `ingest/harness/` | Readers for other agents' sessions (`codex.rs`, `gemini.rs`) and inbox files (`inbox.rs`); each returns the same turn list, which `mod.rs` stores. |
| `hooks.rs` | `bt hooks` for each agent (hooks file, events, skill link) and `emit`, which wraps brief and recall text in the JSON each agent reads. |
| `resolve/` | Resolution: candidate scoring (`candidates.rs`), forward lifecycle (`lifecycle.rs`), reverse analysis (`reverse.rs`) and batch prompts (`packet.rs`). |
| `kg/` | Knowledge graph: claim write rule and relations (`claims.rs`), entities (`entities/`), history queries (`query/`), extraction queue and prompt (`extract.rs`), graph pages (`screen.rs`), lint, export and restore. |
| `embed.rs` | Embedding client, vector table (8-bit values with one scale per vector) and a nearest-neighbour scan that reads vectors from SQLite row by row and keeps only the best `k`. |
| `config.rs`, `clock.rs` | `config.json`, read once per process; UTC timestamps. |
| `src/sql/` | `schema.sql` (tables), `views.sql` (the `item`, `fact`, `thread_stat`, `source_stat` and `entity_stat` read views that pages filter) and `migrate_unify.sql`. Other statements are inline next to the code that runs them. |
| `app/` | React and Vite web app, built to `app/dist/` and embedded into the binary. |
| `mac/` | SwiftUI Mac app, a client of the `/api` routes on `127.0.0.1:8799`. Keeps the last answer per route in memory, so a page shows it at once and refreshes. Built by `mac/build.sh`. Has the same pages and actions as the web app; `scripts/check` fails when an `/api` route is called by only one of the two apps. |

## Data

![Tables and the keys that join them](architecture-data.svg)

- `sources` and `chunks` hold the indexed text; `fts` is an FTS5 index over chunks (Porter stemming). A session's ref is
  `session:<session id>`; commands also take a unique prefix such as `session:1a2b3c4d`. For a session, `bytes` is how
  far the file has been read, `tail` a hash of the 256 bytes before that point and `nturns` the turns stored.
- `source_prs` holds the PR links found in session files (repository, number, URL, time) and shows them on the
  thread. Failed tool calls are stored as `tool` chunks beside the text turns, so the extractor can name a workaround.
- `vec` stores one embedding per distinct chunk text, keyed by a hash, so a growing session embeds only new text.
- `claims` holds every stored claim in one table: decisions, steps, questions and turns (the work items), plus
  `state`, `rationale` and `relation:<kind>` claims. A claim names its thread, cites one chunk and a quote, and carries
  a `state` (`proposed`, `settled`, `rejected`, `superseded`; `open`, `doing`, `looks_done`, `done`, `parked` for steps;
  `open`, `answered` for questions). The `fact` view is the graph's subset: decisions, state, rationale and relations,
  with their entity names; steps, questions and turns are not in it. The columns `text` and `status` are read-only names
  for `title` and `state` that the graph queries use. A relation between projects is a claim about two entities; a
  replaced item points at its replacement in `superseded_by`. `review` is `auto` for a claim the keep rule kept and
  `held|<reason>` for one it left for review. New claim ids carry a 64-bit hash (`d-` plus 16 hex digits); ids from
  before database version 1 keep their 32-bit hash.
- `threads` holds the projects (name, state, area, kind, summary).
- `entities`, `entity_aliases` and `claim_about` say what a claim is about. Decisions are linked to entities by whole-word
  alias match; `state`, `rationale` and `relation` claims name their entities when they are filed.
- `overrides` pins sessions to threads; `kg_overrides` keeps user choices about the graph. `log` records who changed what.
- `claim_links` holds links from a decision (`a`) to an older decision or a reason (`b`): `changed` (the older one no
  longer holds), `restates` (the same decision in other words; both stay true) and `because` (a rationale claim that
  explains it). `resolve.sh --reverses` writes them; a `changed` link needs a second model's yes, as the reversal
  always did. Each rescan labels decisions marked replaced without a link: a resolver repeat or a retire whose reason
  says duplicate becomes `restates`, any other reversal or retire becomes `changed`.
- `claim_edges` holds the extractor's claim-to-claim links: `considered` (chosen decision to rejected alternative)
  and `depends_on` (step to the step it waits for). Each edge keeps the source, chunk and quote that state it; the
  extractor writes one only when the session says it explicitly.

## Processes

The server runs a timer thread every 5 minutes: rescan sources, write a daily backup, embed new chunks up to the
configured batch size.

Hooks, for each connected agent (events per agent: `harnesses.md`, "Support by tool"):

- SessionStart runs `bt brief`, which rescans and prints open questions and threads in motion; embedding is left
  to the timer and the session end. `bt brief --thread <name>` adds the project's latest settled decisions, settled
  ones only.
- UserPromptSubmit runs `bt recall` (`recall.rs`), with no rescan and no model call. It scores settled decisions in
  force by the rare words they share with the prompt (inverse frequency over decision titles) plus a bonus when the
  prompt names a project the decision is about. A decision outside the session's project needs that bonus. The
  session's project is its thread when that thread is a project, else the thread holding 80% of the sessions from
  the working directory. At most two decisions print, each once per session (ids in `recall/<session id>`).
  Paraphrases with no shared words are missed.
- SessionEnd runs `skill/session-end.sh`: `ingest`, marks the session ended, then `skill/sweep.sh` in the background.
  The server also starts `skill/sweep.sh` every 5 minutes while any session is queued or any proposed item is
  unchecked. The sweep holds `<data dir>/sweep.lock`, so only one runs at a time, and drains the queues: extraction
  (`skill/extract.sh`, one small-model call per session), resolution and reverse checks (`skill/resolve.sh`), `bt prs`,
  then `finish.sh --limit 15`. It logs to `<data dir>/logs/sweep.log`. `BACKTRAIL_COMPILE_ON_END=0` skips it.
- Session states (`coverage.rs`): `excluded` (no user turn, Backtrail's own call or a probe prompt, duplicate id, or skipped by
  hand), `complete` (read, and no user turn newer than the read), `running` (file written in the last 30 minutes and
  no end mark), `unread`, `behind` (user turns after the last read; only those turns are sent), and `failed` (5
  failed attempts since the last read; the last error is shown). Unread and behind sessions are queued, including
  Unsorted ones. Every ingest links each item to the entities its text names, so an item reaches the graph without a
  model call when its project or a known alias is named.
- Finished steps: the extraction prompt lists every open step with its id, and the model names the ones the session
  finished, with a quote. `skill/close-step.sh` runs `finish`: the quote must be in the session, and a second model
  (`BACKTRAIL_VERIFY_MODEL`, default opus) must confirm the chunk shows that step finished before it closes;
  otherwise nothing changes. A confirmed step's proof lists the PRs opened and pushes to `main` made in that session, for
  the step's project, with their GitHub state. They are not used to pick the step. `bt match` (shared phrases between step and commit subject) runs only when called. `skill/finish.sh [--dry]` runs
  the same check over steps open for more than a day, using the chunks most like each step. The sessions that finish
  a step are often filed under another thread, so the list covers all threads.

## Search

Keyword (BM25) and vector lists are merged by reciprocal rank fusion, then multiplied by factors for thread state and
citations. Thread names and aliases expand the query. Filed items and `kg find` claims list settled ones before
proposed and superseded ones. Weights and limits: `reference.md`, section "Search ranking".

`bt kg find` uses the same function for its last route: it takes the best chunks from the hybrid search and prints the
claims that cite them (date, speaker, quote), or the chunks when none does. Before that it tries lineage, the entity
page and a match of the question's words against claim text.

## Migration

A database from the earlier layout (items in `nodes`, graph in `claims` and `claim_evidence`, relations in `links`) is
converted the first time the binary opens it. The old file is copied to `backtrail.pre-unify.db` first. Item ids are
kept; an item and the claim copied from it become one row; the conversion runs in one transaction and aborts if the row
count differs from the expected total.

`PRAGMA user_version` holds the database version. Version 1 adds the `bytes`, `tail`, `nturns` and `review` columns,
rewrites session refs from the first 8 characters of the session id to the whole id (in `sources`, `overrides` and
`log`), and runs the keep rule once over every proposed claim, oldest first. The old file is copied to
`backtrail.pre-v1.db` first, and the change runs in one transaction.

## Reading sessions

Claude Code appends to a session file. A changed file is read from `bytes` onward when the 256 bytes before that point
still hash to `tail`; only new turns are added, numbered after the stored ones. A file that shrank, moved or changed
earlier is read again in full. Reading stops at the last complete line, so a line still being written is read on the
next scan. Notes and artifacts are still read again in full when they change; after a scan, `relink` points each claim
whose chunk no longer holds its quote at the chunk that does.

Codex and Gemini CLI files are read again in full when their modification time changes (`ingest/harness/`); stored
turns are replaced, and a file whose session is already stored at the same time only has its path updated. Each
file's working directory sets its project through the same mapping as Claude Code's folder names, so one folder is
one project across agents. Subagent sessions and sessions run from a temporary folder are skipped.

## App

Routes: Home, Graph (decisions, mentions and over-time views), Corrections, Items, Log, Search, Thread, Source. `/map?thread=` shows one
project's items and notes. Home reads `/api/today` and `/api/thread` (one call per project in motion, for its next step
and latest decisions); the Graph page reads `/api/kg/graph`, `/api/kg/claims`,
`/api/kg/lineage`, `/api/kg/suggest` (terms with example quotes) and `/api/kg/growth`. Corrections reads `/api/corrections`: the weekly share of user messages tagged as pushback in `turn_tags` (filled by `skill/corrections.sh`), the kinds per week and the threads with the most tagged messages. A click on a bar, week, kind or thread filters one panel, read from `/api/corrections/messages?kind=&thread=&week=` (the latest 60 matching messages and the total). `POST /api/kg/entity` returns the claims linked and the unread sessions; `POST /api/kg/extract` starts `skill/extract.sh --entity` in the background. All writes from the app go through `POST /api/*` handlers in
`server.rs`, which call the same functions in `items/` and `kg/` as the command line.

Limits: README, section "Limits", and `reference.md`, section "Scale limits".

## Rules in the code

The source carries no comments. The reasons behind rules that the code alone does not show are listed here, by module.

Rust code is formatted with `cargo fmt`. Tables are defined in `src/sql/schema.sql` and read through the views in
`src/sql/views.sql`; a page or command filters a view instead of repeating its joins. Statements stay inline next to
the code that runs them. A change to existing tables is a migration in `src/migrate.rs`.

`scripts/check` runs format, Clippy, tests, TypeScript, ESLint and the Swift build. When `SLOP_CHECK` points at the
local docs linter, it also checks docs and comments; CI runs the script without that optional step.

- `db.rs`: the default data directory is made readable by its user only (it holds session text); a folder named by
  `BACKTRAIL_HOME` may hold other files and is left as it is. Entity ids and pre-version-1 fact ids are 32-bit FNV-1a;
  new fact ids and chunk hashes are 64-bit. A session can be named by a unique prefix of its ref, so short refs in
  notes and old exports resolve. Every claim is filed through the quote lookup (`find_quote`), and `relink` uses it
  to follow a quote when a source is read again.
- `config.rs`: a pattern that does not compile prints a warning and becomes `$^`, which matches nothing (an empty
  pattern would match everything). A `threads.json` array is read when there is no `config.json`.
- `ingest/sessions/`: the first chunk of a turn is exactly the first `n` characters, so quotes filed under the older
  cut still match; later cuts prefer a line break or a space. A last line without a newline counts only when it is
  valid JSON. When one session id exists in two project folders, the larger file is read, so runs do not swap the
  stored text and lose claim evidence. Modification times have whole-second resolution, so a file longer than the bytes
  read is read again even when its mtime is unchanged. Secrets are redacted per turn as well as per chunk, so a token
  cannot survive by straddling a chunk cut.
- `ingest/mod.rs`: commit matching (`bt match`) is not part of the scan, because a shared two-word phrase marked
  unrelated steps as done. The newest three daily backups are kept. Secret redaction is pattern based and misses
  secrets with no fixed shape.
- `ingest/threads.rs`: a build or plan thread gets a project entity, because the extractor and resolver file an item
  under the thread of its main entity. Every other configured thread gets a topic entity, so an item on an idea or
  discussion thread that names no known entity still reaches the graph. A short untitled session stays unsorted. A session pinned with `bt assign` keeps its
  thread on every rescan.
- Sub-threads: a thread with `parent` in config sits under that thread, one level deep. Sessions and items stay on the
  sub-thread. The brief lists a parent with its family's open and done counts and its moving sub-threads indented
  under it, `bt brief --thread <parent>` includes the sub-threads, and recall counts a decision on any member of the
  family as inside the session's project. A parent with an empty `regex` takes no sessions of its own and is left out
  of the orphan report.
- `items/`: the thread follows the first named project entity; mentions alone move an item only off a topic thread,
  so a passing mention leaves an item on its project. Each rescan runs `rehome`: an item on a discussion, idea or
  untyped thread whose entities name exactly one project with its own thread moves there (log action `rehome`), which
  covers items filed before that project's thread existed. An item the user moved is left alone. When the keep rule keeps a claim, a Claude flag of "live"
  or "in effect" is dropped and a "superseded" flag stays.
- `kg/claims.rs`: the speaker and date of a claim come from its chunk, not from the caller. A relation between the
  same two entities, in the same direction and of the same kind, is one claim however it is worded. A claim's id is a
  hash of its entities, type and text; a pre-version-1 claim with the same content is the same claim, and a new id
  taken by different content is refused. Relation entities are checked before any entity is created, so a refused
  relation leaves nothing behind.
- `kg/entities/`: a built-in topic becomes an entity only when user turns in `TOPIC_MIN_SESSIONS` sessions mention
  it; ambiguous words (go, swift, bun, node) are left out. Aliases match longest first, so "react native" does not
  also count as "react". A suggested name is a word capitalised mid-sentence in assistant turns (the user rarely
  capitalises); ties sort by sessions, then name, so the cut at 150 is stable. A model may copy the "Name (kind)"
  suffix from the prompt; lookups strip it.
- `kg/query/`: a claim is in force on a day when it was said by then and has not ended. It ends on the earliest of
  its `valid_to`, the day its replacement was said, and its `retire` log entry; a superseded claim with none of these
  ends on its last update or the resolver's `stale` check, the day the system noticed. Rejected suggestions with no end
  count as not in force on any day. `BACKTRAIL_EXCLUDE_SESSION` hides one session's claims and chunks, for evaluation. Neighbour
  strength is shared claims relative to the size of the entity reached, so a hub does not win by volume.
- `kg/extract.rs`: the queue comes from the session states in `coverage.rs`, Unsorted sessions included, since work run
  from a parent folder lands in Unsorted. The usage estimate is one call per read plus one per item filed, averaged
  over logged reads, and falls back to 5 calls when none is logged.
- `finish.rs`: open steps of every thread are listed, since a session is often filed under another thread than the
  step it finishes. A step closes only when a second model confirms the chunk, because in a test run the first model
  named a step for an unrelated line. Commit subjects were tried as corroboration and dropped: on a busy thread a commit
  lands every day, and embeddings scored unrelated commits above the right one. An item judged by the sweep is skipped
  for a week.
- `prs/`: a PR belongs to a session when Claude Code wrote a `pr-link` record for it there and GitHub dates its creation
  inside the session's span (10 minutes either side). On 2026-11-21, 435 of 449 links passed that test, and 429 of 448
  matched a `gh pr create` in the same session. A step's proof names only PRs in repos of its own project or project
  family. A repo belongs to a project by the `origin` remote of its `repos` folder in `config.json`; a repo not listed
  there goes to the thread whose name matches it, else to the one build thread whose `regex` matches its name, else
  to none. A project page lists the PRs of the repos it and its sub-threads own, whichever session linked them.
  Commit hashes were tried first: most commit output in transcripts is cut by `tail` or condensed, and PRs are
  squash-merged, so a session's own commit hash is not on `main` even when its work landed. A PR with no GitHub row
  shows as unchecked. The sweep reads a repo again only when it has an open or unchecked PR and
  its last read is over an hour old. `bt check` lists done steps whose closing session's PRs were all closed without
  merging (and no merged PR shares the head branch), and claims that say a PR merged when GitHub says otherwise or
  gives a merge date more than a day from the one stated. A claim naming a PR number is checked only when exactly one
  repo fits: the repo it names (folder or GitHub name), else the repos sessions of its thread linked with that number.
  The PR note that `finish` writes into a step's proof is left out of that scan.
- Pushes to `main` or `master`: a ref update line (`abc..def  main -> main`) after a `To github.com/<repo>` line in a
  tool result is stored per session. GitHub's compare API says whether the pushed commit is still an ancestor of the
  branch; with no common ancestor, the commit lookup tells "no longer on main" from "not on GitHub". On-main results
  are checked again after a week, since history can be rewritten. A push whose output lost its `To` line is skipped.
- `resolve.sh` uses a larger model than extraction, because a small model left real matches unresolved.
- `resolve/candidates.rs`: word stems are the first five letters, so "removed" and "remove" match; a shared word counts for more
  when few items use it. Reversal candidates add the most specific shared entities, because word overlap misses a
  reversal in other words and a broad entity has hundreds of claims.
- `search.rs`: an FTS query with an alias group uses an explicit AND, since FTS5 rejects a bare space before a
  parenthesised group. When no chunk holds every word, chunks holding some of them are ranked. Equal scores keep
  chunk order, so a query always returns the same list.
- `views/brief.rs`: the short brief prints one line per project, so a session in a folder with many projects is not
  handed all their detail; questions the user asked print in full, because Claude answers them.
- `views/graph.rs`: mention edges cost one full-text query per thread and are cached until the database changes.
- `cli/dispatch.rs`: a closed stdout (`| head`) exits quietly instead of panicking. The headless extraction session
  gets no brief.
- `migrate.rs`: the keep rule reads tables only, because the views do not exist yet while a database is being opened.
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
