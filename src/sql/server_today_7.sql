-- server.rs: today
SELECT
  n.id,
  n.title,
  n.type,
  n.thread,
  n.updated AS since,
  'doing' AS why
FROM claims n
JOIN threads t ON t.name=n.thread
WHERE n.type='step'
  AND n.state='doing'
  AND (
    SELECT
      IFNULL(MAX(s.last_ts),'0')
    FROM sources s
    WHERE s.thread=n.thread
      AND s.kind='session'
  ) < strftime('%Y-%m-%dT%H:%M:%SZ','now','-7 days')
