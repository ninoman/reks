-- Smart playlists (D29): a playlist whose tracks are a saved rule tree.
-- smart_rule: JSON `RuleNode`; NULL = a static playlist (or a folder).
-- smart_sort: JSON `Sort` for its order (and export order); NULL = date
-- added, newest first.
ALTER TABLE playlist ADD COLUMN smart_rule TEXT;
ALTER TABLE playlist ADD COLUMN smart_sort TEXT;
