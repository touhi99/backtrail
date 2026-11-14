-- server.rs: thread story. Status facts, reasons and decisions about this thread, taken only from its own sources.
SELECT
  c.id,
  c.type,
  c.status,
  c.speaker,
  substr(c.asserted_at, 1, 10) AS d,
  c.text,
  (SELECT s.text FROM claims s WHERE s.id=c.superseded_by) AS newer,
  c.quote AS q
FROM claims c
WHERE c.type IN ('state', 'rationale', 'decision')
  AND c.status NOT IN ('rejected', 'retracted')
  AND EXISTS (SELECT 1 FROM claim_about a JOIN entities e ON e.id=a.entity_id WHERE a.claim_id=c.id AND e.name=?1)
  AND EXISTS (SELECT 1 FROM sources s WHERE s.id=c.source_id AND s.thread=?1)
ORDER BY c.asserted_at
