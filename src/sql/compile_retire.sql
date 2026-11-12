-- compile.rs: retire
UPDATE nodes
SET state='rejected', answer=?2, updated=?3
WHERE id=?1
  AND type!='thread'
  AND state!='rejected'
