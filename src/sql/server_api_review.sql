-- server.rs: api_review
UPDATE nodes
SET state=CASE WHEN ?2=0 THEN (CASE WHEN ?5=1 THEN 'parked' ELSE 'rejected' END) WHEN type IN ('decision','turn') THEN 'settled' ELSE 'open' END, answer=CASE WHEN ?2=0 THEN ?4 ELSE answer END, updated=?3
WHERE id=?1
  AND state='proposed'
