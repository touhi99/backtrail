-- server.rs: source
SELECT
  s.id,
  s.ref,
  s.kind,
  s.title,
  s.project,
  s.thread,
  s.first_ts,
  s.last_ts,
  s.nchunks
FROM sources s
WHERE id=?1
