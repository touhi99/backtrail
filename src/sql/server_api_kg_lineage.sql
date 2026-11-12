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
  v.quote,
  v.stated,
  v.source_id,
  s.title AS source_title
FROM claims c
JOIN claim_evidence v ON v.claim_id=c.id AND v.rowid=(SELECT MIN(rowid) FROM claim_evidence WHERE claim_id=c.id)
LEFT JOIN sources s ON s.id=v.source_id
WHERE c.type LIKE 'relation:%'
  AND c.status!='retracted'
ORDER BY c.asserted_at ASC
