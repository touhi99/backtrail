-- server.rs: pairs
SELECT
  id,
  thread,
  type,
  title,
  state
FROM nodes
WHERE type!='thread'
  AND state NOT IN ('rejected','proposed')
