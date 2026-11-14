-- server.rs: check
SELECT
  t.name
FROM threads t
WHERE t.state NOT IN ('dropped','unsorted')
  AND NOT EXISTS (SELECT 1 FROM claims n WHERE n.thread=t.name AND n.type IN ('decision','step','question','turn'))
