-- server.rs: today
SELECT
  n.id,
  n.title,
  n.type,
  n.thread,
  n.created AS since,
  'open' AS why
FROM nodes n
JOIN nodes t ON t.type='thread' AND t.thread=n.thread
WHERE n.type IN ('step','question')
  AND n.state='open'
  AND t.state NOT IN ('dropped')
ORDER BY n.created ASC
