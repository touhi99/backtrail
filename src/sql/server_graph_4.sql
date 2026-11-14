-- server.rs: graph (an item replaced by another)
SELECT
  id AS a,
  superseded_by AS b
FROM claims
WHERE type IN ('decision','step','question','turn')
  AND superseded_by IS NOT NULL
