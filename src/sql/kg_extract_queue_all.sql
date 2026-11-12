-- kg.rs: extract_queue_all
SELECT
  s.ref,
  (
    SELECT
      group_concat(text,' ')
    FROM chunks
    WHERE source_id=s.id
      AND role='user'
  )
FROM sources s
WHERE s.kind='session'
  AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=s.ref AND l.action='kg-extract')
