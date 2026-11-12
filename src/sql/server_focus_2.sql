-- server.rs: focus
SELECT
  n.id,
  n.type,
  n.title,
  n.body,
  n.state,
  n.tag,
  n.origin,
  n.created,
  n.answer,
  n.source_id,
  n.chunk_id,
  n.stale,
  (SELECT last_ts FROM sources WHERE id=n.source_id) AS source_ts
FROM nodes n
WHERE n.thread=?1
  AND n.type!='thread'
  AND n.state!='rejected'
ORDER BY n.created DESC
