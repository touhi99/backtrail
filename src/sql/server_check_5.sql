-- server.rs: check
SELECT
  n.title,
  n.thread,
  n.created
FROM claims n
WHERE n.state='open'
  AND n.type='question'
  AND n.origin!='owner'
