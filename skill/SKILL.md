---
name: backtrail
description: File decisions, steps and questions into the Backtrail store from real sessions and notes, as suggestions the user accepts or rejects. Use when asked to update Backtrail, compile recent sessions, or review what a thread is waiting on.
---

# Backtrail thinking job

Backtrail indexes Claude Code sessions, notes and artifact snapshots. Facts are filed against threads and must
cite an indexed source with a verbatim quote. The store is read through the `backtrail` binary
(`target/release/backtrail` in the project folder). Never write status into notes.

## Cheap path (default)

Extraction is overview work and does not need a large model. `skill/extract.sh <session ref>...` (or `--session ID`,
`--limit N`, `--entity NAME`) runs one single-shot call per session on a small model (`BACKTRAIL_COMPILE_MODEL`, default
`haiku`) with no tools, no project instructions and no saved session. It pipes `backtrail extract-input <ref>` in and
`backtrail import -` out, then records the cost with `extract-log`. One call returns decisions, steps, questions,
state and rationale claims, and relations, so a session is read once. `skill/session-end.sh` is the SessionEnd hook
body: it runs `ingest` and `embed`, and starts `extract.sh` and `resolve.sh` in the background unless `BACKTRAIL_COMPILE_ON_END=0`.
While `BACKTRAIL_COMPILE` is set, `brief` and the hook exit at once, so an extraction run cannot start another. Main
sessions do not file facts themselves.

Other commands for keeping the store accurate without raw SQL:
- `backtrail flag <id> --as live|in_effect|superseded|done --reason "..."` labels a proposed item after checking it
  against later items, notes and the current code. Flag only with evidence; when unsure, flag `live`.
- `backtrail retire <id> --by <new id> --reason "..."` marks an item replaced. Record a reversal as a Turn
  (`add turn`) with the old and new state in the body.
- `backtrail assign <session ref> <thread>` pins a session to a thread.

## Steps (manual path)

1. `backtrail ingest` to pick up new sources. Artifacts are read with the Artifact tool and saved to
   `<data dir>/artifacts/<id>/index.html` first.
2. `backtrail pending --days 3` lists recent sessions that no item cites.
3. `backtrail show <ref>` prints a source. Read it before filing anything.
4. File each finding with
   `backtrail add <decision|step|question|turn> --thread "<thread>" --title "<=110 chars" --body "<=320 chars" --entities "<main subject>,<others>" --source "<ref>" --quote "<verbatim text from that source>" --origin suggested --state proposed`
   or several at once with `backtrail import facts.json` (also takes `state`, `rationale` and `relation` items with `entities`). Every item names at least one entity (`bt kg entity` lists them) and the thread follows the entity. Add `--valid-to YYYY-MM-DD` for an item that names a relative day. After filing, the command lists existing items that may already settle the new one; if one does, run `backtrail resolve <new id> --by <existing id> --reason "..."`. The command rejects a missing source, a quote not
   found in the source, or text over the limit.
5. Close a step only with evidence: `backtrail done <id> --by claude --proof "<what shows it>"`.
   Check the claim first (run the command, read the code, list the git tags).
6. `backtrail check` reports lost quotes, orphan threads, notes nothing cites, stale work. `backtrail check --pairs`
   lists item pairs that may contradict each other; judge each and record a turn or decision for real conflicts.
6a. `backtrail resolve-queue` lists proposed items no model has judged yet; `skill/resolve.sh` judges them (entities, thread, `valid_to`, whether the store already settles them), one call per item.
6b. `backtrail built <id> --evidence "<what was built>" --check "<where to look and what a good result is>"` shows in the app as "Built, check it".
7. `backtrail match` compares open steps with commits and marks likely matches `looks done?`; the user confirms.
8. Artifacts: run the Artifact tool's `list`, save `[{id,title,updated}]` to a file, run `backtrail artifacts --manifest <file>`.
   Re-read each missing or stale artifact with `action read, path index.html, out_dir <data dir>/artifacts/<id>` (use a
   subagent when there are many, to keep page text out of the main context), then `backtrail ingest`.
9. `backtrail search <words>` answers a question from the store with cited sources. When an answer is worth
   keeping, file it back as a decision or turn with `backtrail add`, so it is found next time.
10. `backtrail link <A> <B> "built on"` records a relation between two threads. `backtrail log` shows the
    append-only change log.

## Rules

- File suggestions as `--state proposed`. The binary keeps a decision, rationale or state claim on its own when the quote
  is a turn the user typed and says it directly, and it is not a likely repeat of an earlier fact; everything else stays
  `proposed` until the user accepts it in the app.
- Cite a session as `session:<session id>`, or a unique prefix of it (`session:<first 8 characters>`).
- A step the user must confirm by hand carries `--check "<where to look and what a good result is>"` (a screen, URL, file path or command). The app shows it as "How to check".
- A title is a fair reading of the quote, not a guess. If the source does not say it, do not file it.
- Questions are things only the user can decide. Do not file a question the sources already answer.
- Nothing about buying or ordering is filed as a task.
- Model work stays in this skill. The `backtrail` binary never calls a model.
