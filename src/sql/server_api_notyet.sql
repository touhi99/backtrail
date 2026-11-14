-- server.rs: api_notyet
UPDATE claims
SET state='open', done_by=NULL, answer=NULL, updated=?2
WHERE id=?1
  AND state='looks_done'
