-- server.rs: api_notyet
UPDATE nodes
SET state='open', done_by=NULL, answer=NULL, updated=?2
WHERE id=?1
  AND state='looks_done'
