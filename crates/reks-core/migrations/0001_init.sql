-- Migration 0001: initial library schema. Spec: docs/phases/phase-1.md.
-- Connection PRAGMAs (journal_mode, foreign_keys) are set in Library::open,
-- not here: journal_mode cannot change inside the migration transaction.

CREATE TABLE track (
  id            INTEGER PRIMARY KEY,
  path          TEXT NOT NULL UNIQUE,        -- absolute, NFC, as on disk
  file_size     INTEGER NOT NULL,
  file_mtime    TEXT NOT NULL,               -- ISO-8601 UTC
  content_hash  TEXT,                        -- blake3 of first+last 1 MiB + size; for move detection
  missing       INTEGER NOT NULL DEFAULT 0,  -- file not found at last check
  kind          TEXT NOT NULL,               -- mp3|aac|alac|flac|wav|aiff|ogg
  duration_ms   INTEGER,
  sample_rate   INTEGER,
  bit_depth     INTEGER,
  bit_rate_kbps INTEGER,
  channels      INTEGER,
  title         TEXT, artist TEXT, album TEXT, album_artist TEXT, genre TEXT,
  composer TEXT, remixer TEXT, label TEXT, mix TEXT, grouping TEXT,
  track_no INTEGER, disc_no INTEGER, year INTEGER,
  comment       TEXT,
  rating        INTEGER NOT NULL DEFAULT 0,  -- 0..5
  color         TEXT,                        -- rekordbox colour name or NULL
  key           TEXT,                        -- canonical, e.g. "Am", "F#m", "Db"; see keys.rs
  key_source    TEXT,                        -- tag|rekordbox|analysis|manual
  bpm_x100      INTEGER,                     -- average/display BPM * 100
  bpm_source    TEXT,
  play_count    INTEGER NOT NULL DEFAULT 0,
  date_added    TEXT NOT NULL,
  last_played   TEXT,
  artwork_id    INTEGER REFERENCES artwork(id) ON DELETE SET NULL,
  rb_track_id   INTEGER,                     -- TrackID from the imported XML, if any
  rb_location   TEXT,                        -- original Location URL from XML
  created_at    TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX track_artist_idx ON track(artist);
CREATE INDEX track_bpm_idx ON track(bpm_x100);
CREATE INDEX track_key_idx ON track(key);

CREATE TABLE artwork (
  id      INTEGER PRIMARY KEY,
  sha256  TEXT NOT NULL UNIQUE,
  mime    TEXT NOT NULL,
  width INTEGER, height INTEGER,
  blob_path TEXT NOT NULL                    -- file under app_cache_dir/artwork/<sha>.jpg
);

-- Beatgrid: one row per tempo anchor (rekordbox TEMPO element). A constant
-- grid has one row. position_ms is the first beat at/after the anchor.
CREATE TABLE beatgrid (
  id        INTEGER PRIMARY KEY,
  track_id  INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
  position_ms REAL NOT NULL,                 -- float ms (XML Inizio is seconds with 3 decimals)
  bpm_x100  INTEGER NOT NULL,
  meter_num INTEGER NOT NULL DEFAULT 4,
  meter_den INTEGER NOT NULL DEFAULT 4,
  beat_in_bar INTEGER NOT NULL DEFAULT 1,    -- rekordbox Battito 1..4
  source    TEXT NOT NULL                    -- rekordbox|analysis|manual
);
CREATE INDEX beatgrid_track_idx ON beatgrid(track_id, position_ms);

-- Cues: hot cues (slot 0..7 = A..H), memory cues (slot NULL), loops (end_ms set).
CREATE TABLE cue (
  id        INTEGER PRIMARY KEY,
  track_id  INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
  slot      INTEGER,                         -- NULL = memory cue
  kind      INTEGER NOT NULL,                -- 0 cue,1 fade-in,2 fade-out,3 load,4 loop (rekordbox Type)
  start_ms  REAL NOT NULL,
  end_ms    REAL,                            -- loops only
  name      TEXT,
  color_rgb INTEGER,                         -- 0xRRGGBB or NULL
  source    TEXT NOT NULL
);
CREATE INDEX cue_track_idx ON cue(track_id);

CREATE TABLE playlist (
  id        INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES playlist(id) ON DELETE CASCADE, -- NULL = root
  is_folder INTEGER NOT NULL DEFAULT 0,
  name      TEXT NOT NULL,
  sort_order INTEGER NOT NULL,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE playlist_track (
  playlist_id INTEGER NOT NULL REFERENCES playlist(id) ON DELETE CASCADE,
  track_id    INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL,
  PRIMARY KEY (playlist_id, position)
);
CREATE INDEX playlist_track_track_idx ON playlist_track(track_id);

CREATE TABLE library_root (                 -- watched folders
  id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, last_scan TEXT
);

CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- Full-text search over track metadata (external-content FTS5 table).
CREATE VIRTUAL TABLE track_fts USING fts5(
  title, artist, album, genre, comment, label, remixer, content='track', content_rowid='id'
);

-- Standard external-content sync triggers (https://sqlite.org/fts5.html §4.4.3).
CREATE TRIGGER track_fts_ai AFTER INSERT ON track BEGIN
  INSERT INTO track_fts(rowid, title, artist, album, genre, comment, label, remixer)
  VALUES (new.id, new.title, new.artist, new.album, new.genre, new.comment, new.label, new.remixer);
END;
CREATE TRIGGER track_fts_ad AFTER DELETE ON track BEGIN
  INSERT INTO track_fts(track_fts, rowid, title, artist, album, genre, comment, label, remixer)
  VALUES ('delete', old.id, old.title, old.artist, old.album, old.genre, old.comment, old.label, old.remixer);
END;
-- Only re-index when an indexed column changes (play counts, ratings etc. skip it).
CREATE TRIGGER track_fts_au
AFTER UPDATE OF title, artist, album, genre, comment, label, remixer ON track BEGIN
  INSERT INTO track_fts(track_fts, rowid, title, artist, album, genre, comment, label, remixer)
  VALUES ('delete', old.id, old.title, old.artist, old.album, old.genre, old.comment, old.label, old.remixer);
  INSERT INTO track_fts(rowid, title, artist, album, genre, comment, label, remixer)
  VALUES (new.id, new.title, new.artist, new.album, new.genre, new.comment, new.label, new.remixer);
END;
