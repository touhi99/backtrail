-- kg.rs: extract_queue
SELECT
  s.ref,
  s.id
FROM sources s
WHERE s.kind='session'
  AND (SELECT COUNT(*) FROM chunks WHERE source_id=s.id AND role='user')>=4
  AND ifnull(s.thread,'Unsorted')!='Unsorted'
  AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=s.ref AND l.action IN ('extract','kg-extract'))
  AND (
    SELECT
      COUNT(*)
    FROM log l
    WHERE l.target=s.ref
      AND l.action IN ('extract-failed','kg-extract-failed','compile-failed')
  )<2
  AND NOT EXISTS (
    SELECT
      1
    FROM chunks k
    WHERE k.source_id=s.id
      AND k.seq=0
      AND (k.text LIKE 'BACKTRAIL-%' OR k.text LIKE 'Reply with%' OR k.text LIKE 'Read the file%' OR k.text LIKE 'Write a %')
  )
ORDER BY s.last_ts DESC
