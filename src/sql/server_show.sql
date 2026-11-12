-- server.rs: show
SELECT
  ch.locator,
  ch.role,
  substr(ch.text,1,700) AS t
FROM chunks ch
JOIN sources s ON s.id=ch.source_id
WHERE s.ref=?1
ORDER BY ch.seq
