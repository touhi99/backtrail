-- server.rs: trail. Every thread with a dated session, artifact or commit.
SELECT thread AS name, state, body AS summary FROM nodes WHERE type='thread' AND state!='unsorted'
