PRAGMA foreign_keys=OFF;
CREATE TABLE network_locations_next (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  protocol TEXT NOT NULL CHECK (protocol IN ('smb', 'nfs', 'webdav', 'sftp', 'ftp')),
  host TEXT NOT NULL,
  port INTEGER,
  path TEXT NOT NULL DEFAULT '',
  username TEXT,
  auth TEXT NOT NULL CHECK (auth IN ('guest', 'password', 'key')),
  security TEXT CHECK (security IN ('http', 'https', 'explicit', 'implicit', 'plain')),
  remember_password INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
INSERT INTO network_locations_next SELECT * FROM network_locations;
DROP TABLE network_locations;
ALTER TABLE network_locations_next RENAME TO network_locations;
PRAGMA foreign_keys=ON;
