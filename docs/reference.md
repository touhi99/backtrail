# Backtrail reference

Details that the README leaves out: every command, every configuration key, search ranking, setup for embeddings and
hooks, and the parts that do not scale unchanged.

## Commands

Commands run as `bt <command>`.

| Command | Purpose |
| --- | --- |
| `ingest [--force]` | Read new or changed sources (new lines only, for a session file that grew) and re-link facts whose passage changed. `--force` reads every session again in full (after a change to the chunking rule); sessions whose file is gone are kept |
| `serve [--port N]` | Run the app |
| `brief [--thread T]` | Rescan, then print open questions, threads in motion and stale items |
| `add <type> --thread --title --source --quote [--body --state --origin]` | File one decision, step, question or turn |
| `import <facts.json>` or `import -` | File many facts through the same rule. Decisions, steps, questions and turns need a thread (taken from the source when missing); `state`, `rationale` and `relation` items name entities |
| `done <step id> --by owner\|claude\|evidence --proof "..."` | Close a step; `claude` and `evidence` need proof |
| `flag <id> --as live\|in_effect\|superseded\|done --reason "..."` | Label a proposed item |
| `retire <id> [--by <id>] --reason "..."` | Retire an item; `--by` records the item that replaced it |
| `set-thread <name> --state --summary` | Change a thread's state or summary |
| `link <A> <B> <kind>` | Record a project-to-project relation by the owner, without a quote |
| `assign <session ref> <thread>` | Pin a session to a thread |
| `search <words> [--thread T] [--kind session\|note\|artifact\|commit] [--state S] [--from DATE] [--to DATE] [--text]` | Search with filters. `--state` keeps passages whose source has a filed item in that state. `--text` forces keyword-only |
| `embed [--limit N]` | Embed passages that have no vector yet (needs the embedding server; see the `embeddings` key) |
| `show <ref>` | Print a source's passages |
| `check [--pairs]` | Report lost quotes, session files that were never indexed, orphan threads, stale work; `--pairs` lists candidate contradictions |
| `match` | Mark open steps that a later commit seems to complete as `looks done?` (word-overlap heuristic, unproven) |
| `pending [--days N]` | List recent sessions that no item cites |
| `artifacts [--manifest list.json]` | Record publish dates and report missing or stale snapshots |
| `extract-input <ref>` | Print one session for a model: the thread's existing items, the entity names, and the turns |
| `extract-queue [--session ID] [--limit N] [--entity E]` | List sessions worth extracting (at most `daily_cap` a day) |
| `extract-log <ref> [--failed 1] --detail "..."` | Record one extraction run and its cost |
| `log [--n N]` | Print the append-only change log |
| `backup` | Copy the store to `<data dir>/backups` (also daily on rescan, newest 3 kept) |
| `kg` | Print claim and entity counts |
| `kg find "<question>"` | Router for history questions: lineage trace, entity page, claims matched by their own words, then the hybrid search (claims that cite the best passages, else the passages). Prints "no record" on a miss. When claims about an entity are newer than any shown, a line gives the latest date |
| `kg page <entity> [--budget N]` | Entity page within a token budget: newest claims in full, older ones rolled up by month |
| `kg trace <entity>` / `kg why <words>` / `kg neighbors <entity>` / `kg passage <words>` | Dated lineage, claim lookup by words, shared-claim neighbours (2 hops, capped), the hybrid search routed to claims |
| `kg suggest` / `kg entity add <name> [--kind K --alias A]` | Suggest new entities and alias spellings (nothing is written); add one by hand |
| `kg state` | Thread state suggested from session activity alone (nothing is changed) |
| `kg merge <keep id> <drop id>` | Merge a duplicate claim into another: its entities move over and the duplicate is closed as superseded |
| `kg lint` | Graph checks (also part of `check`) |
| `kg export <file>` / `kg restore <file>` | Backup of entities and all claims as JSONL; each quote is re-located by source on restore |
| `build` | Ingest all sources, create the configured entities (`config.json` key `entities`) and embed (first run) |
| `demo` | Write a demo store (requires `BACKTRAIL_HOME`) |
| `hooks [status\|install\|remove]` | Show, add or remove the Claude Code hooks in `~/.claude/settings.json`; the old file is copied to `settings.json.bak-backtrail` first |

`relate <A> <B> <kind> --source <ref> --quote "..." [--state proposed]` files a relation between two projects
(`spun out of`, `forked from`, `built on`, `replaced by`) with a quote from a source. It is stored as a relation claim.
`extract.sh` proposes relations in the same pass as decisions and steps.

App screens: Today, thread, source (cited passage highlighted), search, items, log, and the graph. The graph shows
entities as nodes (size is the number of claims, a line means claims about both). Selecting a node opens its
lineage, shared-claim neighbours and claims, each with date, speaker, quote and a link to the source passage. The
panel also lists suggested entities and aliases that can be added with one click. A project page still has a map of
its own notes. The app can tick a step, confirm or dismiss `looks done?`, answer a question, add an idea, and accept
or reject suggested items. Thread lineage drawn on the graph comes from relation claims (`relate`, `link` and `extract.sh` write them).

## Configuration

`<data dir>/config.json` is optional. Without it, each Claude Code project folder is one thread and only memory notes are
indexed. It holds custom thread rules, note folders and repositories:

| Key | Content |
| --- | --- |
| `workspace` | Root folder that note paths are relative to |
| `note_dirs` | Folders of Markdown notes to index |
| `screens_dir` | Folder of HTML design files to index |
| `threads` | Thread definitions: `name`, `area`, `state`, `summary`, `kind`, `regex` (matches session titles and project folders). When set, one thread per project folder is no longer created |
| `note_rules`, `artifact_rules` | Lists of `[thread name, regex]` pairs that assign notes and artifacts to threads |
| `generic_projects` | Project folder names that never decide a thread (default `["home"]`) |
| `repos` | Repository path to thread table, used for commit matching |
| `aliases` | Alternative names used for mention edges |
| `compile` | `daily_cap` (8): extraction runs a day |
| `port` | Server port (8799) |
| `embeddings` | On by default with `url` `http://127.0.0.1:11434` and `model` `embeddinggemma-2:270m`. Keys: `url` (an Ollama-compatible server, `http://` only), `model`, and optionally `query_prefix`, `document_prefix` (default to `task: search result | query: ` and `title: none | text: ` for `embeddinggemma` models, `search_query: ` and `search_document: ` for `nomic` models, empty otherwise) and `batch_cap` (passages per background run, default 2000). `false` turns embeddings off. Changing `model` discards stored vectors and re-embeds. |

## Search ranking

Search ranks passages by keyword (BM25) and, when the embedding server answers, by vector similarity. Otherwise keyword only, with a note in the result. The two rankings
are merged by reciprocal rank fusion. Each passage's score is then adjusted by what the store knows about it: the
thread's state (dropped or parked lowers it), and whether a kept, suggested or rejected item cites the passage or its
source. The result lists these reasons. Filed items are searched in a separate index over title and body.

`bt embed` stores one vector per distinct passage text, keyed by a hash of the text, so a session that grows
embeds only its new passages. Vectors come from a local Ollama-compatible server and stay in the same SQLite file.

## Embeddings

Install Ollama, run `ollama pull embeddinggemma-2:270m` (378 MB), then run `bt embed`. No config key is needed for the
default server. Until then, search is keyword only. A first run over 11,964 passages took 954 s on one Apple Silicon
Mac; later runs embed only new passages.

Model comparison, 2026-11-13, on one store of 12,282 passages. Queries are the titles of 519 filed items; the target is
the passage each item cites. Vector ranking only. "Low overlap" is the 152 queries that share under 30% of their words
with the target passage.

| Model | Hit@10 | MRR@10 | Hit@10, low overlap | MRR@10, low overlap | Embedding speed |
| --- | --- | --- | --- | --- | --- |
| `embeddinggemma-2:270m` | 51.8% | 0.348 | 27.6% | 0.143 | 12.5 passages/s |
| `nomic-embed-text` | 49.7% | 0.327 | 16.4% | 0.056 | 24 passages/s |

Titles were written by a model from the cited passage, so the queries are not typed by people.

## Hooks

`bt hooks install` adds both hooks to `~/.claude/settings.json`; `bt hooks remove` takes them out. By hand:

- `SessionStart`: `bt brief` prints open questions and threads in motion.
- `SessionEnd`: `<path>/skill/session-end.sh` runs `ingest` and `embed` in the background when a session closes. It
  does not call a model. Setting `BACKTRAIL_COMPILE_ON_END=1` in the hook's environment also runs `skill/extract.sh` on the
  session with a small model, which costs tokens.

## Demo store

A store with invented content: `BACKTRAIL_HOME=/tmp/bt-demo bt demo`, then
`BACKTRAIL_HOME=/tmp/bt-demo bt serve --port 8800`. Screenshots from it are in `docs/demo/`.

## Scale limits

Graph queries (entity lookup, claim filter, two-hop) were timed on a scratch store of up to 4 million passages and
stayed under 1 ms at p95; full-text search grew linearly to a p95 of about 650 ms at 4 million passages, so it needs
an entity or time prefilter past about 2 million.

Keyword search uses SQLite FTS5, which handles far more than the 11,800 passages in the test store.
Parts that do not scale unchanged: vectors are compared by brute force over an in-memory copy (about 36 MB at 768 dimensions and 11,800
passages, a few hundred thousand passages at most before it needs an index); `graph()` rebuilds the whole graph per
request with one FTS query per thread; the item index is rebuilt in memory whenever facts change. None of these has
been measured beyond that size.

## Source layout

Rust code is formatted with `cargo fmt`. Longer SQL statements are in `src/sql/*.sql` and loaded with `include_str!`; the
first line of each file names the function that uses it. A few use `{where}`, `{limit}`, `{exclude}` or `{ids}` markers
that the caller replaces. Short statements stay inline.

## Other limits

- Thread assignment is regular-expression based; unmatched sources stay in `Unsorted`.
- One extraction run of a session on a small model cost 0.0019 to 0.0033 USD in three runs (3,100 to 6,200 input tokens).
- Only the text of user and assistant turns is stored, in chunks of at most 2,400 characters. Tool calls, tool output,
  thinking and images are not. On one store this was about 0.5% of the bytes in the transcript files (5.9 MB of
  1.8 GB over 26 days).
- Claude Code removes session transcripts after 30 days by default (setting `cleanupPeriodDays`). A session is indexed
  when it ends and the store keeps its text, but a session that was never indexed before its file was removed is lost.
  The data directory then holds the only copy of the text, so it needs its own backup.
- The session-end hook and the session-start brief hook are configured separately in the Claude Code settings.
