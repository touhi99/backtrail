-- server.rs: brief
SELECT
  title,
  answer
FROM claims
WHERE type='question'
  AND state='answered'
  AND thread=?1
ORDER BY updated DESC
LIMIT 2
