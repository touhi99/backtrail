# Backtrail commands

The complete CLI and the scripts that call it. Hooks, `skill/*.sh` and agents call most of these. Everyday commands are in
[`reference.md`](reference.md#commands).

## All commands

| Command | Purpose |
| --- | --- |
| `ingest [--force]` | Read new or changed sources (new lines only, for a session file that grew) and re-link claims whose chunk changed. `--force` reads every session again in full (after a change to the chunking rule); sessions whose file is gone are kept |
| `serve [--port N]` | Run the app |
| `brief [--thread T \| --all]` | Rescan, then print. With no flag: one line per thread in motion (open and done steps, what waits on the user, the next step) and the questions the user asked. `--thread T`: open questions, next step, recent answers, the latest five settled decisions still in force (each with its date and who confirmed it: `owner-stated`, `owner-accepted` or `from a note`) of one thread. `--all`: the same detail for every thread. A question the user asked and Claude answered is labelled "user asked, Claude answered" |
| `recall [--text PROMPT [--thread T]]` | Without `--text`: the UserPromptSubmit hook; reads the hook JSON on stdin and prints up to two settled decisions that bear on the prompt. With `--text`: list every match and its score, for checking |
| `add <type> --thread --title --entities A,B --source --quote [--body --state --origin --valid-to]` | File one decision, step, question or turn. At least one entity is required; the thread follows the entity. Lists existing items that may already settle it |
| `resolve <id> --by <id> --reason T` | Settle an item with an existing one: a question becomes `answered`, a step or decision `superseded`, `superseded_by` names the settling item |
| `unresolve <id>` | Put a resolved or expired item back to `proposed`; a settled decision that a reversal replaced goes back to `settled` |
| `resolve --expired` | Items whose `valid_to` has passed leave the queue (also done by `resolve-queue`). Model proposals and open questions whose session is more than 14 days old and that are still unconfirmed are parked the same way: state `superseded`, reason in `answer`, `unresolve` brings one back. `bt serve` runs this every 5 minutes. Proposals on a thread whose state is `built`, `dropped` or `parked` are parked the same way. Parking applies to model-proposed items; items the user filed stay as they are |
| `answer <question id> --text T` | Record the user's reply to a question Claude asked; the brief lists open questions with their ids |
| `keep` | Run the keep rule over every proposed claim |
| `resolve-queue [--limit N]`, `resolve-packet <id>`, `resolve-apply <file\|->` | The three halves of `skill/resolve.sh`: unjudged proposed items, the text a model reads for one, and applying its model output. With `--reverses` (`resolve-queue`, `resolve-packet`) they work on settled decisions: the prompt lists the older settled decisions on the same subject (best word and chunk matches, then those sharing the newest decision's most specific entities), and a model output `{"id", "replaces": [..], "reason"}` marks each named older decision `superseded` by the new one. `resolve.sh --reverses` asks a second model to confirm each reversal before it is applied |
| `import <facts.json>` or `import -` | File many claims through the same rule. Decisions, steps, questions and turns need a thread (taken from the source when missing); `state`, `rationale` and `relation` items name entities. `alternative` and `depends` items link a rejected option to its decision, or a step to the step it waits for |
| `done <step id> --by owner\|claude\|evidence --proof "..."` | Close a step; `claude` and `evidence` need proof |
| `flag <id> --as live\|in_effect\|superseded\|done --reason "..."` | Label a proposed item |
| `retire <id> [--by <id>] --reason "..."` | Retire an item; `--by` records the item that replaced it |
| `set-thread <name> --state --summary` | Change a thread's state or summary |
| `link <A> <B> <kind>` | Record a thread-to-thread relation by the user, without a quote |
| `assign <session ref> <thread>` | Pin a session to a thread |
| `search <words> [--thread T] [--kind session\|note\|artifact\|commit] [--state S] [--from DATE] [--to DATE] [--text]` | Search with filters. `--state` keeps chunks whose source has a filed item in that state. `--text` forces keyword-only |
| `embed [--limit N]` | Embed chunks that have no vector yet (needs the embedding server; see the `embeddings` key) |
| `show <ref>` | Print a source's chunks |
| `check [--pairs]` | Report the last 7 days of filing (items filed, how many the user moved to another project or retired, how many `rehome` moved), lost quotes, session files not indexed yet, orphan threads, stale work, PR counts by GitHub state, done steps whose session's PRs all closed unmerged, and claims that a PR merged which GitHub contradicts; `--pairs` lists candidate contradictions |
| `prs [--all]` | Read the GitHub state of PRs that sessions linked (`gh pr list`, one call per repo). Without `--all`, only repos with an open or unchecked PR whose last read is over an hour old. Then checks unchecked pushes to `main` (and on-main ones a week old) with the compare API; the sweep runs this |
| `match` | Mark open steps that a later commit seems to complete as `looks done?` (word-overlap heuristic; it marks unrelated steps that share a phrase, so it runs only when called) |
| `status [--all]` | Session states and graph gaps; lists every session that is not complete or excluded (`--all`: every session). Exits 1 when a session failed or an item has no entity |
| `ended <session id>` | Mark a session ended (the SessionEnd hook runs it) |
| `home` | Print the data directory |
| `artifacts [--manifest list.json]` | Record publish dates and report missing or stale snapshots |
| `extract-input <ref>` | Print one session for a model: the thread's existing items, the open steps of every thread with their ids, the entity names, and the turns |
| `finish <step> --source REF --quote Q [--verified] [--dry]` | Close an open step from a quote found in the source. It closes only with `--verified` (a second model agreed the chunk shows the step finished); without, it is refused. `--dry` prints the model output only |
| `finish-input <step>` | Print a step and the ten chunks written after it that are most like it, for `skill/finish.sh` |
| `finish-evidence <step> --source REF --quote Q` | Print a step and the chunk holding the quote, for the confirming model |
| `finish-queue` | List steps open for more than a day |
| `extract-queue [--session ID] [--limit N] [--entity E] [--no-cap]` | List unread and behind sessions (at most `daily_cap` a day when set, unless `--no-cap`) |
| `extract-log <ref> [--failed 1] --detail "..."` | Record one extraction run and its cost |
| `log [--n N]` | Print the append-only change log |
| `backup` | Copy the database to `<data dir>/backups` (also daily on rescan, newest 3 kept) |
| `kg` | Print claim and entity counts |
| `kg find "<question>"` | Router for history questions: lineage trace, entity page, claims matched by their own words, then the hybrid search (claims that cite the best chunks, else the chunks). Prints "no record" on a miss. When claims about an entity are newer than any shown, a line gives the latest date |
| `kg page <entity> [--budget N]` | Entity page within a token budget: newest claims in full, older ones rolled up by month |
| `--as-of YYYY-MM-DD` on `kg find`, `page`, `trace` and `neighbors` | Answer from the claims in force that day: said by then, not ended yet. A claim ends on the earliest of its `valid_to`, the day its replacement was said and its `retire` log entry. Claims show as `in force then`; "newer claims exist" notes and chunks dated after the day are left out. A claim marked superseded with none of those ends on its last update (proposals parked after 14 days) or on the day of the resolver's `stale` check: the day the system noticed, not the day the world changed |
| `kg trace <entity>` / `kg why <words>` / `kg neighbors <entity>` / `kg passage <words>` | Dated lineage, claim lookup by words, shared-claim neighbours (`neighbors --depth 1-3`, default 1; fan-out capped per hop, hubs ranked down, about 2500 tokens, cuts counted; logged as `kg-query`), the hybrid search routed to claims |
| `kg path <entity A> <entity B>` | Shortest chain of claims between two entities (up to 5 links; the strongest link first among equal chains), each link with its newest claim, date, speaker and quote. Takes `--as-of`; prints "no record" when no chain exists |
| `kg suggest` / `kg entity add <name> [--kind K --alias A]` | Suggest new entities and alias spellings (nothing is written); add one by hand |
| `kg state` | Thread state suggested from session activity alone (nothing is changed) |
| `kg merge <keep id> <drop id>` | Merge a duplicate claim into another: its entities move over and the duplicate is closed as superseded |
| `kg lint` | Graph checks (also part of `check`) |
| `kg export --graph <file>` | Entity graph, not a backup: nodes are entities with kind and claims in force; a link joins two entities when a claim in force names both (weight, first and last day, newest claim). `.graphml` writes GraphML, any other name JSON in NetworkX node-link form. Takes `--as-of` |
| `kg export <file>` / `kg restore <file>` | Backup of entities and all claims as JSONL; each quote is re-located by source on restore |
| `build` | Ingest all sources, create the configured entities (`config.json` key `entities`) and embed (first run), then print how many sessions wait to be read |
| `hooks [status\|install\|remove] [--harness claude,gemini,codex,antigravity]` | Show, add or remove each agent's hooks and skill link (default `claude`; `status` shows all); the old hooks file is copied to `<name>.bak-backtrail` first. Paths and events: [`harnesses.md`](harnesses.md) |
| `agent [claude\|gemini\|codex\|antigravity\|command]` | Print or set the CLI that reads sessions (`config.json` key `agent.cli`) |

`relate <A> <B> <kind> --source <ref> --quote "..." [--state proposed]` files a relation between two threads
(`spun out of`, `forked from`, `built on`, `replaced by`) with a quote from a source. It is stored as a relation claim.
`extract.sh` proposes relations in the same pass as decisions and steps.

## Scripts

| Script | Usage |
| --- | --- |
| `skill/session-end.sh` | SessionEnd hook; reads the hook JSON (`session_id`) on stdin, runs `ingest`, `ended <id>`, then `sweep.sh` in the background |
| `skill/sweep.sh` | One sweep under `<data dir>/sweep.lock`: `ingest`, `embed`, extraction of every queued session, `resolve.sh` and `resolve.sh --reverses` until their queues are empty, `finish.sh --limit 15`. Logs to `<data dir>/logs/sweep.log`. `BACKTRAIL_COMPILE_ON_END=0` skips it |
| `skill/extract.sh [--entity NAME] [--limit N] [--session ID] [--no-cap] \| <session ref>...` | One single-shot model call per session; the answer goes through `bt import -` |
| `skill/resolve.sh [--dry] [--reverses] [--limit N] \| [--dry] [--reverses] <item id>...` | One call per proposed item (`BACKTRAIL_RESOLVE_MODEL`, default sonnet). `--reverses` judges settled decisions, and a second model confirms each reversal |
| `skill/finish.sh [--dry] [--limit N] [step id...]` | Backlog sweep over items older than a day and not judged in the last 7 days |
| `skill/close-step.sh <step id> <source ref> <quote> [--dry]` | Checks the quote with `bt finish --dry`, asks a second model, then closes the item with `--verified` |
| `skill/howto.sh [--dry] [--again] [--limit N] \| [--dry] <step id>...` | Writes the check text for steps waiting on the user; `--again` includes steps judged to need none |
| `install.sh` | Builds the app and binary and links `bt`; `BT_NO_AGENT=1` skips the LaunchAgent, `BT_NO_MAC_APP=1` the Mac app |
| `mac/build.sh` | Builds the Mac app and its icon into `mac/Backtrail.app` |

`--dry` prints each model output as a JSON line and applies nothing. The model scripts set `BACKTRAIL_COMPILE=1`, so the
hooks of the sessions they start exit at once, and run in an empty temporary folder, so no project `CLAUDE.md` is
loaded.
