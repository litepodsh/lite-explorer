PRAGMA foreign_keys=OFF;
CREATE TABLE remote_locations_next (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  provider TEXT NOT NULL CHECK (provider IN ('aws', 'r2', 'custom', 'azblob', 'gdrive')),
  endpoint TEXT,
  region TEXT NOT NULL,
  bucket TEXT,
  prefix TEXT,
  access_key_id TEXT NOT NULL,
  path_style INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
INSERT INTO remote_locations_next SELECT * FROM remote_locations;
DROP TABLE remote_locations;
ALTER TABLE remote_locations_next RENAME TO remote_locations;
PRAGMA foreign_keys=ON;
