-- server.rs: api_review
-- The owner accepts or rejects a proposed fact, or rejects one the keep rule kept (review='auto').
UPDATE claims
SET state=CASE WHEN ?2=0 THEN (CASE WHEN ?5=1 THEN 'parked' ELSE 'rejected' END) WHEN type IN ('step','question') THEN 'open' ELSE 'settled' END, answer=CASE WHEN ?2=0 THEN ?4 ELSE answer END, review=NULL, updated=?3
WHERE id=?1
  AND (state='proposed' OR (review='auto' AND state='settled' AND ?2=0))
