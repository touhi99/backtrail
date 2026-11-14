-- kg.rs: state
SELECT
  t.name,
  t.state,
  (
    SELECT
      MAX(last_ts)
    FROM sources s
    WHERE s.thread=t.name
      AND s.kind='session'
  ),
  (
    SELECT
      COUNT(*)
    FROM sources s
    WHERE s.thread=t.name
      AND s.kind='session'
      AND s.last_ts >= date('now','-3 day')
  )
FROM threads t
WHERE t.name!='Unsorted'
