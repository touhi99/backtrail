-- kg.rs: suggest_data (assistant turns, used to check how a term is capitalised)
SELECT c.text
FROM chunks c
JOIN sources s ON s.id=c.source_id
WHERE s.kind='session'
  AND c.role!='user'
