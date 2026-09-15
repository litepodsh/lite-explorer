CREATE TABLE IF NOT EXISTS favorites (
  path     TEXT PRIMARY KEY,
  name     TEXT NOT NULL,
  source   TEXT NOT NULL,
  hidden   INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL
);
