-- server.rs: check
SELECT
  id,
  thread,
  title
FROM nodes
WHERE type!='thread'
  AND source_id IS NOT NULL
  AND chunk_id IS NULL
