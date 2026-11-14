-- kg.rs: claim_rows (WHERE and LIMIT are filled in by the caller)
SELECT
  c.text,
  c.type,
  ifnull(c.speaker,''),
  c.asserted_at,
  c.status,
  ifnull(c.quote,''),
  c.stated,
  ifnull((SELECT group_concat(o.name, ', ') FROM claim_about x JOIN entities o ON o.id=x.entity_id WHERE x.claim_id=c.id),'')
FROM claims c
WHERE (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%') AND {where}
ORDER BY c.asserted_at DESC
LIMIT {limit}
