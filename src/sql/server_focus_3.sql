-- server.rs: focus
SELECT
  s.id,
  s.kind,
  s.title,
  s.last_ts,
  s.nchunks,
  (SELECT COUNT(*) FROM nodes n WHERE n.source_id=s.id) AS cited
FROM sources s
WHERE s.thread=?1
  AND s.kind IN ('note','artifact')
ORDER BY cited DESC, s.last_ts DESC
