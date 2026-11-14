-- server.rs: today
SELECT
  n.id,
  n.title,
  n.thread,
  n.answer,
  n.chunk_id,
  n.source_id,
  (
    SELECT
      l.actor
    FROM log l
    WHERE l.target=n.id
    ORDER BY l.id DESC
    LIMIT 1
  ) AS by_actor
FROM claims n
WHERE n.state='looks_done'
  AND n.type='step'
