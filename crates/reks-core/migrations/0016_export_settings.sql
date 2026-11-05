-- D42: export options move out of the export wizard into one setting row.
-- Libraries from before keep the experimental options their exports had on
-- (OneLibrary and active loops, D14/D22); a fresh library gets the new
-- defaults (both off) by having no row. Everything else takes the defaults.
INSERT OR IGNORE INTO setting (key, value)
SELECT 'export.settings', '{"onelibrary":true,"activeLoops":true}'
WHERE EXISTS (SELECT 1 FROM track) OR EXISTS (SELECT 1 FROM device);
