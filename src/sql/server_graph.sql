-- server.rs: graph
SELECT
  t.thread AS name,
  t.state,
  t.area,
  (SELECT COUNT(*) FROM sources s WHERE s.thread=t.thread) AS n,
  (
    SELECT
      COUNT(*)
    FROM nodes q
    WHERE q.thread=t.thread
      AND q.type='question'
      AND q.state='open'
  ) AS q,
  (
    SELECT
      MIN(s.first_ts)
    FROM sources s
    WHERE s.thread=t.thread
      AND s.kind='session'
  ) AS first,
  (
    SELECT
      MAX(s.last_ts)
    FROM sources s
    WHERE s.thread=t.thread
      AND s.kind='session'
  ) AS last
FROM nodes t
WHERE t.type='thread'
  AND t.thread!='Unsorted'
