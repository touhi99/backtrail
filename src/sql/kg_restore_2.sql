-- kg.rs: restore
INSERT OR IGNORE INTO claim_evidence(claim_id,source_id,chunk_id,quote,stated,ts)
VALUES(?1,?2,?3,?4,?5,?6)
