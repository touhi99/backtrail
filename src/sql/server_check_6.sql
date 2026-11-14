-- server.rs: check
SELECT
  COUNT(*) AS n
FROM sources s
WHERE s.kind IN ('note','artifact')
  AND NOT EXISTS (SELECT 1 FROM claims n WHERE n.source_id=s.id AND n.type IN ('decision','step','question','turn'))
