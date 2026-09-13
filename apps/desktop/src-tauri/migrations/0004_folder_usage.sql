CREATE TABLE IF NOT EXISTS folder_scans (
  root        TEXT PRIMARY KEY,
  total_bytes INTEGER NOT NULL,
  scanned_at  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS folder_usage (
  root      TEXT NOT NULL,
  path      TEXT NOT NULL,
  name      TEXT NOT NULL,
  bytes     INTEGER NOT NULL,
  is_hidden INTEGER NOT NULL,
  PRIMARY KEY (root, path)
);
