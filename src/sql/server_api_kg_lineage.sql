-- server.rs: api_kg_lineage
SELECT
  c.id,
  c.type,
  c.text,
  c.speaker,
  c.asserted_at,
  c.status,
  (
    SELECT
      group_concat(name, ' → ')
    FROM (
      SELECT
        e.name
      FROM claim_about x
      JOIN entities e ON e.id=x.entity_id
      WHERE x.claim_id=c.id
      ORDER BY x.role='object'
    )
  ) AS path,
  c.quote,
  c.stated,
  c.source_id,
  s.title AS source_title
FROM claims c
LEFT JOIN sources s ON s.id=c.source_id
WHERE c.type LIKE 'relation:%'
  AND c.status!='retracted'
ORDER BY c.asserted_at ASC
