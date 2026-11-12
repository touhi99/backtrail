-- kg.rs: stats
SELECT
  COUNT(*)
FROM claims c
WHERE NOT EXISTS (SELECT 1 FROM claim_evidence e WHERE e.claim_id=c.id)
