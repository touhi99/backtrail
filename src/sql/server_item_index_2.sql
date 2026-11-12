-- server.rs: item_index
SELECT
  ch.id AS chunk_id,
  s.id AS source_id,
  s.ref,
  s.kind,
  s.title,
  s.thread,
  s.last_ts,
  ch.locator,
  ch.text AS body
