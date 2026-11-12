-- server.rs: graph
SELECT
  s.thread AS th,
  COUNT(DISTINCT s.id) AS n
FROM fts
JOIN chunks ch ON ch.id=fts.rowid
JOIN sources s ON s.id=ch.source_id
WHERE fts MATCH ?1
  AND s.thread IS NOT NULL
  AND s.thread NOT IN ('Unsorted', ?2)
GROUP BY s.thread
