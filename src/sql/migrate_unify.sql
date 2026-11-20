-- Copies items (`nodes`), graph claims (`claims_old`) and quoted thread relations (`links`) into `claims` and `threads`.
INSERT INTO claims(id,type,thread,title,body,state,origin,speaker,asserted_at,valid_to,superseded_by,done_by,answer,tag,stale,source_id,chunk_id,quote,stated,created,updated)
SELECT
  n.id,
  n.type,
  n.thread,
  n.title,
  n.body,
  n.state,
  n.origin,
  coalesce(o.speaker, CASE k.role WHEN 'user' THEN 'owner' WHEN 'assistant' THEN 'assistant' ELSE CASE n.origin WHEN 'owner' THEN 'owner' ELSE 'unknown' END END),
  coalesce(o.asserted_at, k.ts, so.last_ts, n.created),
  o.valid_to,
  coalesce(o.superseded_by, (SELECT l.b FROM links l WHERE l.a=n.id AND l.kind='replaced by' LIMIT 1)),
  n.done_by,
  n.answer,
  n.tag,
  n.stale,
  n.source_id,
  n.chunk_id,
  n.quote,
  coalesce((SELECT e.stated FROM claim_evidence e WHERE e.claim_id=o.id LIMIT 1), 'stated'),
  n.created,
  n.updated
FROM nodes n
LEFT JOIN claims_old o ON o.id='c-'||n.id
LEFT JOIN chunks k ON k.id=n.chunk_id
LEFT JOIN sources so ON so.id=n.source_id
WHERE n.type!='thread'
ORDER BY n.rowid;

INSERT INTO claims(id,type,thread,title,body,state,origin,speaker,asserted_at,valid_to,superseded_by,source_id,chunk_id,quote,stated,created,updated)
SELECT
  c.id,
  c.type,
  so.thread,
  c.text,
  c.body,
  CASE c.status WHEN 'active' THEN 'settled' WHEN 'retracted' THEN 'rejected' ELSE c.status END,
  c.origin,
  c.speaker,
  c.asserted_at,
  c.valid_to,
  c.superseded_by,
  e.source_id,
  e.chunk_id,
  e.quote,
  coalesce(e.stated, 'stated'),
  c.created,
  c.created
FROM claims_old c
LEFT JOIN claim_evidence e ON e.rowid=(SELECT MIN(rowid) FROM claim_evidence WHERE claim_id=c.id)
LEFT JOIN sources so ON so.id=e.source_id
WHERE c.id NOT IN (SELECT 'c-'||id FROM nodes WHERE type!='thread')
ORDER BY c.rowid;

UPDATE OR IGNORE claim_about SET claim_id=substr(claim_id,3) WHERE claim_id IN (SELECT 'c-'||id FROM nodes WHERE type!='thread');

DELETE FROM claim_about WHERE claim_id IN (SELECT 'c-'||id FROM nodes WHERE type!='thread');

UPDATE kg_overrides SET target=substr(target,3) WHERE target IN (SELECT 'c-'||id FROM nodes WHERE type!='thread');

UPDATE claims SET superseded_by=substr(superseded_by,3) WHERE superseded_by LIKE 'c-%' AND substr(superseded_by,3) IN (SELECT id FROM claims);

INSERT INTO threads(name,state,area,kind,body,origin,created,updated)
SELECT thread, state, area, kind, body, origin, created, updated FROM nodes WHERE type='thread' ORDER BY rowid;
