-- main.rs: main
UPDATE claims
SET state='looks_done', done_by='claude', answer=?2, updated=?3
WHERE id=?1
  AND type='step'
  AND state IN ('open','doing')
