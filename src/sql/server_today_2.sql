-- server.rs: today
SELECT
  n.id,
  n.title,
  n.body,
  n.thread,
  n.created,
  n.origin
FROM nodes n
WHERE n.type='question'
  AND n.state='open'
  AND n.origin='owner'
ORDER BY n.created DESC
