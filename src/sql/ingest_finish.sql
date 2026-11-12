-- ingest.rs: finish
UPDATE sources
SET nchunks=(SELECT COUNT(*) FROM chunks WHERE source_id=?1), first_ts=?2, last_ts=?3
WHERE id=?1
