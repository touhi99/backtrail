-- compile.rs: queue
SELECT
  s.ref
FROM sources s
WHERE s.kind='session'
  AND ifnull(s.thread,'Unsorted')!='Unsorted'
  AND (SELECT COUNT(*) FROM chunks WHERE source_id=s.id AND role='user')>=?1
  AND NOT EXISTS (SELECT 1 FROM nodes n WHERE n.source_id=s.id)
  AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=s.ref AND l.action='compile')
  AND (
    SELECT
      COUNT(*)
    FROM log l
    WHERE l.target=s.ref
      AND l.action='compile-failed'
  )<2
  AND NOT EXISTS (
    SELECT
      1
    FROM chunks k
    WHERE k.source_id=s.id
      AND k.seq=0
      AND (k.text LIKE 'BACKTRAIL-COMPILE%')
  )
ORDER BY s.last_ts DESC
