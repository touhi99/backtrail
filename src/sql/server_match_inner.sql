-- server.rs: match_inner
SELECT
  ch.text,
  ch.ts
FROM chunks ch
JOIN sources s ON s.id=ch.source_id
WHERE s.kind='commit'
  AND s.thread=?1
  AND ch.locator LIKE 'commit %'
