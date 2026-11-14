-- ingest.rs: seed_threads
INSERT INTO threads(name,state,area,body,origin,created,updated)
VALUES(?1,?3,?4,?2,'seed',?5,?5)
ON CONFLICT(name) DO UPDATE
SET area=?4
