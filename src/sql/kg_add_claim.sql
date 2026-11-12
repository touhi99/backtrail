-- kg.rs: add_claim
INSERT OR IGNORE INTO claims(id,type,text,body,speaker,asserted_at,status,origin,created)
VALUES(?1,?2,?3,?4,?5,?6,'proposed',?7,?8)
