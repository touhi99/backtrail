-- kg.rs: lint
SELECT
  COUNT(*)
FROM claims c
WHERE NOT EXISTS (SELECT 1 FROM claim_evidence v WHERE v.claim_id=c.id)
