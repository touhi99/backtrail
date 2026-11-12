-- server.rs: thread
SELECT
  id,
  ref,
  kind,
  title,
  first_ts,
  last_ts,
  nchunks
FROM sources
WHERE thread=?1
ORDER BY last_ts DESC
