-- server.rs: api_idea
INSERT INTO nodes(id,type,thread,title,body,state,area,origin,kind,created,updated)
VALUES(?1,'thread',?2,?2,'Idea typed in the app.','open','Ideas, no repo','owner','idea',?3,?3)
