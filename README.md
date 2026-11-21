# Backtrail

Backtrail keeps the trail of work done with Claude Code. Sessions and notes are archived in one SQLite file, and the
decisions, steps and open questions in them are filed by project, each with the line it came from. Claude reads the
trail when a session starts; a web app and a Mac app show it by project, over time and as a graph.

| | |
| --- | --- |
| ![Home](docs/demo/home.png) | ![Project page](docs/demo/thread.png) |

## How it works

- A session is read when it ends. Each item is filed with a verbatim quote, checked against the source.
- Each prompt is matched against settled decisions; up to two that bear on it are added to Claude's context.
- A decision is settled when the user typed it, accepts it in an app, or it comes from a note.
- A step closes when a session shows it finished and a second model agrees.
- Unconfirmed items are parked after 14 days, or when their project is built, dropped or parked.
- Things the work mentions (projects, tools, models) form a graph across projects.

Detail: [`docs/architecture.md`](docs/architecture.md).

## Requirements

Rust 1.85+, Node.js and the `claude` CLI. Optional: Ollama (`embeddinggemma-2:270m`) for search by meaning; Swift
(Xcode command line tools) for the Mac app.

## Install

```
./install.sh
bt hooks install
bt build
```

On macOS, `install.sh` also installs a login agent that runs the server and builds the Mac app; elsewhere, run
`bt serve`. Options and first-run notes: [`docs/reference.md`](docs/reference.md).

## Usage

- `bt build` offers to read the newest 20 sessions, so the brief has content on day one.
- Web app: `http://127.0.0.1:8799`. Mac app: `mac/Backtrail.app`.
- `bt search <words>` searches with cited sources; `bt brief` prints what was decided and what is next.
- Full command list: [`docs/reference.md`](docs/reference.md).

## Data

- Reads: Claude Code sessions (`~/.claude/projects`), and the note folders and git repositories in `config.json`.
- Writes: the data directory (database, backups, logs), `~/.claude/settings.json` (hooks), and the login agent on macOS.
- Sends: session text to Claude through the `claude` CLI after each session, about 0.05 to 0.20 USD each (one
  small-model read, one check per item filed, one link check per new decision); passage text to the embedding server (`127.0.0.1:11434`).

## Limits

- Search holds vectors in memory: a few hundred thousand passages at most.
- The quote check shows a quote exists, not that the title reads it fairly.
- An item's project can be wrong. `bt check` reports how many items filed in the last 7 days were moved to another
  project or retired by hand.

## License

MIT. See [`LICENSE`](LICENSE).
