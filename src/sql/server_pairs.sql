-- server.rs: pairs
SELECT
  id,
  thread,
  type,
  title,
  state
FROM claims
WHERE type IN ('decision','step','question','turn')
  AND state NOT IN ('rejected','proposed')
