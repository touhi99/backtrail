-- kg.rs: lint
SELECT
  c.id,
  c.title,
  (
    SELECT
      group_concat(entity_id)
    FROM (
      SELECT
        entity_id
      FROM claim_about
      WHERE claim_id=c.id
      ORDER BY entity_id
    )
  )
FROM claims c
WHERE c.state NOT IN ('rejected','superseded')
  AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%')
