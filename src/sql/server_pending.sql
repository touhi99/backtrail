-- server.rs: pending
SELECT
  s.id,
  s.ref,
  s.title,
  s.thread,
  s.last_ts,
  s.nchunks,
  (
    SELECT
      substr(replace(text,char(10),' '),1,140)
    FROM chunks
    WHERE source_id=s.id
      AND role='user'
    ORDER BY seq
    LIMIT 1
  ) AS first
FROM sources s
WHERE s.kind='session'
  AND s.thread!='Unsorted'
  AND s.nchunks>=6
  AND NOT EXISTS (SELECT 1 FROM nodes n WHERE n.source_id=s.id)
ORDER BY s.last_ts DESC
LIMIT 200
