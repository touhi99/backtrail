-- kg.rs: import_decisions
INSERT INTO claims(id,type,text,body,speaker,asserted_at,status,origin,created)
VALUES(?1,'decision',?2,?3,?4,?5,?6,'migrated',?7)
ON CONFLICT(id) DO UPDATE
SET status=?6, text=?2, body=?3, asserted_at=?5, speaker=?4
