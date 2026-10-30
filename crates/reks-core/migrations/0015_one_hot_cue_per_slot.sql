-- Architecture review F10: an XML import could put a rekordbox hot cue in a
-- slot that already held one set in reks. Keep one cue per slot: the one set
-- in reks (source 'manual'), else the oldest row. Then let SQLite refuse a
-- second one.
DELETE FROM cue
WHERE slot IS NOT NULL
  AND EXISTS (
    SELECT 1 FROM cue o
    WHERE o.track_id = cue.track_id AND o.slot = cue.slot AND o.id <> cue.id
      AND ((o.source = 'manual') > (cue.source = 'manual')
           OR ((o.source = 'manual') = (cue.source = 'manual') AND o.id < cue.id))
  );
CREATE UNIQUE INDEX cue_hot_slot_idx ON cue(track_id, slot) WHERE slot IS NOT NULL;
