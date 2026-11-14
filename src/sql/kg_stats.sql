-- kg.rs: stats
SELECT
  COUNT(*)
FROM claims c
WHERE c.quote IS NULL
  AND c.origin!='owner'
