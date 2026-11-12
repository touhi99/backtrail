-- kg.rs: export
SELECT
  v.claim_id,
  s.ref,
  v.quote,
  v.stated,
  v.ts
FROM claim_evidence v
JOIN sources s ON s.id=v.source_id
