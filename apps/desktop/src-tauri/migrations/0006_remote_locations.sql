CREATE TABLE IF NOT EXISTS remote_locations (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  provider TEXT NOT NULL CHECK (provider IN ('aws', 'r2', 'custom')),
  endpoint TEXT,
  region TEXT NOT NULL,
  bucket TEXT,
  prefix TEXT,
  access_key_id TEXT NOT NULL,
  path_style INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
