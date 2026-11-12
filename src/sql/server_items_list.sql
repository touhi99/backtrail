-- server.rs: items_list
SELECT
  n.id,
  n.type,
  n.title,
  n.body,
  n.state,
  n.thread,
  n.created,
  n.answer,
  n.origin,
  n.stale,
  (
    SELECT
      l.actor
    FROM log l
    WHERE l.target=n.id
    ORDER BY l.id DESC
    LIMIT 1
  ) AS by_actor,
  s.last_ts AS source_ts,
  s.title AS source_title,
  s.id AS source_id,
  n.chunk_id
FROM nodes n
LEFT JOIN sources s ON s.id=n.source_id
JOIN nodes t ON t.type='thread' AND t.thread=n.thread
WHERE n.type!='thread'
  AND n.type LIKE ?1
  AND n.state LIKE ?2
  AND n.thread LIKE ?3
  AND ifnull(t.kind,'') LIKE ?4
ORDER BY n.created DESC
LIMIT 300
