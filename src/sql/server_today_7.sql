-- server.rs: today
SELECT
  n.id,
  n.title,
  n.type,
  n.thread,
  n.updated AS since,
  'doing' AS why
FROM nodes n
JOIN nodes t ON t.type='thread' AND t.thread=n.thread
WHERE n.type='step'
  AND n.state='doing'
  AND (
    SELECT
      IFNULL(MAX(s.last_ts),'0')
    FROM sources s
    WHERE s.thread=n.thread
      AND s.kind='session'
  ) < strftime('%Y-%m-%dT%H:%M:%SZ','now','-7 days')
