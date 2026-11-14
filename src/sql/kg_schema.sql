-- kg.rs: ensure
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

CREATE TABLE IF NOT EXISTS kg_overrides(
  target TEXT NOT NULL,
  field TEXT NOT NULL,
  value TEXT,
  by TEXT,
  ts TEXT,
  PRIMARY KEY(target, field)
);
