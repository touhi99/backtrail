-- server.rs: check
SELECT
  COUNT(*) AS n
FROM sources s
WHERE s.kind IN ('note','artifact')
  AND NOT EXISTS (SELECT 1 FROM nodes n WHERE n.source_id=s.id)
