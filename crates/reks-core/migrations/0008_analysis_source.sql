-- D10 "rekordbox mapping": which analysis a track's analysis_* columns describe.
-- 'reks' = reks's analyser ran (analysis_version is its version);
-- 'rekordbox' = rekordbox's analysis was copied into the reks cache for the
-- file with analysis_hash (analysis_version stays NULL: an analyser upgrade
-- does not make it stale). NULL = neither yet (or a run before this column).

ALTER TABLE track ADD COLUMN analysis_source TEXT;
UPDATE track SET analysis_source = 'reks' WHERE analysis_version IS NOT NULL;
