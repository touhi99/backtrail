-- server.rs: trail lineage. "A built on B", "A spun out of B", "A forked from B".
SELECT type, text, substr(asserted_at, 1, 10) AS d
FROM claims
WHERE type IN ('relation:built on', 'relation:spun out of', 'relation:forked from') AND status NOT IN ('rejected', 'retracted')
