-- server.rs: api_kg_graph
SELECT
  COUNT(*) AS n
FROM claims c
WHERE (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%')
  AND NOT EXISTS (SELECT 1 FROM claim_about z WHERE z.claim_id=c.id)
