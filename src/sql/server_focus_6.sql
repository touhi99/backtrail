-- server.rs: focus
SELECT
  id,
  title,
  last_ts,
  nchunks
FROM sources
WHERE thread=?1
  AND kind='session'
ORDER BY last_ts DESC
LIMIT 6
