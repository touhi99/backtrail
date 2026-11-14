-- db.rs: open
PRAGMA busy_timeout=8000;

PRAGMA journal_mode=WAL;

PRAGMA synchronous=NORMAL;

CREATE TABLE IF NOT EXISTS sources(
  id INTEGER PRIMARY KEY,
  ref TEXT UNIQUE NOT NULL,
  kind TEXT NOT NULL,
  title TEXT,
  project TEXT,
  thread TEXT,
  first_ts TEXT,
  last_ts TEXT,
  mtime INTEGER,
  path TEXT,
  nchunks INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS chunks(
  id INTEGER PRIMARY KEY,
  source_id INTEGER NOT NULL,
  seq INTEGER,
  locator TEXT,
  role TEXT,
  ts TEXT,
  text TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS chunks_src ON chunks(source_id, seq);

CREATE VIRTUAL TABLE IF NOT EXISTS fts USING fts5(text, tokenize='porter unicode61');

-- One table of facts: decisions, steps, questions, turns (items) and rationale, state and relation claims.
-- `thread` is the project the fact came from. `state` is the review and work state in one column:
-- proposed, settled, rejected, superseded (any type); open, doing, looks_done, done, parked (steps);
-- open, answered (questions). One quote from one source supports each fact.
-- `text` and `status` are read-only names the knowledge-graph queries use: text is title, status is state
-- as proposed, active, retracted or superseded.
CREATE TABLE IF NOT EXISTS claims(
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  thread TEXT,
  title TEXT NOT NULL,
  body TEXT,
  state TEXT NOT NULL DEFAULT 'proposed',
  origin TEXT NOT NULL DEFAULT 'compiled',
  speaker TEXT,
  asserted_at TEXT,
  valid_to TEXT,
  superseded_by TEXT,
  done_by TEXT,
  answer TEXT,
  tag TEXT,
  stale TEXT,
  source_id INTEGER,
  chunk_id INTEGER,
  quote TEXT,
  stated TEXT NOT NULL DEFAULT 'stated',
  created TEXT,
  updated TEXT,
  text TEXT GENERATED ALWAYS AS (title) VIRTUAL,
  status TEXT GENERATED ALWAYS AS (
    CASE
      WHEN state='rejected' THEN 'retracted'
      WHEN state='superseded' OR stale LIKE 'superseded%' THEN 'superseded'
      WHEN state='proposed' THEN 'proposed'
      ELSE 'active'
    END
  ) VIRTUAL
);

CREATE INDEX IF NOT EXISTS claims_thread ON claims(thread, type);

CREATE INDEX IF NOT EXISTS claims_type ON claims(type, state, asserted_at);

CREATE INDEX IF NOT EXISTS claims_source ON claims(source_id);

-- A project. Facts name their thread; the entity of the same name carries claims about the project.
CREATE TABLE IF NOT EXISTS threads(
  name TEXT PRIMARY KEY,
  state TEXT,
  area TEXT,
  kind TEXT,
  body TEXT,
  origin TEXT NOT NULL DEFAULT 'seed',
  created TEXT,
  updated TEXT
);

CREATE TABLE IF NOT EXISTS artifact_meta(
  id TEXT PRIMARY KEY,
  title TEXT,
  updated TEXT
);

CREATE TABLE IF NOT EXISTS scanned(
  path TEXT PRIMARY KEY,
  mtime INTEGER
);

CREATE TABLE IF NOT EXISTS overrides(
  ref TEXT PRIMARY KEY,
  thread TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS log(
  id INTEGER PRIMARY KEY,
  ts TEXT,
  actor TEXT,
  action TEXT,
  target TEXT,
  detail TEXT
);
