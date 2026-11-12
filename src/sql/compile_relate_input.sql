-- compile.rs: relate_input
SELECT
  thread,
  ifnull(body,'')
FROM nodes
WHERE type='thread'
  AND thread!='Unsorted'
ORDER BY thread
