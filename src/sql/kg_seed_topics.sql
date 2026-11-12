-- kg.rs: seed_topics
SELECT
  COUNT(DISTINCT k.source_id)
FROM fts
JOIN chunks k ON k.id=fts.rowid
WHERE fts MATCH ?1
  AND k.role='user'
