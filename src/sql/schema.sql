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

CREATE TABLE IF NOT EXISTS nodes(
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  thread TEXT,
  title TEXT NOT NULL,
  body TEXT,
  state TEXT,
  area TEXT,
  origin TEXT NOT NULL DEFAULT 'compiled',
  done_by TEXT,
  answer TEXT,
  source_id INTEGER,
  chunk_id INTEGER,
  quote TEXT,
  created TEXT,
  updated TEXT
);

CREATE INDEX IF NOT EXISTS nodes_thread ON nodes(thread, type);

CREATE TABLE IF NOT EXISTS links(
  a TEXT,
  b TEXT,
  kind TEXT,
  PRIMARY KEY(a,b,kind)
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
