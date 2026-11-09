-- Existing Python ProfileStatistics schema; preserve compatibility.
CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE IF NOT EXISTS baseline (profile TEXT PRIMARY KEY, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS baseline_provenance (profile TEXT PRIMARY KEY, data TEXT NOT NULL, migrated_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS observed_active (profile TEXT, session TEXT, start TEXT, end TEXT, UNIQUE(profile,session,start,end));
CREATE INDEX IF NOT EXISTS observed_active_profile_range ON observed_active(profile,start,end);
CREATE TABLE IF NOT EXISTS events (id INTEGER PRIMARY KEY, profile TEXT, identity TEXT UNIQUE, kind TEXT, time TEXT, session TEXT, stage TEXT, mode TEXT, counted INTEGER, offices TEXT, evidence TEXT);
CREATE INDEX IF NOT EXISTS events_profile_time ON events(profile,time);
CREATE TABLE IF NOT EXISTS active (id INTEGER PRIMARY KEY, profile TEXT, session TEXT, start TEXT, end TEXT, seconds REAL);
CREATE TABLE IF NOT EXISTS sessions (profile TEXT, session TEXT, start TEXT, end TEXT, last_seen TEXT, reason TEXT, context TEXT, PRIMARY KEY(profile,session));
CREATE TABLE IF NOT EXISTS episodes (id INTEGER PRIMARY KEY, profile TEXT, session TEXT, kind TEXT, stage TEXT, mode TEXT, document TEXT, start TEXT, end TEXT, last_seen TEXT, confirmed INTEGER DEFAULT 0);
CREATE INDEX IF NOT EXISTS events_profile_kind_time ON events(profile,kind,time);
CREATE INDEX IF NOT EXISTS active_profile_range ON active(profile,start,end);
CREATE UNIQUE INDEX IF NOT EXISTS active_identity ON active(profile,session,start,end);
CREATE TABLE IF NOT EXISTS activity_days(profile TEXT,day TEXT,PRIMARY KEY(profile,day));
CREATE INDEX IF NOT EXISTS episodes_profile_range ON episodes(profile,start,last_seen);
CREATE INDEX IF NOT EXISTS episodes_open ON episodes(profile,session,kind,end);
