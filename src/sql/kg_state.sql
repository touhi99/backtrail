-- kg.rs: state
SELECT
  t.thread,
  t.state,
  (
    SELECT
      MAX(last_ts)
    FROM sources s
    WHERE s.thread=t.thread
      AND s.kind='session'
  ),
  (
    SELECT
      COUNT(*)
    FROM sources s
    WHERE s.thread=t.thread
      AND s.kind='session'
      AND s.last_ts >= date('now','-3 day')
  )
FROM nodes t
WHERE t.type='thread'
  AND t.thread!='Unsorted'
