-- db.rs: add_item
INSERT INTO claims(id,type,thread,title,body,state,origin,source_id,chunk_id,quote,created,updated,speaker,asserted_at)
VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12,coalesce(?13,?11))
ON CONFLICT(id) DO UPDATE
SET body=?5, quote=?10, chunk_id=?9, source_id=?8
