-- Grid editing with tempo sections (D10, phase 3 task 15): the grid a track
-- had before its first edit, so Reset can bring it back with its source.
-- JSON `{"source": "analysis" | "rekordbox" | null, "anchors": [...]}`;
-- NULL while the grid is not edited (reks-core/src/beatgrid/mod.rs).
ALTER TABLE track ADD COLUMN grid_base TEXT;
