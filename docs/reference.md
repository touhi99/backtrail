# Backtrail reference

Details that the README leaves out: every command, every configuration key, search ranking, setup for embeddings and
hooks, and the parts that do not scale unchanged.

## Commands

Commands run as `bt <command>`.

| Command | Purpose |
| --- | --- |
| `ingest [--force]` | Read new or changed sources (new lines only, for a session file that grew) and re-link facts whose passage changed. `--force` reads every session again in full (after a change to the chunking rule); sessions whose file is gone are kept |
| `serve [--port N]` | Run the app |
| `brief [--thread T \| --all]` | Rescan, then print. With no flag: one line per thread in motion (open and done steps, what waits on the owner, the next step) and the questions the owner asked. `--thread T`: open questions, next step, recent answers, the latest five settled decisions still in force (each with its date and who confirmed it: `owner-stated`, `owner-accepted` or `from a note`) of one thread. `--all`: the same detail for every thread. A question the owner asked and Claude answered is labelled "owner asked, Claude answered" |
| `recall [--text PROMPT [--thread T]]` | Without `--text`: the UserPromptSubmit hook; reads the hook JSON on stdin and prints up to two settled decisions that bear on the prompt. With `--text`: list every match and its score, for checking |
| `add <type> --thread --title --entities A,B --source --quote [--body --state --origin --valid-to]` | File one decision, step, question or turn. At least one entity is required; the thread follows the entity. Lists existing items that may already settle it |
| `resolve <id> --by <id> --reason T` | Settle an item with an existing one: a question becomes `answered`, a step or decision `superseded`, `superseded_by` names the settling item |
| `unresolve <id>` | Put a resolved or expired item back to `proposed`; a settled decision that a reversal replaced goes back to `settled` |
| `resolve --expired` | Items whose `valid_to` has passed leave the queue (also done by `resolve-queue`). Claude's proposals and open questions whose session is more than 14 days old and that are still unconfirmed are parked the same way: state `superseded`, reason in `answer`, `unresolve` brings one back. `bt serve` runs this every 5 minutes. Proposals on a thread whose state is `built`, `dropped` or `parked` are parked the same way. Parking applies to Claude's items; items the owner filed stay as they are |
| `answer <question id> --text T` | Record the owner's reply to a question Claude asked; the brief lists open questions with their ids |
| `keep` | Run the keep rule over every proposed fact |
| `resolve-queue [--limit N]`, `resolve-packet <id>`, `resolve-apply <file\|->` | The three halves of `skill/resolve.sh`: unjudged proposed items, the text a model reads for one, and applying its verdict. With `--reverses` (`resolve-queue`, `resolve-packet`) they work on settled decisions: the packet lists the older settled decisions on the same subject (best word and passage matches, then those sharing the newest decision's most specific entities), and a verdict `{"id", "replaces": [..], "reason"}` marks each named older decision `superseded` by the new one. `resolve.sh --reverses` asks a second model to confirm each reversal before it is applied |
| `import <facts.json>` or `import -` | File many facts through the same rule. Decisions, steps, questions and turns need a thread (taken from the source when missing); `state`, `rationale` and `relation` items name entities |
| `done <step id> --by owner\|claude\|evidence --proof "..."` | Close a step; `claude` and `evidence` need proof |
| `flag <id> --as live\|in_effect\|superseded\|done --reason "..."` | Label a proposed item |
| `retire <id> [--by <id>] --reason "..."` | Retire an item; `--by` records the item that replaced it |
| `set-thread <name> --state --summary` | Change a thread's state or summary |
| `link <A> <B> <kind>` | Record a thread-to-thread relation by the owner, without a quote |
| `assign <session ref> <thread>` | Pin a session to a thread |
| `search <words> [--thread T] [--kind session\|note\|artifact\|commit] [--state S] [--from DATE] [--to DATE] [--text]` | Search with filters. `--state` keeps passages whose source has a filed item in that state. `--text` forces keyword-only |
| `embed [--limit N]` | Embed passages that have no vector yet (needs the embedding server; see the `embeddings` key) |
| `show <ref>` | Print a source's passages |
| `check [--pairs]` | Report the last 7 days of filing (items filed, how many the owner moved to another project or retired, how many `rehome` moved), lost quotes, session files not indexed yet, orphan threads, stale work; `--pairs` lists candidate contradictions |
| `match` | Mark open steps that a later commit seems to complete as `looks done?` (word-overlap heuristic; it marks unrelated steps that share a phrase, so it runs only when called) |
| `pending [--days N]` | List recent sessions that no item cites |
| `artifacts [--manifest list.json]` | Record publish dates and report missing or stale snapshots |
| `extract-input <ref>` | Print one session for a model: the thread's existing items, the open steps of every thread with their ids, the entity names, and the turns |
| `finish <step> --source REF --quote Q [--verified] [--dry]` | Close an open step from a quote found in the source. It closes only with `--verified` (a second model agreed the passage shows the step finished); without, it is refused. `--dry` prints the verdict only |
| `finish-input <step>` | Print a step and the ten passages written after it that are most like it, for `skill/finish.sh` |
| `finish-evidence <step> --source REF --quote Q` | Print a step and the passage holding the quote, for the confirming model |
| `finish-queue` | List steps open for more than a day |
| `extract-queue [--session ID] [--limit N] [--entity E] [--no-cap]` | List sessions worth extracting (at most `daily_cap` a day unless `--no-cap`) |
| `extract-log <ref> [--failed 1] --detail "..."` | Record one extraction run and its cost |
| `log [--n N]` | Print the append-only change log |
| `backup` | Copy the store to `<data dir>/backups` (also daily on rescan, newest 3 kept) |
| `kg` | Print claim and entity counts |
| `kg find "<question>"` | Router for history questions: lineage trace, entity page, claims matched by their own words, then the hybrid search (claims that cite the best passages, else the passages). Prints "no record" on a miss. When claims about an entity are newer than any shown, a line gives the latest date |
| `kg page <entity> [--budget N]` | Entity page within a token budget: newest claims in full, older ones rolled up by month |
| `--as-of YYYY-MM-DD` on `kg find`, `page`, `trace` and `neighbors` | Answer from the claims in force that day: said by then, not ended yet. A claim ends on the earliest of its `valid_to`, the day its replacement was said and its `retire` log entry. Claims show as `in force then`; "newer claims exist" notes and passages dated after the day are left out. A claim marked superseded with none of those ends on its last update (proposals parked after 14 days) or on the day of the resolver's `stale` check: the day the system noticed, not the day the world changed |
| `kg trace <entity>` / `kg why <words>` / `kg neighbors <entity>` / `kg passage <words>` | Dated lineage, claim lookup by words, shared-claim neighbours (`neighbors --depth 1-3`, default 1; fan-out capped per hop, hubs ranked down, about 2500 tokens, cuts counted; logged as `kg-query`), the hybrid search routed to claims |
| `kg path <entity A> <entity B>` | Shortest chain of claims between two entities (up to 5 links; the strongest link first among equal chains), each link with its newest claim, date, speaker and quote. Takes `--as-of`; prints "no record" when no chain exists |
| `kg suggest` / `kg entity add <name> [--kind K --alias A]` | Suggest new entities and alias spellings (nothing is written); add one by hand |
| `kg state` | Thread state suggested from session activity alone (nothing is changed) |
| `kg merge <keep id> <drop id>` | Merge a duplicate claim into another: its entities move over and the duplicate is closed as superseded |
| `kg lint` | Graph checks (also part of `check`) |
| `kg export --graph <file>` | Entity graph, not a backup: nodes are entities with kind and claims in force; a link joins two entities when a claim in force names both (weight, first and last day, newest claim). `.graphml` writes GraphML, any other name JSON in NetworkX node-link form. Takes `--as-of` |
| `kg export <file>` / `kg restore <file>` | Backup of entities and all claims as JSONL; each quote is re-located by source on restore |
| `build` | Ingest all sources, create the configured entities (`config.json` key `entities`) and embed (first run); in a terminal, offers to read the newest 20 unread sessions |
| `demo` | Write a demo store (requires `BACKTRAIL_HOME`) |
| `hooks [status\|install\|remove]` | Show, add or remove the Claude Code hooks in `~/.claude/settings.json`; the old file is copied to `settings.json.bak-backtrail` first |

`relate <A> <B> <kind> --source <ref> --quote "..." [--state proposed]` files a relation between two threads
(`spun out of`, `forked from`, `built on`, `replaced by`) with a quote from a source. It is stored as a relation claim.
`extract.sh` proposes relations in the same pass as decisions and steps.

App screens: Home (the trail of the last two weeks, and one card per project in motion with its next step and latest
decisions, and a link to each project with open questions), project page (latest decisions with who confirmed each,
open questions to answer, steps, a search limited to the project; proposed
items sit under the Archive view as "Unconfirmed suggestions"), source (cited passage highlighted), search, items, log,
and the graph. The graph shows
entities as nodes (size is the number of claims, a line means claims about both). Selecting a node opens its
lineage, shared-claim neighbours and claims, each with date, speaker, quote and a link to the source passage. The
panel also lists suggested entities and aliases that can be added with one click. A project page still has a map of
its own notes. The app can tick a step, confirm or dismiss `looks done?`, answer a question, add an idea, and accept
or reject suggested items. Thread lineage drawn on the graph comes from relation claims (`relate`, `link` and `extract.sh` write them).

## Install options

- `install.sh` links the binary as `bt` in `$BT_BIN_DIR` (default `~/.local/bin`). On macOS it also writes and loads
  the login agent `~/Library/LaunchAgents/dev.backtrail.serve.plist`, which runs `backtrail serve` from login, and
  builds `mac/Backtrail.app` when Swift is present. `BT_NO_AGENT=1` and `BT_NO_MAC_APP=1` skip those. Elsewhere, run
  `bt serve`.
- `BACKTRAIL_HOME` sets the data directory: default `~/Library/Application Support/Backtrail` on macOS,
  `~/.local/share/backtrail` elsewhere. The database is `0600` in a `0700` folder.
- `mac/build.sh` rebuilds the Mac app alone.

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
| `generic_projects` | Project folder names left out when choosing a thread (default `["home"]`) |
| `repos` | Repository path to thread table, used for commit matching |
| `aliases` | Alternative names used for mention edges |
| `compile` | `daily_cap` (8): extraction runs a day |
| `port` | Server port (8799) |
| `embeddings` | On by default with `url` `http://127.0.0.1:11434` and `model` `embeddinggemma-2:270m`. Keys: `url` (an Ollama-compatible server, `http://` only), `model`, and optionally `query_prefix`, `document_prefix` (default to `task: search result | query: ` and `title: none | text: ` for `embeddinggemma` models, `search_query: ` and `search_document: ` for `nomic` models, empty otherwise) and `batch_cap` (passages per background run, default 2000). `false` turns embeddings off. Changing `model` discards stored vectors and re-embeds. |

## Search ranking

Search ranks passages by keyword (BM25) and, when the embedding server answers, by vector similarity. Otherwise keyword only, with a note in the result. The two rankings
are merged by reciprocal rank fusion. Each passage's score is then adjusted by what the store knows about it: the
thread's state (dropped or parked lowers it), and whether a kept, suggested or rejected item cites the passage or its
source. The result lists these reasons. Filed items are searched in a separate index over title and body and listed
kept states first (settled, done, open, answered, looks done, doing), then proposed and superseded ones, each group by
text relevance. `kg find` orders claims the same way: active, then proposed, then superseded.

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

- `SessionStart`: `bt brief` prints one line per thread in motion. Claude runs `bt brief --thread <name>` for the project at hand.
- `SessionEnd`: `<path>/skill/session-end.sh` runs `ingest` and `embed` in the background when a session closes, then
  `skill/extract.sh` on the session and `skill/resolve.sh` on the unjudged proposed items. Both make model calls.
  Extraction also names open steps the session finished; `skill/close-step.sh` closes each one (see `finish`).
  `BACKTRAIL_COMPILE_ON_END=0` in the hook's environment skips them. It then runs `skill/resolve.sh --reverses
  --limit 10`, which links the newest settled decisions not yet checked to older ones (changed, restates) and to the
  reasons from their session (because); `BACKTRAIL_LINK_ON_END=0` skips it.

## First run on a machine with existing sessions

```
./install.sh
bt hooks install
bt build
```

- `bt build` indexes every session and note, then embeds. On one Mac, 457 sessions and 136 notes were indexed in 4 s and
  11,728 passages embed in about 15 minutes. Without Ollama it prints how to install it and search stays keyword only.
- The brief and the graph are empty at first: decisions come from a model reading a session. The `SessionEnd` hook reads
  only the session that just ended, so earlier history stays unread. In a terminal, `bt build` offers to read the newest
  20 sessions; otherwise it prints the commands (`skill/extract.sh --no-cap --limit N`, then `skill/resolve.sh`). A
  session costs about 0.01 USD to read on a small model, plus about 0.01 USD per item filed for the check that follows;
  0.05 to 0.15 USD per session in practice.
- Without `config.json`, each Claude Code project folder becomes a thread named after the folder. Thread rules,
  entities and note folders are set in `config.json` (see Configuration).
- The model scripts call the `claude` CLI, so their usage counts against the account that is logged in on that machine.
  A second machine or account sees only its own sessions; `bt kg export <file>` and `bt kg restore <file>` carry claims
  across.
- `bt hooks install` writes absolute paths to this checkout, so it is run on each machine.

## Demo store

A store with invented content: `BACKTRAIL_HOME=/tmp/bt-demo bt demo`, then
`BACKTRAIL_HOME=/tmp/bt-demo bt serve --port 8800`. Screenshots from it are in `docs/demo/` (home, project page, search,
graph, items).

## Scale limits

Graph queries (entity lookup, claim filter, two-hop) were timed on a scratch store of up to 4 million passages and
stayed under 1 ms at p95; full-text search grew linearly to a p95 of about 650 ms at 4 million passages, so it needs
an entity or time prefilter past about 2 million.

Keyword search uses SQLite FTS5, which handles far more than the 11,800 passages in the test store.
Parts that do not scale unchanged: vectors are compared by brute force over an in-memory copy (about 36 MB at 768 dimensions and 11,800
passages, a few hundred thousand passages at most before it needs an index); `graph()` rebuilds the whole graph per
request with one FTS query per thread; the item index is rebuilt in memory whenever facts change. None of these has
been measured beyond that size.

## Scripts

| Script | Usage |
| --- | --- |
| `skill/session-end.sh` | SessionEnd hook; reads the hook JSON (`session_id`) on stdin. In the background: `ingest`, `embed`, `extract.sh` on the session, `resolve.sh --limit 20`, `finish.sh --limit 15`. `BACKTRAIL_COMPILE_ON_END=0` skips the model calls; `resolve.sh --reverses --limit 10` links decisions unless `BACKTRAIL_LINK_ON_END=0` |
| `skill/extract.sh [--entity NAME] [--limit N] [--session ID] [--no-cap] \| <session ref>...` | One single-shot model call per session; the answer goes through `bt import -` |
| `skill/resolve.sh [--dry] [--reverses] [--limit N] \| [--dry] [--reverses] <item id>...` | One call per proposed item (`BACKTRAIL_RESOLVE_MODEL`, default sonnet). `--reverses` judges settled decisions, and a second model confirms each reversal |
| `skill/finish.sh [--dry] [--limit N] [step id...]` | Backlog sweep over items older than a day and not judged in the last 7 days |
| `skill/close-step.sh <step id> <source ref> <quote> [--dry]` | Checks the quote with `bt finish --dry`, asks a second model, then closes the item with `--verified` |
| `skill/howto.sh [--dry] [--again] [--limit N] \| [--dry] <step id>...` | Writes the check text for steps waiting on the owner; `--again` includes steps judged to need none |
| `install.sh` | Builds the app and binary and links `bt`; `BT_NO_AGENT=1` skips the login agent, `BT_NO_MAC_APP=1` the Mac app |
| `mac/build.sh` | Builds the Mac app and its icon into `mac/Backtrail.app` |

`--dry` prints each verdict as a JSON line and applies nothing. The model scripts set `BACKTRAIL_COMPILE=1`, so the
hooks of the sessions they start exit at once, and run in an empty temporary folder, so no project `CLAUDE.md` is
loaded.

## Source layout

Rust code is formatted with `cargo fmt`. Tables are defined in `src/sql/schema.sql` and read through the views in
`src/sql/views.sql`; a page or command filters a view instead of repeating its joins. Statements stay inline next to
the code that runs them. A change to existing tables is a migration in `src/migrate.rs`.

Source files carry no comments; `// SAFETY:` is the exception. Names and types say what code does, and the reasons
behind non-obvious rules are in `architecture.md`, section "Rules in the code". `scripts/check` runs every check
(format, Clippy, tests, TypeScript, ESLint, Swift build, docs and comment lint) and CI runs the same script.

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
