-- kg.rs: restore
INSERT OR REPLACE INTO claims(id,type,text,body,speaker,asserted_at,valid_to,status,superseded_by,origin,created)
VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
