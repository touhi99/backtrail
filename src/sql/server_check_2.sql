-- server.rs: check
SELECT
  t.thread
FROM nodes t
WHERE t.type='thread'
  AND t.state NOT IN ('dropped','unsorted')
  AND NOT EXISTS (SELECT 1 FROM nodes n WHERE n.thread=t.thread AND n.type!='thread')
