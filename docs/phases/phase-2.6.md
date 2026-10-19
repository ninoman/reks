# Phase 2.6 — File converter (WAV, AIFF, FLAC)

Status: tasks 1–5 done (2026-10-19/20, started ahead of the Phase 2 hardware
campaign with the maintainer's approval: pure `reks-audio` code). Depends on:
Phase 1 (decode, tags, library) and Phase 2 (`audiocheck`, export, hardware
campaign). Independent of Phase 2.5; either can
go first. Read `docs/decisions.md` D4, D6, D25 and `docs/research.md` §4, §11 first.

## Goal

The user selects tracks and converts them to WAV, AIFF or FLAC with a few
settings. Two jobs:

1. **Play on older players.** FLAC/ALAC, 88.2/96 kHz and float or
   `WAVE_FORMAT_EXTENSIBLE` WAVs don't load on CDJ-2000NXS and older (R§4).
   Converting them to plain 16/24-bit PCM at 44.1/48 kHz does.
2. **Save space.** WAV/AIFF to FLAC is lossless and about half the size.

The converted file takes over the library track (cues, grid, playlists,
history, ratings stay attached), and the original file is never deleted.

## Scope

- Sources: lossless only (FLAC, ALAC, WAV, AIFF). MP3/AAC are refused (D25).
- Targets: WAV (plain PCM `fmt`, never EXTENSIBLE, never float), AIFF (plain
  `AIFF`, never AIFF-C), FLAC.
- Settings, nothing more:

  | Setting | Choices | Default |
  |---|---|---|
  | Format | WAV / AIFF / FLAC | from preset |
  | Bit depth | Keep (max 24) / 16 / 24 | Keep |
  | Sample rate | Keep / 44.1 kHz / 48 kHz | Keep |
  | Destination | Next to the original / chosen folder | Next to the original |
  | Library | Replace in library / Add as new track | Replace |
  | Originals | Keep / Move to the OS Trash | Keep |

  Presets: **Older players** (AIFF, Keep depth, rate capped at 48 kHz:
  88.2 → 44.1, 96 → 48) and **Save space** (FLAC, Keep, Keep). 32-bit sources
  become 24-bit. Going to 16-bit applies TPDF dither. No FLAC level knob (one
  fixed default). Mono stays mono; more than 2 channels is refused.
- Tags and artwork copied with `lofty` (D6): ID3v2 chunk in WAV and AIFF,
  Vorbis comments + PICTURE in FLAC. Players show DB metadata, not tags (R§4).
- Plan → dry run → apply, as for exports: the dry run lists every file, skips
  (lossy, already in target form, >2 channels), size before/after and warnings.
- Phase 2 hook: `audiocheck` issues that conversion fixes (`Codec` for
  FLAC/ALAC, `WavExtensible`, `FloatWav`, `BitDepth`, `SampleRateUnsupported`,
  `AiffC`; see `reks-export-rb/src/audiocheck.rs`) get a
  "Convert…" action that opens the converter on those tracks with the Older
  players preset.
- `reks-cli convert` for headless runs.

## Non-goals

- Lossy targets (MP3/AAC) and lossy sources (D25).
- Converting on export only (FLAC in the library, AIFF on the stick). Useful,
  but it puts a transcoder in the export path; needs its own decision.
- Loudness normalisation, trimming, channel mixing, ALAC output, format
  options beyond the table above.
- Permanently deleting originals (Trash only, and only if asked).

## Interfaces

```rust
// reks-audio::convert — pure audio, no library
pub enum Target { Wav, Aiff, Flac }
pub enum Depth { Keep, Bits16, Bits24 }
pub enum Rate { Keep, Hz44100, Hz48000 }
pub struct ConvertSpec { pub target: Target, pub depth: Depth, pub rate: Rate }
pub struct Output { pub frames: u64, pub sample_rate: u32, pub bit_depth: u8, pub pcm_hash: [u8; 32] }
/// Streams source → (resample) → (dither) → encoder into `tmp`. Never whole-track
/// in memory; reports progress; honours a cancel token.
pub fn convert(src: &Path, tmp: &Path, spec: &ConvertSpec, progress: &dyn Fn(f32),
               cancel: &CancelToken) -> Result<Output>;

// reks-core::convert — plan/apply against the library
pub struct ConvertPlan { pub items: Vec<PlannedConversion>, pub skipped: Vec<(TrackId, SkipReason)>,
                         pub bytes_before: u64, pub bytes_after_est: u64, pub warnings: Vec<Warning> }
pub fn plan(lib: &Library, tracks: &[TrackId], spec: &ConvertSpec, opts: &ConvertOptions) -> Result<ConvertPlan>;
pub fn apply(lib: &Library, plan: &ConvertPlan, events: &dyn Fn(ConvertEvent),
             cancel: &CancelToken) -> Result<ConvertReport>;
```

`apply`, per file: write `<dest>.reks-tmp` in the destination folder, fsync,
decode it back with Symphonia and check frame count (and the PCM hash when no
resample or dither ran), copy tags, rename to the final name (never over an
existing file: add ` (2)`), then update the library in one transaction.
Cancel or failure removes the temp file and leaves the track as it was.

Library update on Replace: `track.path`, `kind`, `file_size`, `file_mtime`,
`content_hash`, `sample_rate`, `bit_depth`, `bit_rate_kbps` change; cues and
grids are in ms, so they stay. The overview waveform cache is dropped.
`rb_location` is kept. A new table records `(old_path, new_track_id, converted_at)`,
so the folder scan doesn't re-add a kept original as a new track and the report
can list originals. Add as new: a new track row with the same metadata, cues
and grid copied.

## Tasks

1. [x] **PCM writers**: own WAV (`fmt` tag 1, 16-byte `fmt` for ≤ 2 ch, 16/24-bit)
   and AIFF (`COMM` + `SSND`, 80-bit rate) little/big-endian writers with
   explicit offsets; round-trip through Symphonia; header byte tests (24-bit
   WAV is tag 1, not 0xFFFE). `hound` stays a dev-dependency (D25).
   Done in [`reks-audio::write`](../../crates/reks-audio/src/write/mod.rs):
   `WavWriter`, `AiffWriter` behind a `PcmWriter` trait; they stream and
   patch sizes in `finish()`, add the RIFF/IFF pad byte, range-check samples
   and refuse over 4 GiB of data (`WriteError::TooLarge`). Exact-byte tests
   per writer; [convert_blocks.rs](../../crates/reks-audio/tests/convert_blocks.rs)
   turns hound's EXTENSIBLE 24/96 WAV into a tag-1 one with the same samples.
2. [x] **FLAC encoder**: `flacenc` behind a small `FlacWriter`; licence check of
   its transitive deps in `cargo deny`; round-trip through Symphonia,
   sample-exact on the fixtures. Done in
   [write/flac.rs](../../crates/reks-audio/src/write/flac.rs): one
   `encode_fixed_size_frame` per 4,096-frame block, written as it comes, then
   the final STREAMINFO (min = max block size, sample count, MD5); flacenc's
   whole-stream API would hold the encoded file in memory. Default features
   off (no threads, no serde). New crates are all MIT or Apache-2.0 (D25).
   Symphonia checks our MD5 on read-back; a flipped MD5 byte fails the read.
3. [x] **Native decode stream** in `reks-audio`: source-depth integer samples in
   chunks (the preview path is stereo `i16`, D5); streaming `rubato` resample;
   TPDF dither to 16-bit; tests: sample-exact passthrough, dither bounded to ±1 LSB.
   Done: [`NativeStream`](../../crates/reks-audio/src/native.rs) yields one
   packet at a time, right-aligned at the source depth (float sources as
   32-bit, clipped at full scale), fails on an undecodable packet instead of
   skipping it (a skip would shift cues) and checks the FLAC MD5;
   [`StreamResampler`](../../crates/reks-audio/src/resample.rs) (rubato FFT,
   delay removed, output trimmed to `ceil(frames·to/from)`; the preview
   decoder now uses it too, at `f32`); [`Requantiser`](../../crates/reks-audio/src/dither.rs)
   widens exactly, rounds to 24 bits and applies seeded TPDF dither at 16 bits
   (deterministic output). Fixtures: `reks-fixtures-gen` renders the 24/96 and
   float WAVs on the fly (fixtures/README.md).
4. [x] **`convert()`**: pipeline, progress, cancel, verify; tags and artwork copy
   with `lofty`; tests per source × target on fixtures (the 24/96 and float
   WAV generators are in `fixtures/gen`, task 3).
   Done in [`reks-audio::convert`](../../crates/reks-audio/src/convert.rs):
   creates `tmp` with `create_new` (never overwrites), streams packet by
   packet, fsyncs, copies tags (primary tag first, others fill gaps; ID3v2
   for WAV/AIFF, Vorbis + PICTURE for FLAC), then decodes the file back and
   checks rate, depth, frame count and [`pcm_hash`](../../crates/reks-audio/src/convert.rs)
   (BLAKE3 of the samples) against what was written, on every path.
   Same rate and depth gives the source's own `pcm_hash`. A drop guard removes
   `tmp` on any error or cancel. `CancelToken` lives in `reks-audio` for now;
   task 5 makes `reks-core` re-export it instead of its own copy.
   Found on the way: Symphonia gives no bit depth for ALAC (now read from the
   magic cookie, in `probe` too), and rubato's delay is half a frame off when
   its FFT output block is odd (96 → 44.1 kHz); `StreamResampler` now picks
   an even block. Tests: [convert.rs](../../crates/reks-audio/tests/convert.rs)
   over WAV, AIFF, FLAC, 24/96 ALAC (new fixture), 24/96 and float WAV × the
   three targets.
5. [x] **Library plan/apply**: migration for the superseded-file table, scan skips
   those paths, Replace / Add as new, Trash option (`trash` crate, licence
   check), collision naming, report. Tests on a temp library: cues, grid,
   playlists and play history survive Replace; a re-scan adds nothing;
   cancel leaves no temp files and no DB change.
   Done in [`reks-core::convert`](../../crates/reks-core/src/convert.rs) and
   migration [`0004_superseded_file.sql`](../../crates/reks-core/migrations/0004_superseded_file.sql).
   `plan` probes headers only and skips lossy, > 2 channels, missing,
   unreadable, duplicate ids and files already in target form (same kind,
   rate and depth, and a plain header: an EXTENSIBLE or float WAV and AIFF-C
   still convert). The Older players rate cap is `ConvertOptions::cap_rate_48k`
   (`Preset::OlderPlayers.options()`); `Rate` has no "cap" value. Warnings:
   renamed, 16-bit dither, and CONV1 for Replace on tracks imported from
   rekordbox. `apply` re-checks that the track and file size are as planned,
   takes the next free name at that moment (a name is taken by a file, its
   `.reks-tmp`, a track row or another item), checks the frame count and, for
   same rate/depth integer sources, the PCM hash against the source, then
   renames without clobbering (hard link, or check + rename where the
   filesystem has none) and fsyncs the folder. Replace keeps `duration_ms`,
   `channels` and `rb_location`; Add as new copies metadata, rating, cues
   and grid but not play history or the rekordbox ids. Trash needs Replace
   (otherwise the kept track would point at a trashed file); on macOS it uses
   NSFileManager (no Finder automation prompt), and a failed move is
   reported, not fatal. Deleting the new track drops its `superseded_file`
   row, so a re-scan adds the original back. `CancelToken` is now the
   `reks_audio` one, re-exported. The Trash move itself has no automated
   test (it would fill the tester's Trash).
6. **UI**: "Convert…" in the track context menu; dialog with the preset and the
   table above; dry-run list; background progress with cancel; report.
   `audiocheck` warnings link to it (Phase 2 dry-run view).
7. **`reks-cli convert`** with the same plan/dry-run/apply flags.
8. **Hardware session**: checklist below; log in `docs/hw-test-log.md`.

## Acceptance criteria

- WAV ↔ AIFF ↔ FLAC at the same rate and depth is sample-exact (hash match).
- A 6-minute 24/96 FLAC converts to 24/48 AIFF in under 5 s on the
  maintainer's machine, with memory bounded (no whole-track buffer).
- Replace keeps every cue, grid row, playlist entry and play-history entry;
  the track plays in the preview player from the new file.
- No conversion ever modifies or deletes the original unless Trash was chosen,
  and an existing file is never overwritten.
- `audiocheck` reports no issue for any file the Older players preset produced.
- The hardware checklist passes and is logged.

## Hardware checklist

1. On a CDJ-2000NXS (or older Device Library player, if available) and a
   CDJ-2000NXS2: export a playlist with AIFF and WAV converted from a 24/96
   FLAC, from an ALAC and from a float WAV, by the Older players preset. All load
   and play.
2. A 24-bit WAV and a WAV with an ID3 chunk load (E-8305 check).
3. A track converted after rekordbox analysis: hot and memory cues land where
   they did on the original (compare on NXS2 by ear and position display).
4. FLAC from Save space plays on NXS2 and CDJ-3000.
5. Record player, firmware, date and results in `docs/hw-test-log.md`.

## Open items

- CONV1 Whether rekordbox's ANLZ for the original file can still be used after
  Replace (PPTH names the old path; waveform data is the same audio at the same
  rate). **UNVERIFIED**; until checked, a replaced track falls back to generated
  analysis on export, and the dry run says so.
- CONV2 Whether ID3 chunks in WAV upset any player (rekordbox and CDJs ignore
  file tags for display, R§4). **UNVERIFIED**.
- CONV3 Players' handling of mono PCM. **UNVERIFIED**; if one fails, the Older
  players preset writes stereo.

## Risks

- **A converted file that won't load on stage.** Mitigation: our own writers
  with fixed header forms, `audiocheck` on every output, hardware checklist.
- **Losing the user's original.** Mitigation: never overwrite, never delete;
  temp file + verify + rename; Trash only on request.
- **Cue drift.** Mitigation: lossless sources only, so the samples line up;
  sample-rate change keeps ms positions; checklist item 3.
- **Duplicate tracks after a re-scan** of a folder that holds both files.
  Mitigation: superseded-file table.
