-- Phase 2 task 15 (D27, D28): stick profiles. A device is a USB stick keyed
-- by the OS's own volume ID; its profile is a live selection of playlists and
-- folders; device_synced_playlist is what the last successful sync wrote.
-- Spec: docs/stick-profiles.md §5.

CREATE TABLE device (
  id             INTEGER PRIMARY KEY,
  volume_serial  TEXT UNIQUE,              -- the OS's volume ID (D28); NULL if it gives none; relinking replaces it
  label          TEXT NOT NULL DEFAULT '', -- volume label when last seen
  size_bytes     INTEGER NOT NULL,         -- volume size when last seen
  name           TEXT NOT NULL,            -- user-given
  target         TEXT NOT NULL,            -- TargetProfile ('Cdj3000', ...)
  created_at     TEXT NOT NULL,
  last_synced_at TEXT
);
-- The "Is this '<name>'?" lookup for an unknown or missing serial.
CREATE INDEX device_label_size_idx ON device(label, size_bytes);

CREATE TABLE device_selection (            -- the profile: playlists and folders (folder = all below it, live)
  device_id   INTEGER NOT NULL REFERENCES device(id) ON DELETE CASCADE,
  playlist_id INTEGER NOT NULL REFERENCES playlist(id) ON DELETE CASCADE,
  PRIMARY KEY (device_id, playlist_id)
);
CREATE INDEX device_selection_playlist_idx ON device_selection(playlist_id);

CREATE TABLE device_synced_playlist (      -- what the last successful sync wrote, one row per stick playlist
  device_id     INTEGER NOT NULL REFERENCES device(id) ON DELETE CASCADE,
  position      INTEGER NOT NULL,          -- order in the stick's tree, depth first
  playlist_id   INTEGER REFERENCES playlist(id) ON DELETE SET NULL,
  path          TEXT NOT NULL,             -- JSON array of folder names + playlist name, as on the stick
  track_count   INTEGER NOT NULL,
  entries       TEXT NOT NULL,             -- JSON array of library track ids in entry order (null = not a library track)
  content_hash  TEXT NOT NULL,             -- blake3 of the ordered entries
  synced_at     TEXT NOT NULL,
  PRIMARY KEY (device_id, position)
);
-- ON DELETE SET NULL looks rows up by playlist_id.
CREATE INDEX device_synced_playlist_playlist_idx ON device_synced_playlist(playlist_id);
