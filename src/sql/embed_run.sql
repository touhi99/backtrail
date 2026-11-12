-- embed.rs: run
SELECT
  hash,
  MIN(text)
FROM chunks
WHERE hash IS NOT NULL
  AND hash NOT IN (SELECT hash FROM vec)
GROUP BY hash
LIMIT ?1
