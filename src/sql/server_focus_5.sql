-- server.rs: focus
SELECT
  locator AS heading,
  MIN(id) AS chunk_id
FROM chunks
WHERE source_id=?1
  AND locator NOT IN ('#top','§top')
GROUP BY locator
ORDER BY MIN(seq)
LIMIT 9
