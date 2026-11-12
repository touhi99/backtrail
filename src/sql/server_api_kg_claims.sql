-- server.rs: api/kg/claims (the WHERE filter is filled in by the caller)
SELECT
  c.id,
  c.type,
  c.text,
  c.body,
  c.speaker,
  c.asserted_at,
  c.status,
  (
    SELECT
      group_concat(o.name, ' | ')
    FROM claim_about x
    JOIN entities o ON o.id=x.entity_id
    WHERE x.claim_id=c.id
  ) AS entities
FROM claims c
WHERE {where}
ORDER BY c.asserted_at DESC
LIMIT 150
