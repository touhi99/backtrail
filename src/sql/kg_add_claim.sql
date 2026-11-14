-- kg.rs: add_claim
INSERT OR IGNORE INTO claims(id,type,thread,title,body,speaker,asserted_at,state,origin,source_id,chunk_id,quote,stated,created,updated)
VALUES(?1,?2,?3,?4,?5,?6,?7,'proposed',?8,?9,?10,?11,?12,?13,?13)
