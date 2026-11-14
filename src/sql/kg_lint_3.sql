-- kg.rs: lint
SELECT
  COUNT(*)
FROM claims c
WHERE c.quote IS NULL
  AND c.origin!='owner'
  AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%')
