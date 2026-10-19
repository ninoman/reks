-- Phase 2.6 task 5 (D25): files the converter replaced. The original stays on
-- disk (unless moved to the Trash), so the folder scan skips these paths
-- instead of adding them back as new tracks, and the report can list them.
-- Spec: docs/phases/phase-2.6.md "Interfaces".

CREATE TABLE superseded_file (
  old_path      TEXT PRIMARY KEY,          -- library path of the original, as track.path was
  new_track_id  INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
  converted_at  TEXT NOT NULL              -- ISO-8601 UTC
);
CREATE INDEX superseded_file_track_idx ON superseded_file(new_track_id);
