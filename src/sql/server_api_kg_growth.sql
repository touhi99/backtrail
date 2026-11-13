-- server.rs: api_kg_growth
SELECT
  c.id AS id,
  substr(COALESCE(NULLIF(c.asserted_at, ''), s.first_ts), 1, 10) AS d,
  s.id AS sid,
  s.title AS stitle,
  s.kind AS skind,
  (SELECT group_concat(a.entity_id, ',') FROM claim_about a WHERE a.claim_id=c.id) AS es
FROM claims c
JOIN sources s ON s.id=(
  SELECT e.source_id FROM claim_evidence e WHERE e.claim_id=c.id ORDER BY e.ts, e.chunk_id LIMIT 1
)
WHERE c.status != 'retracted'
ORDER BY d
