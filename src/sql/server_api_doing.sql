-- server.rs: api_doing
UPDATE claims
SET state=CASE state WHEN 'doing' THEN 'open' ELSE 'doing' END, updated=?2
WHERE id=?1
  AND type='step'
  AND state IN ('open','doing')
