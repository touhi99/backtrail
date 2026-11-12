-- server.rs: api_kg_graph
SELECT
  COUNT(*) AS n
FROM claims c
WHERE NOT EXISTS (SELECT 1 FROM claim_about z WHERE z.claim_id=c.id)
