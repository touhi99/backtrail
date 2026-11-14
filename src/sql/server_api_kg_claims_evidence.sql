-- server.rs: evidence for the listed claims (a quoted id list is filled in by the caller)
SELECT
  c.id AS claim_id,
  c.quote,
  c.stated,
  c.asserted_at AS ts,
  c.source_id,
  s.ref AS source,
  s.title AS source_title
FROM claims c
LEFT JOIN sources s ON s.id=c.source_id
WHERE c.id IN ({ids})
  AND c.quote IS NOT NULL
