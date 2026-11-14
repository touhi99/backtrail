-- kg.rs: restore
INSERT OR REPLACE INTO claims(id,type,thread,title,body,state,origin,speaker,asserted_at,valid_to,superseded_by,done_by,answer,tag,stale,source_id,chunk_id,quote,stated,created,updated)
VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)
