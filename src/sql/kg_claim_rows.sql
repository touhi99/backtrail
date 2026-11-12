-- kg.rs: claim_rows (WHERE and LIMIT are filled in by the caller)
SELECT
  c.text,
  c.type,
  ifnull(c.speaker,''),
  c.asserted_at,
  c.status,
  ifnull((SELECT v.quote FROM claim_evidence v WHERE v.claim_id=c.id ORDER BY v.ts LIMIT 1),''),
  ifnull((SELECT v.stated FROM claim_evidence v WHERE v.claim_id=c.id ORDER BY v.ts LIMIT 1),'stated'),
  ifnull((SELECT group_concat(o.name, ', ') FROM claim_about x JOIN entities o ON o.id=x.entity_id WHERE x.claim_id=c.id),'')
FROM claims c
WHERE {where}
ORDER BY c.asserted_at DESC
LIMIT {limit}
