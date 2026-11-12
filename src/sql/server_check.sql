-- server.rs: check
SELECT
  n.title,
  n.thread,
  n.created,
  n.type
FROM nodes n
JOIN nodes t ON t.type='thread' AND t.thread=n.thread
WHERE n.type IN ('step','question')
  AND n.state='open'
  AND t.state='dropped'
