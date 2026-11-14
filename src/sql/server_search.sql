-- server.rs: search
SELECT
  n.id,
  n.type,
  n.title,
  n.thread,
  n.state,
  IFNULL(n.stale,'') AS stale,
  n.review
FROM temp.nfts f
JOIN claims n ON n.rowid=f.rowid
LEFT JOIN sources s ON s.id=n.source_id
WHERE nfts MATCH ?1
  AND (?2='' OR n.thread=?2)
  AND (?3='' OR s.kind=?3)
  AND (?4='' OR substr(IFNULL(s.last_ts,n.created),1,10)>=?4)
  AND (?5='' OR substr(IFNULL(s.last_ts,n.created),1,10)<=?5)
  AND (?6='' OR n.state=?6)
  AND (n.state!='rejected' OR ?6='rejected')
ORDER BY bm25(nfts, 3.0, 1.0)
LIMIT 20
