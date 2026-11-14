-- kg.rs: export
SELECT
  c.id,
  c.type,
  c.thread,
  c.title,
  c.body,
  c.state,
  c.origin,
  c.speaker,
  c.asserted_at,
  c.valid_to,
  c.superseded_by,
  c.done_by,
  c.answer,
  c.tag,
  c.stale,
  s.ref,
  c.quote,
  c.stated,
  c.created,
  c.updated
FROM claims c
LEFT JOIN sources s ON s.id=c.source_id
