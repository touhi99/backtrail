-- server.rs: graph
SELECT
  l.a,
  l.b,
  l.kind,
  l.quote,
  l.ts,
  l.state,
  l.chunk_id,
  s.ref AS source,
  s.title AS source_title,
  s.id AS source_id
FROM links l
LEFT JOIN sources s ON s.id=l.source_id
WHERE ifnull(l.state,'') != 'rejected'
  AND NOT (l.a LIKE 'T:%' AND l.b LIKE 'T:%' AND l.chunk_id IS NOT NULL AND l.quote IS NOT NULL)
