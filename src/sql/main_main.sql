-- main.rs: main
UPDATE claims
SET state='answered', answer=?2, done_by='claude', updated=?3
WHERE id=?1
  AND type='question'
  AND origin='owner'
  AND state='open'
