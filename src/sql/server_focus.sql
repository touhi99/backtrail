-- server.rs: focus
SELECT
  thread AS name,
  area,
  state,
  kind,
  body AS summary,
  (SELECT COUNT(*) FROM sources s WHERE s.thread=?1) AS n
FROM nodes
WHERE type='thread'
  AND thread=?1
