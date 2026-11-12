-- server.rs: today
SELECT
  (SELECT COUNT(*) FROM sources WHERE kind='session') AS sessions,
  (SELECT COUNT(*) FROM sources WHERE kind='note') AS notes,
  (SELECT COUNT(*) FROM sources WHERE kind='artifact') AS artifacts,
  (SELECT COUNT(*) FROM chunks) AS chunks,
  (SELECT COUNT(*) FROM nodes WHERE type!='thread') AS items
