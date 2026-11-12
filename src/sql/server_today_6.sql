-- server.rs: today
SELECT
  s.id,
  s.ref,
  s.title,
  s.kind,
  s.thread,
  s.last_ts
FROM sources s
WHERE s.kind='session'
  AND s.thread IS NOT NULL
ORDER BY s.last_ts DESC
LIMIT 400
