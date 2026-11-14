-- server.rs: focus
SELECT
  name,
  area,
  state,
  kind,
  body AS summary,
  (SELECT COUNT(*) FROM sources s WHERE s.thread=?1) AS n
FROM threads
WHERE name=?1
