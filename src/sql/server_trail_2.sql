-- server.rs: trail sources. Commit runs carry the repo's first commit as first_ts, so they use last_ts.
SELECT id, thread, kind, title,
  substr(CASE WHEN kind='commit' THEN last_ts ELSE first_ts END, 1, 10) AS d
FROM sources
WHERE kind IN ('session', 'artifact', 'commit') AND thread IS NOT NULL AND first_ts IS NOT NULL
ORDER BY d
