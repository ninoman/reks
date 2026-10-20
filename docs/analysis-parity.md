# Analysis parity with rekordbox

Phase 3 task 12 (and the per-codec timing of task 4): how reks's own
analysis (D10 beats/grid, D12 key) compares with rekordbox's analysis of the
same files. Measured before any tuning (version 2), then after each tuning
step; re-run after every analyser change (`ANALYSER_VERSION`).

Measured 2026-10-20: analyser version 3 (Beat This! small, libKeyFinder
port, task 4 timing, tuning steps 1–2 of task 12a), rekordbox 6.8.6
`collection.xml` of the maintainer's library (3,062 tracks, 2,646 files
present: 2,361 FLAC, 133 MP3, 147 WAV/AIFF, 5 MP4), Apple M4 Pro
(14 cores), macOS 26.6.

**Against the acceptance criteria** (phase 3), on the 1,762 tracks whose
rekordbox grid has one anchor:

| Criterion | Target | Version 2 | Version 3 | |
| --- | --- | --- | --- | --- |
| BPM \|Δ\| ≤ 0.05 | ≥ 90% | 85.1% | 96.7% | met |
| Downbeat within 30 ms (mod bar) | ≥ 80% | 70.9% | 73.3% | below |
| Key exact | ≥ 70% | 62.3% | 62.3% | below |
| Key exact or relative | ≥ 85% | 63.8% | 63.8% | below |
| Throughput per core (small model) | ≥ 5× real time | 25.0× | 24.2× | met |

The bar is not lowered: the [tuning plan](#tuning-plan) continues with bar
placement (downbeat) and key. The reference is the maintainer's grids as
rekordbox holds them, many corrected by hand, so this measures distance from
a checked grid, not just from rekordbox's own analysis. Grids with several
anchors are the maintainer's edits of tempo changes and re-phased sections;
they are reported but not scored (the maintainer's call, 2026-10-20).

## Reproduce

```sh
cargo build --release -p reks-cli
export REKS_PARITY_XML=~/path/to/collection.xml   # rekordbox: File > Export Collection in xml format
export REKS_PARITY_OUT=~/Library/Caches/reks-parity   # outside the repo: per-track data
./target/release/reks parity run       # resumable; --jobs N, --only mp3,flac, --every N, --remap FROM=TO, --retry-failed
./target/release/reks parity report --md report.md --tracks tracks.tsv
```

`run` analyses every track whose file exists with the app's pipeline
(decode → Beat This! small → grid → key → waveform columns,
[`analyse_input`](../crates/reks-analysis/src/pipeline.rs)) on N workers
sharing one rayon pool, like the background queue. Results are appended to
`results.jsonl` (every placement of each tracker beat, key, chroma, decoder
timeline, timings) and each track's low-band onset curve to `kick/` (200
frames/s, one byte each), keyed by `Location`, size, mtime and analyser
version, so a re-run only analyses what changed (`--retry-failed` also
redoes errors). `report` prints the aggregates below; `--tracks`
writes per-track rows for inspecting failures locally. Neither file belongs
in the repository: only aggregates of the maintainer's library are published. Code:
[parity.rs](../crates/reks-cli/src/parity.rs).

## Method

- **Reference**: rekordbox's `collection.xml` — `TEMPO` anchors (`Inizio`
  in ms with 3 decimals, `Bpm`, `Battito`) and `Tonality`. Each anchor's
  tempo holds until the next; the first also extends backwards. Several
  anchors with one tempo (re-phased by grid edits) count as a constant grid.
  Grids the maintainer edited are part of the reference as they are.
- **Our grid** is rebuilt from the stored beats and kick curve with
  [`build_grid_with_pulse`](../crates/reks-analysis/src/grid.rs) and default
  options, which is what the pipeline does. What-ifs without decoding:
  `--placement` (beat placement, `BeatPlacement`), `--min-coverage`,
  `--change-beats`, `--change-ratio`, `--kick-phase` (0 = off),
  `--kick-gain`, and `--analyser N` for an older version's stored beats.
- **BPM** agrees when |ours − rekordbox| ≤ 0.05. rekordbox's BPM is the
  anchors' tempo, or `AverageBpm` when anchors differ. Ours is the constant
  tempo, or the median beat gap of a dynamic grid. Misses are classed as
  close (≤ 0.5), double/half, 3/2 or 2/3, other.
- **Downbeat** agrees when our grid's first downbeat lies within 30 ms of a
  rekordbox downbeat (modulo bar: the nearest one in rekordbox's grid).
  Misses are classed by the rekordbox beat our downbeat lands on (bar
  rotated by 1–3 beats) or "off any beat by > 30 ms". **Grid phase** (not
  a criterion): the first downbeat's distance to any rekordbox beat, for
  BPM-matched tracks; it separates the grid's position from the bar.
- **Key**: rekordbox's `Tonality` parsed by `reks_core::Key`; exact,
  relative (same Camelot number), neighbour (±1 Camelot, same mode),
  parallel (same tonic, other mode), other.
- **Scored set**: tracks whose rekordbox grid has one `TEMPO` anchor.
  Several anchors are the maintainer's hand edits (tempo changes,
  re-phased sections); they get their own row and are not scored.
- **Genre**: free-text `Genre` tags folded into buckets by keyword
  (`genre_bucket` in parity.rs); most of this library has no genre tag.
- **Timing offset** (task 4): for BPM-matched tracks on a constant rekordbox
  grid, the median of (rekordbox beat − our raw beat) over the raw beats
  within a quarter beat of rekordbox's grid. Grouped by codec and, for MP3,
  by the [`timing`](../crates/reks-audio/src/timing.rs) rule. "Raw" adds
  back the lead the decoder inserted: rekordbox against Symphonia's plain
  gapless decode. Lossless files have no codec offset, so their residual is
  the tracker's own placement bias against rekordbox; a codec's rule is
  right when its residual matches theirs.
- **Throughput**: audio duration ÷ process CPU time (user + system) over
  the run, so other load on the machine doesn't distort it; wall-clock
  figures per run are listed too.

## Results

Output of `reks parity report` (aggregates only).

Analyser version 3; 3062 tracks in the XML, 2646 analysed (2 failed to decode or analyse), 2644 compared (0 without a rekordbox grid).

Scored: the 1762 tracks whose rekordbox grid has one anchor; the 882 with several anchors (hand-edited) are listed separately and not scored.

| Criterion | Target | Measured | n |
| --- | --- | --- | --- |
| BPM \|Δ\| ≤ 0.05 | ≥ 90% | 96.7% | 1762 |
| Downbeat within 30 ms (mod bar) | ≥ 80% | 73.3% | 1762 |
| Key exact | ≥ 70% | 62.3% | 1760 |
| Key exact or relative | ≥ 85% | 63.8% | 1760 |

| rekordbox grid | Tracks | BPM | Downbeat | Key exact |
| --- | --- | --- | --- | --- |
| one anchor | 1762 | 96.7% | 73.3% | 62.3% |
| several anchors (edited) | 882 | 93.9% | 70.2% | 65.2% |

### BPM

| reks vs rekordbox | Tracks | Share |
| --- | --- | --- |
| match (|Δ| ≤ 0.05) | 1704 | 96.7% |
| close (|Δ| ≤ 0.5) | 16 | 0.9% |
| double/half | 9 | 0.5% |
| 3/2 or 2/3 | 2 | 0.1% |
| other | 29 | 1.6% |
| no grid | 2 | 0.1% |

| reks grid | Tracks | BPM match | Downbeat |
| --- | --- | --- | --- |
| constant | 1734 | 98.2% | 74.1% |
| dynamic (fallback) | 26 | 3.8% | 23.1% |

### Downbeat (first downbeat, modulo bar)

| reks's first downbeat | Tracks | Share |
| --- | --- | --- |
| within 30 ms of a rekordbox downbeat | 1291 | 73.3% |
| on beat 2 of rekordbox's bar | 116 | 6.6% |
| on beat 3 of rekordbox's bar | 145 | 8.2% |
| on beat 4 of rekordbox's bar | 87 | 4.9% |
| more than 30 ms from any rekordbox beat | 121 | 6.9% |
| no grid | 2 | 0.1% |

Among tracks whose BPM matches: 74.9% of 1704 within 30 ms.

Grid phase, same tracks: our first downbeat within 30 ms of any rekordbox beat 95.0%, within 10 ms 84.2%.

### Key

| reks vs rekordbox | Tracks | Share |
| --- | --- | --- |
| exact | 1096 | 62.3% |
| relative | 27 | 1.5% |
| neighbour (±1 Camelot) | 93 | 5.3% |
| parallel | 270 | 15.3% |
| other | 274 | 15.6% |

### Per genre

| Genre (tag bucket) | Tracks | BPM | Downbeat | Key exact | Key exact/relative |
| --- | --- | --- | --- | --- | --- |
| (no genre tag) | 1159 | 96.5% | 75.9% | 60.2% | 62.0% |
| Psytrance / Goa | 145 | 99.3% | 62.8% | 66.2% | 66.9% |
| Electro | 109 | 92.7% | 67.9% | 67.9% | 69.7% |
| Electronic | 89 | 95.5% | 66.3% | 64.0% | 64.0% |
| Trance | 82 | 98.8% | 69.5% | 70.7% | 70.7% |
| House | 65 | 98.5% | 80.0% | 63.1% | 64.6% |
| Dance | 41 | 100.0% | 75.6% | 68.3% | 70.7% |
| Other | 34 | 94.1% | 58.8% | 67.6% | 67.6% |
| Techno | 30 | 96.7% | 80.0% | 58.6% | 58.6% |
| Ambient / Downtempo | 8 | 100.0% | 37.5% | 62.5% | 75.0% |

### Timing offset per codec (task 4)

Residual: rekordbox − reks beat time in ms after the decoder's lead (`reks_audio::timing`), per track the median over raw tracker beats within a quarter beat of rekordbox's constant grid, BPM-matched tracks only. Raw = residual + lead: rekordbox − Symphonia's gapless decode. A codec's convention is right when its residual matches the lossless formats' (the tracker's own bias against rekordbox).

| Codec / rule | Tracks | Residual median | Residual 25–75% | Raw median |
| --- | --- | --- | --- | --- |
| ALAC | 4 | -2.4 | -6.8 … +1.6 | -2.4 |
| FLAC | 2252 | +1.8 | +0.2 … +4.8 | +1.8 |
| M4A | 1 | +13.5 | +13.5 … +13.5 | +13.5 |
| MP3: MP3, LAME Info tag: + delay + 1 frame | 62 | +1.9 | +0.6 … +6.2 | +53.1 |
| MP3: MP3, non-LAME Info tag: + delay | 25 | +4.6 | +0.4 … +11.1 | +27.9 |
| MP3: as decoded | 43 | +2.5 | +1.0 … +14.3 | +2.5 |
| WAV/AIFF | 140 | +1.9 | +0.4 … +4.1 | +1.9 |

### Throughput

Per core: 24.2× real time (312.5 h of audio over 12.9 CPU hours, decode, beats, key and waveform columns; process user + system time).
- run 2026-10-20T13:59:35.935Z: 2646 tracks (311.9 h audio) on 13 workers / 14 cores, 136 min wall, 10.6× real time per worker by wall clock
- run 2026-10-20T19:34:13.537Z: 7 tracks (0.6 h audio) on 13 workers / 14 cores, 0 min wall, 13.5× real time per worker by wall clock

## Tuning plan

The baseline is below the bar on BPM, downbeat and key, so the phase
continues with tuning, in the maintainer's order of priority: BPM, then
downbeat, then key. The bar stays as it is. Each step is re-measured with
`reks parity report` (grid changes, from stored beats) or `run` (anything
that touches audio), and the doc is updated with the new numbers.

1. **Fixed grid first (BPM). Done (version 3).** In version 2 a constant
   grid matched rekordbox's BPM in 99.2% of cases and the dynamic fallback
   (taken whenever fewer than 80% of tracked beats sat on the best constant
   grid) in 49.9%; it fired on noisy beat lists, not tempo changes. Lowering
   `min_coverage` alone reached 92.8% (0.5). Two findings went further:
   - Mixxx's tempo comes from one steady stretch. On noisy tracks the
     tracked beats that do sit on rekordbox's grid fit rekordbox's tempo to
     ±0.005 BPM, yet Mixxx's estimate was 0.05–0.5 BPM off. `fit_tempo`
     searches 0.01 BPM steps within ±3% of Mixxx's estimate and the median
     beat gap, keeps the tempo whose best phase puts the most beats within
     15 ms of a line (roundest on ties); the phase still comes from the
     steady region (beats cluster off the beat on noisy tracks).
   - `min_coverage` 0.3 (at the tempo's best phase): below it there is no
     steady beat. A dynamic grid also needs a tempo change
     (`tempo_change`: a constant region of ≥ 32 beats ≥ 0.5% off the
     grid's tempo, not 2:1 or 3:2). Only 4 tracks of this library have
     rekordbox grids that change tempo, all by < 1 BPM, so the rule is
     tested on synthetic beats.

   On the version 2 beats: BPM 85.1% → 96.7%, downbeat 70.9% → 72.4%;
   26 scored tracks keep a dynamic grid (3.8% BPM match: no steady beat).
2. **Kick-led beats and grid fit (BPM, phase). Done (version 3).** A
   low-band (40–150 Hz) onset curve per track (`onset.rs`) gives each beat
   a kick candidate and its attack (`BeatCandidates`). Measured on the
   scored tracks (grid phase = first downbeat within 30 / 10 ms of a
   rekordbox beat, BPM-matched):

   | Beats / grid phase | BPM | Downbeat | Phase 30 ms | Phase 10 ms |
   | --- | --- | --- | --- | --- |
   | broadband onset (step 1) | 96.7% | 72.4% | 93.3% | 82.5% |
   | kick attack, else onset | 96.9% | 70.1% | 92.5% | 81.0% |
   | broadband onset, kick chooses the phase | 96.7% | 73.3% | 95.0% | 84.2% |

   Placing beats on the kick puts more single beats near rekordbox's lines
   (76% vs 74% within 10 ms, 497-track sample) but fewer grids in phase, so beats stay on the
   broadband onset. The kick instead chooses the grid's phase: the curve's
   leading edge peaks on rekordbox's line (its level stays high 20–30 ms
   after the attack, which made it useless for positioning), and
   `kick_phase` moves the grid to another cluster of beats within 60 ms
   when that cluster's lines sit on 1.5× the kick attack. The phase always
   comes from the beats. Moving the grid to the pulse's peak instead lost
   precision (81.6% → 77.7% within 10 ms on 1,833 tracks). Kick-scored
   *tempo* was not needed: BPM is above the bar, and the remaining misses
   (1.6% "other", 0.5% double/half) are tracks where the tracker followed
   another pulse.
3. **Bar placement (downbeat).** Next. With a matching constant grid
   (version 3), 74.9% of first downbeats agree (1,275 of 1,703); the misses
   are the bar rotated by 1–3 beats (20.1%) and 5.0% off the beat by
   > 30 ms. The anchor is the single
   tracker downbeat nearest the start of the constant region, usually in the
   intro. A majority vote over all tracker downbeats did **not** help
   (292 vs 295 of 349 BPM-matched constant grids in a first sample): the
   model's bar phase itself differs from the one rekordbox and the
   maintainer use. Candidates, in order: anchor beat 1 on phrase structure
   (kick re-entry after a breakdown, 16/32-beat phrase boundaries from the
   low-band envelope); the full Beat This! model (better downbeats, larger
   download); otherwise leave it to the grid editor's set-downbeat.
4. **Key.** First check the port: run libKeyFinder (C++) on a sample of the
   library and compare with our output (task 5's parity check is still
   UNVERIFIED); a port bug would make every number here moot. Then the
   errors in order of size: same tonic, wrong mode (15.3% of scored
   tracks); a fourth apart, and a semitone apart in minor (Phrygian-sounding
   psytrance). Swapping in other standard profiles (Krumhansl, Temperley,
   a minor bias) on the stored chroma did not get near 70% (best ≈ 57% exact
   in a first sample of 552 tracks), so the work is in the mode decision and
   in segment-wise rather than whole-track chroma.

Hardware is unaffected by this measurement; the phase's CDJ checklist still
applies to whatever grid the tuning ends with.

## Open questions

- 5 of 22 ffmpeg-tagged MP3s sit one frame (≈ 26 ms) off the rule; no header
  field (encoder version, ID3 size, bit reservoir) separates them from the
  rest. Untagged MP3s show similar tails, so they may be grid-placement
  noise; revisit with more ffmpeg-tagged files.
- AAC: one track. Re-measure when the library has more.
- Dynamic grids and per-beat tempo in PQTZ (D10) are untouched by step 1;
  only *when* the fallback fires changes. The tempo-change rule has not met
  a large real tempo change yet (this library's 4 change by < 1 BPM).
- The version 3 run shared the machine with other work, so its wall-clock
  figure is low; the per-core figure is CPU time and unaffected.

