-- kg.rs: suggest_data
SELECT
  c.text,
  c.source_id
FROM chunks c
JOIN sources s ON s.id=c.source_id
WHERE s.kind='session'
  AND c.role='user'
  AND length(c.text)<3000
  AND c.text NOT LIKE '%task-notification%'
  AND c.text NOT LIKE 'This session is being continued%'
  AND c.text NOT LIKE '<bash-%'
