-- compile.rs: relate
INSERT INTO links(a,b,kind,source_id,chunk_id,quote,ts,state)
VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
ON CONFLICT(a,b,kind) DO UPDATE
SET source_id=?4, chunk_id=?5, quote=?6, ts=?7, state=?8
