-- server.rs: api_idea
INSERT OR IGNORE INTO sources(ref,kind,title,thread,first_ts,last_ts,mtime,nchunks)
VALUES(?1,'owner','Typed in the app',?2,?3,?3,0,1)
