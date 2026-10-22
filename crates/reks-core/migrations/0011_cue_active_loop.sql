-- Phase 5 task 17: a memory loop (slot NULL, kind 4, end_ms set) that
-- playback enters by itself when the playhead reaches its start (rekordbox's
-- "active loop"). 0 for every other cue. The rekordbox XML has no field for
-- it; see docs/phases/phase-5.md task 17 for how an export carries it.
ALTER TABLE cue ADD COLUMN active_loop INTEGER NOT NULL DEFAULT 0;
