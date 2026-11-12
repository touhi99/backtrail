-- server.rs: artifact_manifest
INSERT INTO artifact_meta(id,title,updated)
VALUES(?1,?2,?3)
ON CONFLICT(id) DO UPDATE
SET title=?2, updated=?3
