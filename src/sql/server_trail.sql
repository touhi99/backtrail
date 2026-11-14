-- server.rs: trail. Every thread with a dated session, artifact or commit.
SELECT name, state, body AS summary FROM threads WHERE state!='unsorted'
