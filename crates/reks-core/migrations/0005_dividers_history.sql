-- Phase 2.5 task 1 (D22, D23): playlist dividers and play history.
-- Spec: docs/phases/phase-2.5.md, Data model.

-- Dividers (D22): a playlist entry is a track or a labelled divider. Table
-- rebuild because track_id becomes nullable (SQLite can't alter a column).
CREATE TABLE playlist_track_new (
  playlist_id INTEGER NOT NULL REFERENCES playlist(id) ON DELETE CASCADE,
  track_id    INTEGER REFERENCES track(id) ON DELETE CASCADE,  -- NULL for dividers
  position    INTEGER NOT NULL,
  item_kind   TEXT NOT NULL DEFAULT 'track',                   -- 'track' | 'divider'
  label       TEXT,                                            -- dividers only
  PRIMARY KEY (playlist_id, position),
  CHECK ((item_kind = 'track'   AND track_id IS NOT NULL AND label IS NULL) OR
         (item_kind = 'divider' AND track_id IS NULL     AND label IS NOT NULL))
);
INSERT INTO playlist_track_new (playlist_id, track_id, position)
  SELECT playlist_id, track_id, position FROM playlist_track;
DROP TABLE playlist_track;
ALTER TABLE playlist_track_new RENAME TO playlist_track;
CREATE INDEX playlist_track_track_idx ON playlist_track(track_id);

-- Play history (D23): sessions imported from sticks, one entry per played item.
CREATE TABLE play_session (
  id              INTEGER PRIMARY KEY,
  source          TEXT NOT NULL,          -- 'device_library' (H2/H3 add 'onelibrary', 'engine')
  fingerprint     TEXT NOT NULL UNIQUE,   -- SHA-256, see phase-2.5.md Matching
  volume_serial   TEXT,
  source_name     TEXT NOT NULL,          -- as on the stick, e.g. 'HISTORY 001'
  name            TEXT NOT NULL,          -- user-editable, defaults to source_name
  played_on       TEXT,                   -- YYYY-MM-DD; NULL until HIST1 is decoded
  venue TEXT, notes TEXT,
  excluded        INTEGER NOT NULL DEFAULT 0,
  excluded_reason TEXT,                   -- 'user' | 'pre_baseline'
  imported_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE play_session_entry (
  session_id INTEGER NOT NULL REFERENCES play_session(id) ON DELETE CASCADE,
  position   INTEGER NOT NULL,
  item_kind  TEXT NOT NULL DEFAULT 'track',   -- 'divider' entries never count
  track_id   INTEGER REFERENCES track(id) ON DELETE SET NULL,
  raw_title TEXT, raw_artist TEXT, raw_path TEXT, raw_duration_ms INTEGER,
  PRIMARY KEY (session_id, position)
);
CREATE INDEX play_session_entry_track_idx ON play_session_entry(track_id);

-- Frozen rekordbox baseline. From here on track.play_count / last_played are
-- a cache written only by recompute_play_stats (task 7).
ALTER TABLE track ADD COLUMN baseline_play_count  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE track ADD COLUMN baseline_last_played TEXT;
UPDATE track SET baseline_play_count = play_count, baseline_last_played = last_played;

-- The migration date (UTC) as the baseline date: Phase 1 stores no import
-- date. Settings values are JSON, hence the quotes.
INSERT OR IGNORE INTO setting (key, value)
  VALUES ('history.baseline_date', '"' || date('now') || '"');
