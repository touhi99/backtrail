-- server.rs: demo
INSERT OR IGNORE INTO sources(ref,kind,title,thread,first_ts,last_ts,mtime)
VALUES(?1,'note',?2,?3,'2026-11-02T12:00:00Z','2026-11-09T12:00:00Z',1)
