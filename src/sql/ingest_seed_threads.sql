-- ingest.rs: seed_threads
INSERT INTO nodes(id,type,thread,title,body,state,area,origin,created,updated)
VALUES(?1,'thread',?2,?2,?3,?4,?5,'seed',?6,?6)
ON CONFLICT(id) DO UPDATE
SET area=?5
