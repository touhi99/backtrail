-- server.rs: graph
SELECT
  t.name AS name,
  t.state,
  t.area,
  (SELECT COUNT(*) FROM sources s WHERE s.thread=t.name) AS n,
  (
    SELECT
      COUNT(*)
    FROM claims q
    WHERE q.thread=t.name
      AND q.type='question'
      AND q.state='open'
  ) AS q,
  (
    SELECT
      MIN(s.first_ts)
    FROM sources s
    WHERE s.thread=t.name
      AND s.kind='session'
  ) AS first,
  (
    SELECT
      MAX(s.last_ts)
    FROM sources s
    WHERE s.thread=t.name
      AND s.kind='session'
  ) AS last
FROM threads t
WHERE t.name!='Unsorted'
