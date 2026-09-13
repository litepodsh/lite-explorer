CREATE TABLE IF NOT EXISTS recents (
  path      TEXT PRIMARY KEY,
  name      TEXT NOT NULL,
  kind      TEXT NOT NULL,
  opened_at INTEGER NOT NULL
);
