-- Saved filters (D35): named rule trees the Filters drawer loads. Ops and
-- reference rules: reks-core/src/filter_preset.rs.

CREATE TABLE filter_preset (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL,
  position   INTEGER NOT NULL,  -- dense 0..n, display order
  rule       TEXT NOT NULL,     -- JSON `RuleNode`
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
-- Unique case-insensitively (ASCII, as tag names).
CREATE UNIQUE INDEX filter_preset_name_idx ON filter_preset(name COLLATE NOCASE);
