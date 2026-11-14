-- kg.rs: link_entities
SELECT
  e.id,
  e.name,
  (
    SELECT
      group_concat(alias, char(31))
    FROM entity_aliases a
    WHERE a.entity_id=e.id
  )
FROM entities e
