-- server.rs: thread
SELECT
  n.id,
  n.type,
  n.title,
  n.body,
  n.state,
  n.origin,
  n.tag,
  n.done_by,
  (
    SELECT
      l.actor
    FROM log l
    WHERE l.target=n.id
    ORDER BY l.id DESC
    LIMIT 1
  ) AS by_actor,
  n.answer,
  n.created,
  n.quote,
  n.chunk_id,
  n.source_id,
  n.stale,
  s.last_ts AS source_ts,
  s.ref AS source_ref,
  s.title AS source_title,
  s.kind AS source_kind
FROM nodes n
LEFT JOIN sources s ON s.id=n.source_id
WHERE n.thread=?1
  AND n.type!='thread'
ORDER BY n.created
