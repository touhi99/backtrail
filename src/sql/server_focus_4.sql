-- server.rs: focus
SELECT
  ch.source_id AS sid,
  ch.text
FROM chunks ch
JOIN sources s ON s.id=ch.source_id
WHERE s.kind='note'
  AND ch.text LIKE '%[[%'
