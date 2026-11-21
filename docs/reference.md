# Backtrail reference

Details that the README leaves out: the everyday commands, every configuration key, search ranking, setup for
embeddings and hooks, and the parts that do not scale unchanged.

## Commands

Everyday commands:

| Command | Purpose |
| --- | --- |
| `build` | Ingest sources, create configured entities and embed on first run |
| `serve [--port N]` | Run the app |
| `brief [--thread T \| --all]` | Print what was decided and what is next |
| `search <words>` | Search with cited sources |
| `show <ref>` | Print one source's chunks |
| `status [--all]` | Session states and graph gaps |
| `check [--pairs]` | Filing, quote and stale-work checks |
| `hooks [...]`, `agent [...]` | Connect agents or change the extraction CLI |
| `home`, `backup` | Data directory and a database backup |

Every other command is called by hooks, `skill/*.sh` or an agent, not typed by hand. The full list, including the
queue and prompt steps and the knowledge-graph commands, is in [`commands.md`](commands.md).

## Install options

- `install.sh` links the binary as `bt` in `$BT_BIN_DIR` (default `~/.local/bin`). On macOS it also writes and loads
  the LaunchAgent `~/Library/LaunchAgents/dev.backtrail.serve.plist`, which runs `backtrail serve` from login, and
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
| `threads` | Thread definitions: `name`, `area`, `state`, `summary`, `kind`, `regex` (matches session titles and project folders), `parent` (the name of another thread, one level deep). When set, one thread per project folder is no longer created |
| `note_rules`, `artifact_rules` | Lists of `[thread name, regex]` pairs that assign notes and artifacts to threads |
| `generic_projects` | Project folder names left out when choosing a thread (default `["home"]`) |
| `repos` | Repository folder to thread table: their git history is read, and a pull request belongs to the thread that owns its repo |
| `aliases` | Alternative names used for mention edges |
| `compile` | Extraction settings (the key keeps its old name): `daily_cap`, extraction runs a day; unset means no limit |
| `agent` | The CLI that reads sessions: `cli` is `claude` (default), `gemini`, `codex`, `antigravity` or `command`; `small`, `medium`, `large` name the models for the CLIs other than `claude`; `command` is a shell command that reads the prompt on stdin and prints the answer. `BACKTRAIL_AGENT` overrides `cli`. See [`harnesses.md`](harnesses.md#extraction-model) |
| `port` | Server port (8799) |
| `embeddings` | On by default with `url` `http://127.0.0.1:11434` and `model` `embeddinggemma-2:270m`. Keys: `url` (an Ollama-compatible server, `http://` only), `model`, and optionally `query_prefix`, `document_prefix` (default to `task: search result | query: ` and `title: none | text: ` for `embeddinggemma` models, `search_query: ` and `search_document: ` for `nomic` models, empty otherwise) and `batch_cap` (chunks per background run, default 2000). Changing `model` discards stored vectors and re-embeds. |

## Search ranking

Search ranks chunks by keyword (BM25) and, when the embedding server answers, by vector similarity. Otherwise keyword only, with a note in the result. The two rankings
are merged by reciprocal rank fusion. Each chunk's score is then adjusted by its thread's state (dropped or parked
lowers it) and by whether a kept, suggested or rejected item cites the chunk or its source. The result lists these
reasons. Filed items are searched in a separate index over title and body and listed
kept states first (settled, done, open, answered, looks done, doing), then proposed and superseded ones, each group by
text relevance. `kg find` orders claims the same way: active, then proposed, then superseded.

`bt embed` stores one vector per distinct chunk text, keyed by a hash of the text, so a session that grows
embeds only its new chunks. Vectors come from a local Ollama-compatible server and stay in the same SQLite file.

## Embeddings

`install.sh` requires Ollama and pulls `embeddinggemma-2:270m` (378 MB); `bt build` then embeds every chunk. No
config key is needed for the default server. A first run over 11,964 chunks took 954 s on one Apple Silicon
Mac; later runs embed only new chunks.

Model comparison, 2026-11-13, on one database of 12,282 chunks. Queries are the titles of 519 filed items; the target is
the chunk each item cites. Vector ranking only. "Low overlap" is the 152 queries that share under 30% of their words
with the target chunk.

| Model | Hit@10 | MRR@10 | Hit@10, low overlap | MRR@10, low overlap | Embedding speed |
| --- | --- | --- | --- | --- | --- |
| `embeddinggemma-2:270m` | 51.8% | 0.348 | 27.6% | 0.143 | 12.5 chunks/s |
| `nomic-embed-text` | 49.7% | 0.327 | 16.4% | 0.056 | 24 chunks/s |

Titles were written by a model from the cited chunk, so the queries are not typed by people.

## Hooks

`bt hooks install` adds the Claude Code hooks to `~/.claude/settings.json`; `--harness gemini,codex,antigravity`
adds the other agents' hooks ([`harnesses.md`](harnesses.md#support-by-tool)); `bt hooks remove` takes them out. By
hand, for Claude Code:

- `SessionStart`: `bt brief` prints one line per thread in motion. Claude runs `bt brief --thread <name>` for the project at hand.
- `UserPromptSubmit`: `bt recall` adds up to two settled decisions that match the prompt.
- `SessionEnd`: `<path>/skill/session-end.sh` runs `ingest`, marks the session ended and starts `skill/sweep.sh` in
  the background. The sweep reads every queued session with `skill/extract.sh`, judges every unjudged proposed item
  with `skill/resolve.sh`, links settled decisions to older ones and to their reasons with `resolve.sh --reverses`, and
  runs `finish.sh --limit 15`. All of these make model calls. Extraction also names open steps the session finished;
  `skill/close-step.sh` closes each one (see `finish`). `BACKTRAIL_COMPILE_ON_END=0` in the hook's environment skips
  the sweep. The server starts the same sweep every 5 minutes while work is queued.

## First run on a machine with existing sessions

```
./install.sh
bt hooks install
bt build
```

- `bt build` indexes every session and note, then embeds. On one Mac, 457 sessions and 136 notes were indexed in 4 s and
  11,728 chunks embed in about 15 minutes. When Ollama does not answer, it prints how to start it.
- The brief and the graph are empty at first: decisions come from a model reading a session. The sweep reads every unread
  session, including earlier history, once the server runs; `bt skip <ref>` excludes a session from reading. A
  session takes one small-model call to read plus one check call per item filed: about 6 calls per session in
  practice, counted against the plan or API key of the agent CLI set in `agent`.
- Without `config.json`, each Claude Code project folder becomes a thread named after the folder. Thread rules,
  entities and note folders are set in `config.json` (see Configuration).
- The model scripts call the agent CLI set in `agent`, so their usage counts against the account that is logged in on that machine.
  A second machine or account sees only its own sessions; `bt kg export <file>` and `bt kg restore <file>` carry claims
  across.
- `bt hooks install` writes absolute paths to this checkout, so it is run on each machine.

## Scale limits

Graph queries (entity lookup, claim filter, two-hop) were timed on a scratch database of up to 4 million chunks and
stayed under 1 ms at p95; full-text search grew linearly to a p95 of about 650 ms at 4 million chunks, so it needs
an entity or time prefilter past about 2 million.

Keyword search uses SQLite FTS5, which handles far more than the 11,800 chunks in the test database.
Parts that do not scale unchanged: vectors are 8-bit (772 bytes each at 768 dimensions) and scanned
from SQLite on each search without a copy in memory: 0.11 s and 14 MB peak at 15,600 chunks, 0.28 s and 12 MB at
188,000 (measured on repeated vectors); 8-bit top-10 results match 32-bit ones 98% of the time; `graph()` rebuilds the whole graph per
request with one FTS query per thread; the item index is rebuilt in memory whenever claims change. None of these has
been measured beyond that size.

## Other limits

- Thread assignment is regular-expression based; unmatched sources stay in `Unsorted`.
- One extraction run of a session on a small model took 3,100 to 6,200 input tokens in three runs.
- User and assistant text is stored in chunks of at most 2,400 characters, and a failed tool call is stored with its
  tool name, input summary and error excerpt. Other tool output, thinking and images are not. On one database this was
  about 0.5% of the bytes in the transcript files (5.9 MB of 1.8 GB over 26 days).
- Claude Code removes session transcripts after 30 days by default (setting `cleanupPeriodDays`). A session is indexed
  when it ends and the database keeps its text, but a session that was never indexed before its file was removed is lost.
  The data directory then holds the only copy of the text, so it needs its own backup.
