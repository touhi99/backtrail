-- kg.rs: neighbors
SELECT
  o.id,
  o.name,
  COUNT(*) AS n
FROM claim_about a
JOIN claim_about b ON a.claim_id=b.claim_id AND b.entity_id!=a.entity_id
JOIN entities o ON o.id=b.entity_id
WHERE a.entity_id=?1
GROUP BY o.id
ORDER BY n DESC
LIMIT 12
