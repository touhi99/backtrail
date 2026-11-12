-- server.rs: check
SELECT
  thread,
  COUNT(*) AS n,
  group_concat(title, ' | ') AS t
FROM sources
WHERE kind IN ('note','artifact')
  AND thread IS NOT NULL
  AND (lower(title) LIKE '%plan%' OR lower(title) LIKE '%blueprint%' OR lower(title) LIKE '%roadmap%')
GROUP BY thread
HAVING n > 1
