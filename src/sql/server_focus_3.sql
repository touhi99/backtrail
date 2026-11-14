-- server.rs: focus
SELECT
  s.id,
  s.kind,
  s.title,
  s.last_ts,
  s.nchunks,
  (SELECT COUNT(*) FROM claims n WHERE n.source_id=s.id AND n.type IN ('decision','step','question','turn')) AS cited
FROM sources s
WHERE s.thread=?1
  AND s.kind IN ('note','artifact')
ORDER BY cited DESC, s.last_ts DESC
