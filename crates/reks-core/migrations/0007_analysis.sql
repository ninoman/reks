-- Phase 3 task 11: background analysis queue (0006 is the grid editor's grid_locked). Spec: docs/phases/phase-3.md
-- ("Analysis cache") and reks-core/src/analysis.rs. The grid itself lives in
-- `beatgrid` (source = 'analysis'); the key in track.key (key_source =
-- 'analysis'). These columns say which analyser run produced them and for
-- which file contents, so a new analyser version or a changed file re-queues
-- the track.

ALTER TABLE track ADD COLUMN analysis_version INTEGER;  -- reks_analysis::ANALYSER_VERSION of the last run
ALTER TABLE track ADD COLUMN analysis_hash    TEXT;     -- content hash of the file that run analysed
ALTER TABLE track ADD COLUMN analysed_at      TEXT;     -- ISO-8601 UTC
ALTER TABLE track ADD COLUMN analysis_error   TEXT;     -- set when that run failed; not retried until version or file change
ALTER TABLE track ADD COLUMN beat_confidence  REAL;     -- 0..1, beat tracker
ALTER TABLE track ADD COLUMN key_confidence   REAL;     -- 0..1, key detector (uncalibrated)
