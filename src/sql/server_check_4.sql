-- server.rs: check
SELECT
  id,
  thread,
  title
FROM claims
WHERE type IN ('decision','step','question','turn')
  AND source_id IS NOT NULL
  AND chunk_id IS NULL
