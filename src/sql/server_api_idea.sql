-- server.rs: api_idea
INSERT INTO threads(name,state,area,body,origin,kind,created,updated)
VALUES(?1,'open','Ideas, no repo','Idea typed in the app.','owner','idea',?2,?2)
