-- kg.rs: lint
SELECT
  c.id,
  c.text,
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
WHERE c.status NOT IN ('retracted','superseded')
