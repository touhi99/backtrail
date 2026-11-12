-- server.rs: graph
SELECT
  'T:'||ea.name AS a,
  'T:'||eb.name AS b,
  substr(c.type, 10) AS kind,
  v.quote,
  v.ts,
  CASE c.status WHEN 'proposed' THEN 'proposed' ELSE NULL END AS state,
  v.chunk_id,
  s.ref AS source,
  s.title AS source_title,
  s.id AS source_id
FROM claims c
JOIN claim_about sa ON sa.claim_id=c.id AND sa.role='subject'
JOIN entities ea ON ea.id=sa.entity_id
JOIN claim_about ob ON ob.claim_id=c.id AND ob.role='object'
JOIN entities eb ON eb.id=ob.entity_id
JOIN claim_evidence v ON v.claim_id=c.id
LEFT JOIN sources s ON s.id=v.source_id
WHERE c.type IN ('relation:spun out of','relation:forked from','relation:built on','relation:replaced by')
  AND c.status!='retracted'
