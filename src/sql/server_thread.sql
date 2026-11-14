-- server.rs: thread
SELECT
  name,
  area,
  state,
  kind,
  body AS summary
FROM threads
WHERE name=?1
