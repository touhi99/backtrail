-- server.rs: api_kg
SELECT
  e.id,
  e.name,
  e.kind,
  (
    SELECT
      group_concat(alias, ', ')
    FROM entity_aliases a
    WHERE a.entity_id=e.id
      AND a.alias != lower(e.name)
  ) AS aliases,
  (SELECT COUNT(*) FROM claim_about x WHERE x.entity_id=e.id) AS claims
FROM entities e
ORDER BY claims DESC, e.name
