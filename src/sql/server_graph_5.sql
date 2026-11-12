-- server.rs: graph
SELECT
  id,
  type,
  title,
  state,
  thread,
  source_id
FROM nodes
WHERE type!='thread'
  AND state!='rejected'
  AND thread!='Unsorted'
