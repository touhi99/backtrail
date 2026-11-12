-- kg.rs: migrate_links
INSERT INTO claims(id,type,text,speaker,asserted_at,status,origin,created)
VALUES(?1,?2,?3,?4,?5,?6,'migrated',?7)
ON CONFLICT(id) DO UPDATE
SET status=?6
