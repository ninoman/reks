-- Grid lock (Phase 3 task 13): a locked grid cannot be edited in the grid
-- editor, and re-analysis never replaces it (nor a grid with source = manual).
ALTER TABLE track ADD COLUMN grid_locked INTEGER NOT NULL DEFAULT 0;
