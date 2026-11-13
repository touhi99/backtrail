-- server.rs: trail milestones. Steps done, decisions kept, questions answered.
SELECT thread, type, title, substr(coalesce(updated, created), 1, 10) AS d
FROM nodes
WHERE (type='step' AND state IN ('done', 'looks_done'))
   OR (type='decision' AND state='settled')
   OR (type='question' AND state IN ('answered', 'done'))
