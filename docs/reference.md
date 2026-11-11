# Backtrail reference

Details that the README leaves out: every command, every configuration key, search ranking, setup for embeddings and
hooks, and the parts that do not scale unchanged.

## Commands

| Command | Purpose |
| --- | --- |
| `ingest` | Read new or changed sources and re-link items to their quotes |
| `serve [--port N]` | Run the app |
| `brief [--thread T]` | Rescan, then print open questions, threads in motion and stale items |
| `add <type> --thread --title --source --quote [--body --state --origin]` | File one item |
| `import <items.json>` or `import -` | File many items through the same rule |
| `done <step id> --by owner\|claude\|evidence --proof "..."` | Close a step; `claude` and `evidence` need proof |
| `flag <id> --as live\|in_effect\|superseded\|done --reason "..."` | Label a proposed item |
| `retire <id> [--by <id>] --reason "..."` | Retire an item, optionally linking its replacement |
| `set-thread <name> --state --summary` | Change a thread's state or summary |
| `link <A> <B> <kind>` | Record a thread-to-thread relation |
| `assign <session ref> <thread>` | Pin a session to a thread |
| `search <words> [--thread T] [--kind session\|note\|artifact\|commit] [--state S] [--from DATE] [--to DATE] [--text]` | Search with filters. `--state` keeps passages whose source has a filed item in that state. `--text` forces keyword-only |
| `embed [--limit N]` | Embed passages that have no vector yet (needs the `embeddings` key) |
| `show <ref>` | Print a source's passages |
| `check [--pairs]` | Report lost quotes, orphan threads, stale work; `--pairs` lists candidate contradictions |
| `match` | Mark open steps that a later commit seems to complete as `looks done?` (word-overlap heuristic, unproven) |
| `pending [--days N]` | List recent sessions that no item cites |
| `artifacts [--manifest list.json]` | Record publish dates and report missing or stale snapshots |
| `compile-input <ref>` | Print the trimmed text of one session for a model |
| `compile-queue [--session ID] [--limit N]` | List sessions worth compiling |
| `compile-log <ref> [--failed 1] --detail "..."` | Record one compile run and its cost |
| `log [--n N]` | Print the append-only change log |
| `backup` | Copy the store to `BacktrailBackups` (also daily on rescan, 14 kept) |
| `demo` | Write a demo store (requires `BACKTRAIL_HOME`) |

`relate <A> <B> <kind> --source <ref> --quote "..." [--state proposed]` files a relation between two threads
(`spun out of`, `forked from`, `built on`, `replaced by`) with a quote from a source. `relate-input <ref>` prints the
packet for a model to propose relations.

App screens: Today, thread, source (cited passage highlighted), search, items, log, and the map. The map shows each
thread as a bar from its first to its last session, with an arrow where one thread came out of, or was built on,
another; selecting an arrow shows its quote. The app can tick a step, confirm or dismiss `looks done?`, answer a
question, add an idea, and accept or reject suggested items.

## Configuration

`<data dir>/config.json` holds everything specific to one machine:

| Key | Content |
| --- | --- |
| `workspace` | Root folder that note paths are relative to |
| `note_dirs` | Folders of Markdown notes to index |
| `screens_dir` | Folder of HTML design files to index |
| `threads` | Thread definitions: `name`, `area`, `state`, `summary`, `kind`, `regex` (matches session titles) |
| `note_rules`, `art_rules` | Regular expressions that assign notes and artifacts to threads |
| `repos` | Repository path to thread table, used for commit matching |
| `aliases` | Alternative names used for mention edges |
| `compile` | `min_owner_turns` (4), `daily_cap` (8), `max_chars` (24000) |
| `port` | Server port (8799) |
| `embeddings` | Optional. `url` (an Ollama-compatible server, `http://` only), `model`, and optionally `query_prefix`, `document_prefix` (default to `search_query: ` and `search_document: ` for `nomic` models, empty otherwise) and `batch_cap` (passages per background run, default 2000). Absent: embeddings are off. Changing `model` discards stored vectors and re-embeds. |

## Search ranking

Search ranks passages by keyword (BM25) and, when embeddings are configured, by vector similarity. The two rankings
are merged by reciprocal rank fusion. Each passage's score is then adjusted by what the store knows about it: the
thread's state (dropped or parked lowers it), and whether a kept, suggested or rejected item cites the passage or its
source. The result lists these reasons. Filed items are searched in a separate index over title and body.

`backtrail embed` stores one vector per distinct passage text, keyed by a hash of the text, so a session that grows
embeds only its new passages. Vectors come from a local Ollama-compatible server and stay in the same SQLite file.

## First run on a new machine

Written from the steps used on the development machine. Not yet run on a second machine.

1. Build as in the README.
2. Create `<data dir>/config.json` with at least `workspace` and `note_dirs`. Threads are added with `regex` rules.
3. Run `backtrail ingest`. The first run reads every session log and may take a minute.
4. Run `backtrail serve` and open `http://127.0.0.1:8799`.
5. Optional embeddings: install Ollama, run `ollama pull nomic-embed-text` (274 MB), add the `embeddings` key, then run
   `backtrail embed`. The first full run embedded 11,473 passages in 472 s on one Apple Silicon Mac. Later runs embed
   only new passages.
6. Optional hooks, in `~/.claude/settings.json`:
   - `SessionStart`: `<path>/target/release/backtrail brief` prints open questions and threads in motion.
   - `SessionEnd`: `<path>/skill/session-end.sh` runs `ingest` and `embed` in the background when a session closes.
     It does not call a model. Setting `BACKTRAIL_COMPILE_ON_END=1` in the hook's environment also compiles the
     session with a small model, which costs tokens.

A demo store with invented content: `BACKTRAIL_HOME=/tmp/tm-demo backtrail demo`, then
`BACKTRAIL_HOME=/tmp/tm-demo backtrail serve --port 8800`. Screenshots from it are in `docs/demo/`.

## Scale limits

Keyword search uses SQLite FTS5, which handles far more than the present 11,800 passages. Parts that do not scale
unchanged: vectors are compared by brute force over an in-memory copy (about 36 MB at 768 dimensions and 11,800
passages, a few hundred thousand passages at most before it needs an index); `graph()` rebuilds the whole graph per
request with one FTS query per thread; the item index is rebuilt in memory whenever items change. None of these has
been measured beyond the present store.

## Other limits

- Thread assignment is regular-expression based; unmatched sources stay in `Unsorted`.
- A compile run of one session on a small model cost 0.0013 and 0.0041 USD in two runs (3,300 and 10,700 input tokens).
- The session-end hook and the session-start brief hook are configured separately in the Claude Code settings.
