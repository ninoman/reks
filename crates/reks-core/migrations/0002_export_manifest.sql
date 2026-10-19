-- Phase 2 task 13 (D23): what reks last exported to each stick, one row per
-- file, filled from the validator's read-back. Phase 2.5 matches player
-- history entries through it, keyed by (volume serial, stick path).

CREATE TABLE export_manifest (              -- latest export to each stick
  volume_serial TEXT NOT NULL,
  stick_path    TEXT NOT NULL,              -- '/Contents/...' as in the PDB track row
  item_kind     TEXT NOT NULL DEFAULT 'track',   -- 'track' | 'divider' (Phase 2.5)
  track_id      INTEGER REFERENCES track(id) ON DELETE SET NULL,
  exported_at   TEXT NOT NULL,
  PRIMARY KEY (volume_serial, stick_path)
);

-- ON DELETE SET NULL looks rows up by track_id.
CREATE INDEX export_manifest_track_idx ON export_manifest(track_id);
