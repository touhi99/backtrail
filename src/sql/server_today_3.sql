-- server.rs: today
SELECT
  n.id,
  n.type,
  n.title,
  n.body,
  n.thread,
  n.chunk_id,
  n.source_id,
  n.stale,
  s.last_ts AS source_ts,
  s.title AS source_title,
  s.kind AS source_kind
FROM nodes n
LEFT JOIN sources s ON s.id=n.source_id
WHERE n.origin='suggested'
  AND n.state='proposed'
ORDER BY n.created DESC
