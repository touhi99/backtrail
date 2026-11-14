-- compile.rs: retire
UPDATE claims
SET state='rejected', answer=?2, updated=?3
WHERE id=?1
  AND type IN ('decision','step','question','turn')
  AND state!='rejected'
