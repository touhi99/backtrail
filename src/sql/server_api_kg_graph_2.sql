-- server.rs: api_kg_graph
SELECT
  x.entity_id AS a,
  y.entity_id AS b,
  COUNT(*) AS w
FROM claim_about x
JOIN claim_about y ON x.claim_id=y.claim_id AND x.entity_id<y.entity_id
GROUP BY 1,2
