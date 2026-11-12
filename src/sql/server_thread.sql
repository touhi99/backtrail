-- server.rs: thread
SELECT
  thread AS name,
  area,
  state,
  kind,
  body AS summary
FROM nodes
WHERE type='thread'
  AND thread=?1
