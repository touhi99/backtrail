# Coding agents

Backtrail works with Claude Code, Gemini CLI, Codex CLI and Antigravity. One machine usually connects one or two of
them. Each is connected in up to four ways; only the first two are needed for Backtrail to work.

| Part | Needed | What it does |
| --- | --- | --- |
| Session reader | yes | Indexes the tool's transcripts, so its sessions appear in search, projects and the brief |
| Extraction model | yes | The CLI that reads sessions for decisions and checks them (`bt agent`); one per machine |
| Hooks | no | Add the brief at session start and settled decisions to each prompt; read a session as soon as it ends |
| Skill | no | `skill/SKILL.md`, the Agent Skills format all four tools load, for filing items by hand |

Without a session-end hook, the sweep, which runs every 5 minutes, reads a session once its file has not changed for
30 minutes, and the agent gets no
Backtrail context unless it runs `bt brief` itself.

## Support by tool

| | Claude Code | Gemini CLI | Codex CLI | Antigravity |
| --- | --- | --- | --- | --- |
| Session reader | built in | built in | built in | converter needed |
| Extraction model | `claude -p` | `gemini -p` | `codex exec --ephemeral` | `agy -p` |
| Hooks file | `~/.claude/settings.json` | `~/.gemini/settings.json` | `~/.codex/hooks.json` | `~/.gemini/config/hooks.json` |
| Session start | `SessionStart` | `SessionStart` | `SessionStart` | `SessionStart` |
| Each prompt | `UserPromptSubmit` | `BeforeAgent` | `UserPromptSubmit` | `PreInvocation` |
| Session end | `SessionEnd` | `SessionEnd` | sweep | sweep |
| Skill link | `~/.claude/skills/backtrail` | `~/.agents/skills/backtrail` | `~/.agents/skills/backtrail` | `~/.gemini/config/skills/backtrail` |
| Tested | yes | no | no | no |

`CODEX_HOME` and `GEMINI_CLI_HOME` move the Codex and Gemini paths. Codex runs a new hook only after it is trusted
in `/hooks`. Codex and Antigravity have no session-end event (`Stop` fires every turn), so the sweep reads those
sessions.

## Connecting

`./install.sh` asks which tools to connect, offering the ones found on `PATH`, installs hooks and the skill for
those only, and asks which one reads sessions when more than one is chosen. Without a terminal it takes
`BT_HARNESSES=gemini,codex` and `BT_AGENT=gemini`; `BT_HOOKS=0` skips hooks and skills.

Later changes:

```
bt hooks status                                  # every tool
bt hooks install --harness gemini,codex          # hooks and skill link
bt hooks remove --harness codex                  # Backtrail's entries only; the old file is kept as *.bak-backtrail
bt agent gemini                                  # the reading agent
```

Hook commands are `bt brief --hook <tool>` and `bt recall --hook <tool>`: the same text as for Claude Code,
wrapped in the JSON each tool reads (`hookSpecificOutput.additionalContext` for Gemini CLI and Codex, `injectSteps`
for Antigravity).

## Extraction model

Reading sessions, checking steps and writing how-to pages are prompts sent through `skill/ask.py`, using the CLI in
`config.json`:

```json
{"agent": {"cli": "gemini", "small": "gemini-3-flash", "large": "gemini-3-pro"}}
```

| `cli` | Models |
| --- | --- |
| `claude` (default) | One model per job, below |
| `gemini`, `codex`, `antigravity` | The `small`, `medium` and `large` keys; an unset tier falls back to another set tier, and with none set the CLI's default model runs every job |
| `command` | the shell command in `command` reads the prompt on stdin and prints the answer |

For `small`, pick a fast, low-cost model: it reads every session. For `large`, pick the strongest model the CLI
offers: it confirms reversals and finished steps, where a wrong yes changes the graph. `medium` can be either.

| Job | Tier | Claude default | Claude override |
| --- | --- | --- | --- |
| Read a session | `small` | haiku | `BACKTRAIL_COMPILE_MODEL` |
| Link claims to earlier ones | `medium` | sonnet | `BACKTRAIL_RESOLVE_MODEL` |
| Find finished steps | `medium` | sonnet | `BACKTRAIL_FINISH_MODEL` |
| Confirm a reversal or a finished step | `large` | opus | `BACKTRAIL_VERIFY_MODEL` |

`command` covers any tool with a headless mode, for example `"ollama run qwen3:32b"`. Prompts start with `BACKTRAIL-CALL`, so a
session a tool records for the call is left unread. `skill/ask.py --check` prints the CLI in use and fails when it
is missing.

## Session readers

| Tool | Files read | Project from |
| --- | --- | --- |
| Claude Code | `~/.claude/projects/<dir>/<id>.jsonl` | the folder name |
| Codex CLI | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` and `archived_sessions/` | `cwd` in `session_meta` |
| Gemini CLI | `~/.gemini/tmp/<project>/chats/session-*.jsonl` or `.json` | `.project_root` next to `chats/` |
| Any other tool | `<data dir>/inbox/**/*.jsonl` in the format below | `cwd` in the header |

A session started in the same folder lands in the same project whichever tool ran it: the working directory is
mapped to a project name the way Claude Code names its project folders. Sessions run from a temporary folder are
skipped.

- Codex: user turns come from `user_message` events (text after `## My request for Codex:` when present),
  assistant turns from `agent_message` events, and the title from `session_index.jsonl`. Files without those events
  fall back to `response_item` messages, minus injected context blocks. Subagent threads are skipped.
- Gemini CLI: `user` and `gemini` records become turns, thought parts are dropped, `$rewindTo` and
  `$patch.removeIds` are applied, and `summary` is the title. Subagent chats are skipped.

## Session format

UTF-8 JSON Lines for the inbox. The first line is the header; each further line is one turn.

```json
{"backtrail_session":1,"harness":"antigravity","id":"7a1e9c40-aaaa-4bbb-8ccc-0123456789ab","cwd":"/home/dev/shop","title":"Onboarding copy"}
{"role":"user","ts":"2026-11-21T07:00:00.000Z","text":"Keep onboarding to three screens"}
{"role":"assistant","ts":"2026-11-21T07:00:20.000Z","text":"Three screens, signup moved to the end."}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `backtrail_session` | yes | format version, `1` |
| `harness` | yes | short tool name, lower case |
| `id` | yes | the tool's own session id, stable across rewrites; stored as `session:<harness>-<id>` |
| `cwd` | no | absolute working directory; sets the project |
| `title` | no | session title |
| `role` | yes | `user` for what the person typed, `assistant` for the agent's reply text |
| `ts` | yes | ISO 8601 UTC with `Z`, for example `2026-11-21T07:00:00.000Z` |
| `text` | yes | plain text of the turn |

- One file per session. Rewrite the whole file when the session grows; a file is read again when its modification
  time changes, and its stored turns are replaced.
- Leave out tool calls, tool output, system prompts, injected context and hidden reasoning. The converter format
  carries typed text only; the Claude Code reader also keeps failed tool calls. Turns shorter than 15 characters are
  dropped.
- Timestamps increase through the file; decision reading resumes from the last turn it read by timestamp.
- Other roles and malformed lines are skipped.

## Adding a tool

Steps for an agent working in this repository:

1. Find where the tool stores transcripts and read its source or docs for the record format: the session id,
   working directory, title, and which records hold the person's typed text and the agent's reply.
2. Pick the route:
   - **Converter** (any language, no Rust change): write one inbox file per session in the format above. Put it in
     `<data dir>/converters/` as an executable; the sweep runs each one every 5 minutes before ingest, with the
     inbox path as its only argument.
   - **Built-in reader**: add `src/ingest/harness/<tool>.rs` with `files()` and `parse()` returning a `Session`,
     and add it to `READERS` in `src/ingest/harness/mod.rs`. Codex and Gemini CLI are the examples.
3. For hooks, add the tool to `Harness` in `src/hooks.rs`: its hooks file, events, skill folder, and the JSON its
   hooks read in `emit`.
4. Check with a scratch database, so the real one stays untouched:

   ```
   BACKTRAIL_HOME=/tmp/bt-try bt ingest
   sqlite3 /tmp/bt-try/backtrail.db "SELECT ref, harness, project, title, nturns FROM sources WHERE harness IS NOT NULL"
   BACKTRAIL_HOME=/tmp/bt-try bt extract-input session:<harness>-<id>
   ```

   Expected: one row per session with the right project and title, and `extract-input` lists the typed turns
   without tool output.

## Limits

- Gemini CLI, Codex CLI and Antigravity support follows each tool's source and docs as of November 2026 and has not
  run against the real tools. Codex hook output fields come from its docs summary.
- Codex compressed rollouts and the `history_mode: paginated` layout are not read.
- Antigravity writes `<app data>/brain/<conversation id>/.system_generated/logs/transcript.jsonl`; its record format
  is undocumented, so its sessions need a converter written against a real file.
- Small local models read through `command` file little: a 3B model through Ollama completed the run and filed
  nothing.
