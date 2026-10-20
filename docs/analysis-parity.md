# Analysis parity with rekordbox

Phase 3 task 12 (and the per-codec timing of task 4): how reks's own
analysis (D10 beats/grid, D12 key) compares with rekordbox's analysis of the
same files. Measured once before any tuning; re-run after every analyser
change (`ANALYSER_VERSION`).

Measured 2026-10-20: analyser version 2 (Beat This! small, libKeyFinder
port, task 4 timing), rekordbox 6.8.6 `collection.xml` of the maintainer's
library (3,062 tracks, 2,646 files present: 2,361 FLAC, 133 MP3, 147
WAV/AIFF, 5 MP4), Apple M4 Pro (14 cores), macOS 26.6.

**Against the acceptance criteria** (phase 3), on the 1,762 tracks whose
rekordbox grid has one anchor:

| Criterion | Target | Measured | |
| --- | --- | --- | --- |
| BPM \|Δ\| ≤ 0.05 | ≥ 90% | 85.1% | below |
| Downbeat within 30 ms (mod bar) | ≥ 80% | 70.9% | below |
| Key exact | ≥ 70% | 62.3% | below |
| Key exact or relative | ≥ 85% | 63.8% | below |
| Throughput per core (small model) | ≥ 5× real time | 25.0× | met |

The bar is not lowered: the [tuning plan](#tuning-plan) below is the next
step. The reference is the maintainer's grids as rekordbox holds them, many
corrected by hand, so this measures distance from a checked grid, not just
from rekordbox's own analysis. Grids with several anchors are the
maintainer's edits of tempo changes and re-phased sections; they are
reported but not scored (the maintainer's call, 2026-10-20).

## Reproduce

```sh
cargo build --release -p reks-cli
export REKS_PARITY_XML=~/path/to/collection.xml   # rekordbox: File > Export Collection in xml format
export REKS_PARITY_OUT=~/Library/Caches/reks-parity   # outside the repo: per-track data
./target/release/reks parity run       # resumable; --jobs N, --only mp3,flac, --every N, --remap FROM=TO
./target/release/reks parity report --md report.md --tracks tracks.tsv
```

`run` analyses every track whose file exists with the app's pipeline
(decode → Beat This! small → grid → key → waveform columns,
[`analyse_input`](../crates/reks-analysis/src/pipeline.rs)) on N workers
sharing one rayon pool, like the background queue. Results are appended to
`results.jsonl` (raw tracker beats, key, chroma, decoder timeline, timings),
keyed by `Location`, size, mtime and analyser version, so a re-run only
analyses what changed. `report` prints the aggregates below; `--tracks`
writes per-track rows for inspecting failures locally. Neither file belongs
in the repository: only aggregates of the maintainer's library are published. Code:
[parity.rs](../crates/reks-cli/src/parity.rs).

## Method

- **Reference**: rekordbox's `collection.xml` — `TEMPO` anchors (`Inizio`
  in ms with 3 decimals, `Bpm`, `Battito`) and `Tonality`. Each anchor's
  tempo holds until the next; the first also extends backwards. Several
  anchors with one tempo (re-phased by grid edits) count as a constant grid.
  Grids the maintainer edited are part of the reference as they are.
- **Our grid** is rebuilt from the stored raw beats with
  [`build_grid`](../crates/reks-analysis/src/grid.rs) and default options,
  which is what the pipeline does; `report --min-coverage x` is a what-if
  for the grid builder without decoding again.
- **BPM** agrees when |ours − rekordbox| ≤ 0.05. rekordbox's BPM is the
  anchors' tempo, or `AverageBpm` when anchors differ. Ours is the constant
  tempo, or the median beat gap of a dynamic grid. Misses are classed as
  close (≤ 0.5), double/half, 3/2 or 2/3, other.
- **Downbeat** agrees when our grid's first downbeat lies within 30 ms of a
  rekordbox downbeat (modulo bar: the nearest one in rekordbox's grid).
  Misses are classed by the rekordbox beat our downbeat lands on (bar
  rotated by 1–3 beats) or "off any beat by > 30 ms".
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

Analyser version 2; 3062 tracks in the XML, 2646 analysed (2 failed to decode or analyse), 2644 compared (0 without a rekordbox grid).

Scored: the 1762 tracks whose rekordbox grid has one anchor; the 882 with several anchors (hand-edited) are listed separately and not scored.

| Criterion | Target | Measured | n |
| --- | --- | --- | --- |
| BPM \|Δ\| ≤ 0.05 | ≥ 90% | 85.1% | 1762 |
| Downbeat within 30 ms (mod bar) | ≥ 80% | 70.9% | 1762 |
| Key exact | ≥ 70% | 62.3% | 1760 |
| Key exact or relative | ≥ 85% | 63.8% | 1760 |

| rekordbox grid | Tracks | BPM | Downbeat | Key exact |
| --- | --- | --- | --- | --- |
| one anchor | 1762 | 85.1% | 70.9% | 62.3% |
| several anchors (edited) | 882 | 82.0% | 67.5% | 65.2% |

### BPM

| reks vs rekordbox | Tracks | Share |
| --- | --- | --- |
| match (|Δ| ≤ 0.05) | 1500 | 85.1% |
| close (|Δ| ≤ 0.5) | 163 | 9.3% |
| double/half | 6 | 0.3% |
| other | 91 | 5.2% |
| no grid | 2 | 0.1% |

| reks grid | Tracks | BPM match | Downbeat |
| --- | --- | --- | --- |
| constant | 1261 | 99.2% | 79.5% |
| dynamic (fallback) | 499 | 49.9% | 49.5% |

### Downbeat (first downbeat, modulo bar)

| reks's first downbeat | Tracks | Share |
| --- | --- | --- |
| within 30 ms of a rekordbox downbeat | 1250 | 70.9% |
| on beat 2 of rekordbox's bar | 101 | 5.7% |
| on beat 3 of rekordbox's bar | 121 | 6.9% |
| on beat 4 of rekordbox's bar | 72 | 4.1% |
| more than 30 ms from any rekordbox beat | 216 | 12.3% |
| no grid | 2 | 0.1% |

Among tracks whose BPM matches: 75.6% of 1500 within 30 ms.

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
| (no genre tag) | 1159 | 85.4% | 73.3% | 60.2% | 62.0% |
| Psytrance / Goa | 145 | 83.4% | 55.2% | 66.2% | 66.9% |
| Electro | 109 | 86.2% | 71.6% | 67.9% | 69.7% |
| Electronic | 89 | 78.7% | 60.7% | 64.0% | 64.0% |
| Trance | 82 | 84.1% | 67.1% | 70.7% | 70.7% |
| House | 65 | 98.5% | 81.5% | 63.1% | 64.6% |
| Dance | 41 | 85.4% | 70.7% | 68.3% | 70.7% |
| Other | 34 | 76.5% | 64.7% | 67.6% | 67.6% |
| Techno | 30 | 90.0% | 83.3% | 58.6% | 58.6% |
| Ambient / Downtempo | 8 | 50.0% | 62.5% | 62.5% | 75.0% |

### Timing offset per codec (task 4)

Residual: rekordbox − reks beat time in ms after the decoder's lead (`reks_audio::timing`), per track the median over raw tracker beats within a quarter beat of rekordbox's constant grid, BPM-matched tracks only. Raw = residual + lead: rekordbox − Symphonia's gapless decode. A codec's convention is right when its residual matches the lossless formats' (the tracker's own bias against rekordbox).

| Codec / rule | Tracks | Residual median | Residual 25–75% | Raw median |
| --- | --- | --- | --- | --- |
| ALAC | 2 | -2.4 | -3.5 … -1.2 | -2.4 |
| FLAC | 1973 | +1.8 | +0.2 … +4.1 | +1.8 |
| M4A | 1 | +13.5 | +13.5 … +13.5 | +13.5 |
| MP3: MP3, LAME Info tag: + delay + 1 frame | 54 | +2.0 | +0.8 … +6.1 | +53.1 |
| MP3: MP3, non-LAME Info tag: + delay | 22 | +3.7 | +0.4 … +22.0 | +28.8 |
| MP3: as decoded | 35 | +2.0 | +0.7 … +9.2 | +2.0 |
| WAV/AIFF | 131 | +1.9 | +0.3 … +4.0 | +1.9 |

### Throughput

Per core: 25.0× real time (311.9 h of audio over 12.5 CPU hours, decode, beats, key and waveform columns; process user + system time).
- run 2026-10-20T11:45:28.815Z: 2646 tracks (311.9 h audio) on 13 workers / 14 cores, 116 min wall, 12.4× real time per worker by wall clock

## Tuning plan

The baseline is below the bar on BPM, downbeat and key, so the phase
continues with tuning, in the maintainer's order of priority: BPM, then
downbeat, then key. The bar stays as it is. Each step is re-measured with
`reks parity report` (grid changes, from stored beats) or `run` (anything
that touches audio), and the doc is updated with the new numbers.

1. **Fixed grid first (BPM).** On the scored tracks a constant grid matches
   rekordbox's BPM in 99.2% of cases; the dynamic fallback, taken
   whenever fewer than 80% of tracked beats sit on the best constant grid,
   matches in 49.9%. Nearly all of this library is steady electronic
   music, so the fallback mostly fires on noisy beat lists, not on real
   tempo changes. What-if (`report --min-coverage`, no re-decoding):

   | `min_coverage` | Constant grids | BPM | Downbeat |
   | --- | --- | --- | --- |
   | 0.8 (today) | 1,261 | 85.1% | 70.9% |
   | 0.6 | 1,523 | 90.5% | 72.4% |
   | 0.5 | 1,598 | 92.8% | 72.9% |
   | 0.4 | 1,647 | 93.8% | 72.5% |

   Plan: lower `GridOptions::min_coverage` to about 0.5, and fall back to a
   dynamic grid only on evidence of a real tempo change (a second constant
   region with a different tempo that holds for many bars), so a
   tempo-changing track still gets one.
2. **Kick-led beats and grid fit (BPM, phase).** The beat is almost always
   on the kick. Today each tracked beat is snapped to the sharpest *high*-
   frequency rise within ±30 ms (`snap_to_onset`, task 2), which favours
   hi-hats and percussion. Instead: a low-band (≈ 40–150 Hz) onset envelope;
   snap to the kick when one is near, keep the model's beat otherwise; and
   choose the constant grid's tempo and phase by how well it lines up with
   the kick pulse over the whole track, not only by counting tracked beats.
   Drum loops and busy percussion then stop breaking the constant grid.
   Store the low-band envelope in the results so this tunes without
   re-decoding.
3. **Bar placement (downbeat).** With a matching constant grid, 79.7% of
   first downbeats agree (997 of 1,251); the misses are the bar rotated by
   1–3 beats (19.0%), almost never off the beat (1.3% off by > 30 ms). The anchor is the single
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
  only *when* the fallback fires changes.

