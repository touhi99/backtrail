-- Read models. Each screen and command filters one of these views instead of repeating the joins.
-- They are TEMP views, created on every connection, so they always match the binary and need no migration.

-- A work item: a decision, step, question or turn, with the source it cites and who last changed it.
CREATE TEMP VIEW IF NOT EXISTS item AS
SELECT
  n.id, n.type, n.thread, n.title, n.body, n.state, n.origin, n.tag, n.done_by, n.answer, n.stale, n.review,
  n.quote, n.created, n.updated, n.superseded_by, n.source_id, n.chunk_id,
  s.ref AS source_ref, s.title AS source_title, s.kind AS source_kind, s.last_ts AS source_ts,
  (SELECT l.actor FROM log l WHERE l.target=n.id ORDER BY l.id DESC LIMIT 1) AS by_actor
FROM claims n
LEFT JOIN sources s ON s.id=n.source_id
WHERE n.type IN ('decision','step','question','turn');

-- A fact about named entities, as the graph shows it: a decision, state or rationale claim, or a relation.
-- `entities` lists the entity names, subject before object (then by entity id), separated by char(31).
CREATE TEMP VIEW IF NOT EXISTS fact AS
SELECT
  c.id, c.type, c.thread, c.text, c.body, c.state, c.status, c.origin, c.speaker, c.asserted_at, c.quote, c.stated,
  c.superseded_by, c.source_id, c.chunk_id,
  s.ref AS source, s.title AS source_title, s.kind AS source_kind, s.first_ts AS source_first_ts,
  (SELECT group_concat(name, char(31)) FROM (
     SELECT e.name FROM claim_about x JOIN entities e ON e.id=x.entity_id
     WHERE x.claim_id=c.id ORDER BY x.role='object', x.entity_id)) AS entities
FROM claims c
LEFT JOIN sources s ON s.id=c.source_id
WHERE c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%';

-- A thread (project) with its counts. `first` and `last` span every source; the `session_` pair spans sessions only.
CREATE TEMP VIEW IF NOT EXISTS thread_stat AS
SELECT
  t.name, t.area, t.state, t.kind, t.body AS summary,
  (SELECT COUNT(*) FROM sources s WHERE s.thread=t.name) AS sources,
  (SELECT MIN(s.first_ts) FROM sources s WHERE s.thread=t.name AND s.first_ts>'2000') AS first,
  (SELECT MAX(s.last_ts) FROM sources s WHERE s.thread=t.name) AS last,
  (SELECT MIN(s.first_ts) FROM sources s WHERE s.thread=t.name AND s.kind='session') AS session_first,
  (SELECT MAX(s.last_ts) FROM sources s WHERE s.thread=t.name AND s.kind='session') AS session_last,
  (SELECT COUNT(*) FROM claims q WHERE q.thread=t.name AND q.type='question' AND q.state='open') AS open_questions,
  (SELECT COUNT(*) FROM claims q WHERE q.thread=t.name AND q.type='question' AND q.state='open' AND q.origin!='owner') AS open_q,
  (SELECT COUNT(*) FROM claims q WHERE q.thread=t.name AND q.type='step' AND q.state IN ('open','doing')) AS open_s,
  (SELECT COUNT(*) FROM claims q WHERE q.thread=t.name AND q.type='step' AND q.state='done') AS done_s
FROM threads t;

-- A source with the number of work items that cite it.
CREATE TEMP VIEW IF NOT EXISTS source_stat AS
SELECT
  s.id, s.ref, s.kind, s.title, s.project, s.thread, s.first_ts, s.last_ts, s.nchunks,
  (SELECT COUNT(*) FROM claims n WHERE n.source_id=s.id AND n.type IN ('decision','step','question','turn')) AS cited
FROM sources s;

-- An entity with its other spellings and the number of claims about it.
CREATE TEMP VIEW IF NOT EXISTS entity_stat AS
SELECT
  e.id, e.name, e.kind,
  (SELECT group_concat(alias, ', ') FROM entity_aliases a WHERE a.entity_id=e.id AND a.alias!=lower(e.name)) AS aliases,
  (SELECT COUNT(*) FROM claim_about x WHERE x.entity_id=e.id) AS claims
FROM entities e;
