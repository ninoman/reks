-- Architecture review F01: an XML import used to keep `analysis` grid rows
-- beside the `rekordbox` rows it wrote, so the two grids interleaved. The
-- rekordbox grid wins (D10); reks's own result stays in the analysis cache.
DELETE FROM beatgrid
WHERE source = 'analysis'
  AND track_id IN (SELECT track_id FROM beatgrid WHERE source = 'rekordbox');
