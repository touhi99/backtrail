-- Tables and indexes. Created on open when missing; changes to existing stores are migrations in db.rs.
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
  nchunks INTEGER DEFAULT 0,
  -- sessions only: how far the file has been read, a hash of the 256 bytes before that point, turns stored
  bytes INTEGER NOT NULL DEFAULT 0,
  tail TEXT,
  nturns INTEGER NOT NULL DEFAULT 0,
  prs_read INTEGER NOT NULL DEFAULT 0,
  -- sessions from a tool other than Claude Code: codex, gemini, or the name in an inbox file
  harness TEXT
);

CREATE TABLE IF NOT EXISTS source_prs(
  source_id INTEGER NOT NULL,
  repo TEXT NOT NULL,
  number INTEGER NOT NULL,
  url TEXT NOT NULL,
  ts TEXT,
  PRIMARY KEY(source_id, url)
);

-- Pull request state as GitHub reports it through `gh pr list`. A PR with no row here is unchecked.
CREATE TABLE IF NOT EXISTS pr_state(
  repo TEXT NOT NULL,
  number INTEGER NOT NULL,
  title TEXT,
  state TEXT NOT NULL,
  created_at TEXT,
  merged_at TEXT,
  closed_at TEXT,
  head TEXT,
  base TEXT,
  checked_at TEXT NOT NULL,
  PRIMARY KEY(repo, number)
);

-- The last `gh pr list` call per repo and its error, if it failed.
CREATE TABLE IF NOT EXISTS pr_repos(
  repo TEXT PRIMARY KEY,
  checked_at TEXT NOT NULL,
  error TEXT
);

CREATE INDEX IF NOT EXISTS sources_thread ON sources(thread, last_ts);

-- Sessions the owner or Claude read and decided hold nothing to file. They leave the pending list.
CREATE TABLE IF NOT EXISTS source_review(
  source_id INTEGER PRIMARY KEY,
  ts TEXT NOT NULL,
  actor TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS chunks(
  id INTEGER PRIMARY KEY,
  source_id INTEGER NOT NULL,
  seq INTEGER,
  locator TEXT,
  role TEXT,
  ts TEXT,
  text TEXT NOT NULL,
  -- hash of the text; the key of its embedding in `vec`
  hash TEXT
);

CREATE INDEX IF NOT EXISTS chunks_src ON chunks(source_id, seq);

-- One embedding per distinct passage text.
CREATE TABLE IF NOT EXISTS vec(
  hash TEXT PRIMARY KEY,
  model TEXT NOT NULL,
  v BLOB NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS fts USING fts5(text, tokenize='porter unicode61');

-- One table of claims: decisions, steps, questions, turns (items) and rationale, state and relation claims.
-- `thread` is the project the claim came from. `state` is the review and work state in one column:
-- proposed, settled, rejected, superseded (any type); open, doing, looks_done, done, parked (steps);
-- open, answered (questions). One quote from one source supports each claim.
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
  -- 'auto' when the keep rule kept the fact, 'held|<reason>' when it left it for review
  review TEXT,
  -- for a step the owner must confirm: where to look and what a good result is
  check_how TEXT,
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
  parent TEXT,
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

CREATE INDEX IF NOT EXISTS log_target ON log(target);

-- What a claim is about. Entities have aliases; a relation has a subject and an object.
CREATE TABLE IF NOT EXISTS entities(
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT,
  created TEXT
);

CREATE TABLE IF NOT EXISTS entity_aliases(
  alias TEXT PRIMARY KEY,
  entity_id TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS entity_aliases_e ON entity_aliases(entity_id);

CREATE TABLE IF NOT EXISTS claim_about(
  claim_id TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'subject',
  PRIMARY KEY(claim_id, entity_id, role)
);

CREATE INDEX IF NOT EXISTS claim_about_e ON claim_about(entity_id, claim_id);

CREATE TABLE IF NOT EXISTS claim_links(
  a TEXT NOT NULL,
  b TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('changed','restates','because')),
  reason TEXT,
  actor TEXT,
  created TEXT,
  PRIMARY KEY(a, b, kind)
);

CREATE INDEX IF NOT EXISTS claim_links_b ON claim_links(b);

-- Claim-to-claim edges beyond changed/restates/because: a considered alternative and step dependencies.
CREATE TABLE IF NOT EXISTS claim_edges(
  a TEXT NOT NULL,
  b TEXT NOT NULL,
  kind TEXT NOT NULL,
  reason TEXT,
  source_id INTEGER,
  chunk_id INTEGER,
  quote TEXT,
  actor TEXT,
  created TEXT,
  PRIMARY KEY(a, b, kind)
);

CREATE INDEX IF NOT EXISTS claim_edges_b ON claim_edges(b);

CREATE TABLE IF NOT EXISTS kg_overrides(
  target TEXT NOT NULL,
  field TEXT NOT NULL,
  value TEXT,
  by TEXT,
  ts TEXT,
  PRIMARY KEY(target, field)
);
