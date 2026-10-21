-- My Tags (D29): two levels, categories (parent_id NULL) with tags below
-- them. Spec: docs/phases/phase-1.md ("My Tags") and reks-core/src/tag.rs,
-- which enforces the two-level rule.

CREATE TABLE tag (
  id         INTEGER PRIMARY KEY,
  parent_id  INTEGER REFERENCES tag(id) ON DELETE CASCADE,  -- NULL = category
  name       TEXT NOT NULL,
  sort_order INTEGER NOT NULL,                              -- dense 0..n among siblings
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
-- Unique per parent, case-insensitively; categories (NULL parent) included.
CREATE UNIQUE INDEX tag_name_idx ON tag(ifnull(parent_id, 0), name COLLATE NOCASE);

CREATE TABLE track_tag (
  track_id INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
  tag_id   INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
  PRIMARY KEY (track_id, tag_id)
) WITHOUT ROWID;
CREATE INDEX track_tag_tag_idx ON track_tag(tag_id);

-- rekordbox's default categories.
INSERT INTO tag (parent_id, name, sort_order, created_at, updated_at) VALUES
  (NULL, 'Genre',      0, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  (NULL, 'Components', 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  (NULL, 'Situation',  2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
