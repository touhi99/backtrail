-- kg.rs: export
SELECT
  id,
  type,
  text,
  body,
  speaker,
  asserted_at,
  valid_to,
  status,
  superseded_by,
  origin,
  created
FROM claims
