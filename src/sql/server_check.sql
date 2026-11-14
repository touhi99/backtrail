-- server.rs: check
SELECT
  n.title,
  n.thread,
  n.created,
  n.type
FROM claims n
JOIN threads t ON t.name=n.thread
WHERE n.type IN ('step','question')
  AND n.state='open'
  AND t.state='dropped'
