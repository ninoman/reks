# ref-rb6-demo: rekordbox 6 ANLZ reference files

`USBANLZ/` is copied unchanged from rekordcrate 0.3.0's test data
(`data/complete_export/demo_tracks/PIONEER/USBANLZ`, MPL-2.0, © Jan Holthuis
and rekordcrate contributors; <https://github.com/Holzhaus/rekordcrate>). It is
rekordbox 6's USB export of its two bundled demo tracks: analysis data only,
no audio.

| Folder | Track (`PPTH`) | Grid | Detail columns |
| --- | --- | --- | --- |
| `P016/0000875E` | `/Contents/Loopmasters/UnknownAlbum/Demo Track 1.mp3` | 368 beats, 128.00 BPM | 25,866 |
| `P053/0001D21F` | `/Contents/Loopmasters/UnknownAlbum/Demo Track 2.mp3` | 257 beats, 120.00 BPM | 19,208 |

Neither track has cues. The audio is not available, so the golden test in
`crates/reks-anlz/tests/golden.rs` builds from the reference grid and duration
(not from our own analysis) and compares structure, PQTZ and cue sections;
waveform bytes are only measured. A reference set with audio (`ref-<ver>/`,
see `docs/phases/phase-3.md` task 9) runs through the ignored test there.

SHA-1: `.DAT` 0366d6d0… / 1e2fdde7…, `.EXT` 186218bf… / d8bb8da5…,
`.2EX` 3cad4e9c… / 7c132f8a… (P016 / P053).
