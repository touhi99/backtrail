-- kg.rs: import_decisions
SELECT
  n.id,
  n.title,
  ifnull(n.body,''),
  ifnull(n.thread,''),
  n.state,
  n.stale,
  n.source_id,
  n.chunk_id,
  ifnull(n.quote,''),
  k.role,
  ifnull(k.ts, so.last_ts)
FROM nodes n
JOIN chunks k ON k.id=n.chunk_id
JOIN sources so ON so.id=n.source_id
WHERE n.type='decision'
  AND n.source_id IS NOT NULL
  AND n.quote IS NOT NULL
