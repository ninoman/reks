# ref-rb6-demo: rekordbox 6 reference export (no audio)

`PIONEER/USBANLZ/`, `PIONEER/rekordbox/export.pdb` and
`PIONEER/rekordbox/exportExt.pdb` are copied unchanged from rekordcrate
0.3.0's test data (`data/complete_export/demo_tracks/PIONEER/`, MPL-2.0,
© Jan Holthuis and rekordcrate contributors;
<https://github.com/Holzhaus/rekordcrate>). It is rekordbox 6's USB export of
its two bundled demo tracks: database and analysis data only, no audio and
no settings files. The folder is a stick root, so `reks rb diff <ours>
fixtures/rekordbox/ref-rb6-demo` compares a stick against it (Phase 2 task 11).

| Folder | Track (`PPTH` = PDB `file_path`) | Grid | Detail columns |
| --- | --- | --- | --- |
| `P016/0000875E` | `/Contents/Loopmasters/UnknownAlbum/Demo Track 1.mp3` | 368 beats, 128.00 BPM | 25,866 |
| `P053/0001D21F` | `/Contents/Loopmasters/UnknownAlbum/Demo Track 2.mp3` | 257 beats, 120.00 BPM | 19,208 |

Neither track has cues; the PDB has no playlists. The audio is not
available, so the golden test in `crates/reks-anlz/tests/golden.rs` builds
from the reference grid and duration (not from our own analysis) and compares
structure, PQTZ and cue sections; waveform bytes are only measured. A
reference set with audio (`ref-<ver>/`, see `docs/phases/phase-3.md` task 9)
runs through the ignored test there.

SHA-1 (ANLZ: P016 / P053): `.DAT` 0366d6d0… / 1e2fdde7…, `.EXT` 186218bf… /
d8bb8da5…, `.2EX` 3cad4e9c… / 7c132f8a…; `export.pdb`
b334ad01cfb31e333de78b45135e2668d74d0a0f, `exportExt.pdb`
069ce341eb4724c427ff90194bd67b73d519fb7f.
