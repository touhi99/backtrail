-- server.rs: api_tick
UPDATE claims
SET state=CASE state WHEN 'done' THEN 'open' ELSE 'done' END, done_by='owner', updated=?2
WHERE id=?1
  AND type='step'
