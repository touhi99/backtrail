-- server.rs: evidence for the listed claims (a quoted id list is filled in by the caller)
SELECT
  v.claim_id,
  v.quote,
  v.stated,
  v.ts,
  v.source_id,
  s.ref AS source,
  s.title AS source_title
FROM claim_evidence v
LEFT JOIN sources s ON s.id=v.source_id
WHERE v.claim_id IN ({ids})
