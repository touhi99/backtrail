-- server.rs: today
SELECT
  t.thread AS name,
  t.area,
  t.state,
  t.kind,
  t.body AS summary,
  (
    SELECT
      MIN(first_ts)
    FROM sources s
    WHERE s.thread=t.thread
      AND s.first_ts>'2000'
  ) AS first,
  (SELECT COUNT(*) FROM sources s WHERE s.thread=t.thread) AS sources,
  (
    SELECT
      COUNT(*)
    FROM nodes q
    WHERE q.thread=t.thread
      AND q.type='question'
      AND q.state='open'
      AND q.origin!='owner'
  ) AS open_q,
  (
    SELECT
      COUNT(*)
    FROM nodes q
    WHERE q.thread=t.thread
      AND q.type='step'
      AND q.state IN ('open','doing')
  ) AS open_s,
  (
    SELECT
      COUNT(*)
    FROM nodes q
    WHERE q.thread=t.thread
      AND q.type='step'
      AND q.state='done'
  ) AS done_s,
  (SELECT MAX(last_ts) FROM sources s WHERE s.thread=t.thread) AS last
FROM nodes t
WHERE t.type='thread'
ORDER BY last DESC
