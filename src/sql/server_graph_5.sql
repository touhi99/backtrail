-- server.rs: graph
SELECT
  id,
  type,
  title,
  state,
  thread,
  source_id
FROM claims
WHERE type IN ('decision','step','question','turn')
  AND state!='rejected'
  AND thread!='Unsorted'
