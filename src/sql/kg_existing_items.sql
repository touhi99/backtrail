-- kg.rs: extract_input
SELECT
  type,
  title
FROM claims
WHERE thread=?1
  AND type IN ('decision','step','question','turn')
  AND state!='rejected'
ORDER BY created DESC
LIMIT 40
