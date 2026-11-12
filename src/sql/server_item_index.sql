-- server.rs: item_index
CREATE VIRTUAL TABLE IF NOT EXISTS temp.nfts USING fts5(title, body, tokenize='porter unicode61');

DELETE
FROM temp.nfts;

INSERT INTO temp.nfts(rowid, title, body) SELECT
  rowid, title, IFNULL(body,'')
FROM nodes
WHERE type!='thread';
