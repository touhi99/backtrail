-- kg.rs: lint
SELECT
  c.id,
  c.status,
  n.state
FROM claims c
JOIN nodes n ON 'c-'||n.id=c.id
WHERE (n.state='rejected' AND c.status!='retracted')
  OR (n.state='settled' AND c.status IN ('proposed','retracted'))
  OR (n.state='proposed' AND c.status='active')
