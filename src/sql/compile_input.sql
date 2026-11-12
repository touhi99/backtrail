-- compile.rs: input
SELECT
  type,
  title
FROM nodes
WHERE thread=?1
  AND type!='thread'
  AND state!='rejected'
ORDER BY created DESC
LIMIT 40
