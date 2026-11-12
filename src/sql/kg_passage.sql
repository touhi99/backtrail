-- kg.rs: passage (an optional AND clause hiding one session is filled in by the caller)
SELECT
  s.ref,
  ifnull(substr(k.ts,1,10),''),
  k.role,
  snippet(fts,0,'[',']','…',24)
FROM fts
JOIN chunks k ON k.id=fts.rowid
JOIN sources s ON s.id=k.source_id
WHERE fts MATCH ?1
  AND s.kind='session'
  AND (?2='' OR k.source_id IN (SELECT e.source_id FROM claim_evidence e JOIN claim_about a ON a.claim_id=e.claim_id WHERE a.entity_id=?2))
  AND (?3='' OR k.ts>=?3){exclude}
ORDER BY bm25(fts)
LIMIT 5
