CREATE TABLE IF NOT EXISTS network_locations (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  protocol TEXT NOT NULL CHECK (protocol IN ('smb', 'nfs', 'webdav', 'sftp', 'ftp')),
  host TEXT NOT NULL,
  port INTEGER,
  path TEXT NOT NULL DEFAULT '',
  username TEXT,
  auth TEXT NOT NULL CHECK (auth IN ('guest', 'password')),
  security TEXT CHECK (security IN ('http', 'https', 'explicit', 'implicit', 'plain')),
  remember_password INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS known_hosts (
  host TEXT NOT NULL,
  port INTEGER NOT NULL,
  protocol TEXT NOT NULL,
  algorithm TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  PRIMARY KEY (host, port, protocol)
);
