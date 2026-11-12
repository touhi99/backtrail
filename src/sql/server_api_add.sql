-- server.rs: api_add
INSERT INTO nodes(id,type,thread,title,body,state,origin,tag,source_id,chunk_id,quote,created,updated)
VALUES(?1,?2,?3,?4,?5,?6,'owner',?7,?8,?9,?4,?10,?10)
