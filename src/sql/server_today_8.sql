-- server.rs: today
SELECT
  n.id,
  n.title,
  n.type,
  n.thread,
  n.created AS since,
  'open' AS why
FROM claims n
JOIN threads t ON t.name=n.thread
WHERE n.type IN ('step','question')
  AND n.state='open'
  AND t.state NOT IN ('dropped')
ORDER BY n.created ASC
