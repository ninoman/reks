# Fixtures

Test inputs for the reks crates. Everything here is generated or synthetic:
never commit commercial audio or a real user's library (CLAUDE.md "Testing rules").

## `audio/` — sine + click test signal

One signal in five formats. Source of truth: [`gen/src/lib.rs`](gen/src/lib.rs).

| Property | Value |
| --- | --- |
| Length | 30 s = 1,323,000 samples |
| Format | 44,100 Hz, mono, 16-bit |
| Sine | 440 Hz (A), amplitude 0.25 |
| Click | every beat at 128 BPM, 64 beats (16 bars of 4/4); 10 ms 2 kHz burst, amplitude 0.6, linear decay |
| Beat `n` onset | sample `(n * 2_646_000 + 64) / 128` (period 20,671.875 samples) |

| File | Size (bytes) | How it is made |
| --- | --- | --- |
| `sine-click-128.wav` | 2,646,044 | `cargo run -p reks-fixtures-gen` |
| `sine-click-128.mp3` | 240,605 | ffmpeg, libmp3lame 64 kb/s CBR |
| `sine-click-128.flac` | 480,894 | ffmpeg, FLAC level 8 |
| `sine-click-128.aiff` | 2,646,076 | ffmpeg, PCM s16be |
| `sine-click-128.m4a` | 248,357 | ffmpeg, native AAC-LC 64 kb/s |

The WAV is deterministic: the generator avoids the platform `sin()` so the
bytes are identical on every OS. A unit test (`matches_checked_in_wav`) fails
if the checked-in file and the generator output differ, so regenerate it after
changing the generator.

The compressed files were encoded once from the WAV with **ffmpeg 7.1.1**
(macOS, `/usr/local/bin/ffmpeg`) and are checked in as they are. Different
ffmpeg versions may produce different bytes, so do not re-encode them
without updating this table and `rekordbox/collection-small.xml` (`Size`).
Commands, run in `fixtures/audio/`:

```sh
C='-map_metadata -1 -fflags +bitexact -flags:a +bitexact -metadata title="Sine Click 128" -metadata artist="reks fixtures"'
eval ffmpeg -y -i sine-click-128.wav -c:a libmp3lame -b:a 64k $C sine-click-128.mp3
eval ffmpeg -y -i sine-click-128.wav -c:a flac -compression_level 8 $C sine-click-128.flac
eval ffmpeg -y -i sine-click-128.wav -c:a pcm_s16be $C sine-click-128.aiff
eval ffmpeg -y -i sine-click-128.wav -c:a aac -b:a 64k $C sine-click-128.m4a
```

Tags: MP3, FLAC and M4A carry title `Sine Click 128` and artist `reks fixtures`;
AIFF carries the title only (ffmpeg's AIFF muxer drops `artist`); WAV has no tags.
The MP3 decodes to 30.04 s because of the encoder's padding frame.

## `rekordbox/collection-small.xml`

A hand-written rekordbox `collection.xml` (rekordbox XML format, `PRODUCT`
rekordbox 7.0.0) for parser and import tests:

- 3 tracks: the MP3, FLAC and M4A above. `Location` is
  `file://localhost/Users/reks/Music/reks%20fixtures/<file>`, so import tests
  remap the folder prefix to `fixtures/audio/`. `Size` matches the files.
- Each track: one `TEMPO` anchor (`Inizio="0.000"`, 128 BPM, 4/4, `Battito="1"`),
  `Tonality="Am"`, different `Rating`/`Colour`/`Genre`/`PlayCount` values.
- Cues on beat positions: memory cues (`Num="-1"`), hot cues A–B with RGB
  colours and names, and one 4-beat loop (`Type="4"`, 15.000–16.875 s, hot cue C)
  on track 1.
- Playlists: `ROOT` → folder `reks fixtures` → `All fixtures` (tracks 1–3) and
  `Lossless` (track 2).

## `device/diskutil-info-fat32.plist`

Output of `diskutil info -plist /Volumes/REKSTEST` on macOS 26 for a 64 MB
disk image (`hdiutil create -size 64m -fs "MS-DOS FAT32" -volname REKSTEST
-layout MBRSPUD`), used by `reks-export-rb`'s plist parser tests. Its
`VolumeUUID` is `10DE9FD0-1935-3F77-BDA8-01229E4D1714`; the FAT32 boot
sector's volume serial is `4993-1CFB`.
