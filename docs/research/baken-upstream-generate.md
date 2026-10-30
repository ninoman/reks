# Draft upstream issue: baken-export `generate` path

**Status: DRAFT, not posted.** The maintainer must approve before anything is
posted to [M-Igashi/baken](https://github.com/M-Igashi/baken) (MIT). Source for
this draft: perf audit finding P3-5, checked against baken-export 4.6.1 (the
same code is in v4.8.0, the latest release on 2026-10-30).

## Corrections to the audit's wording

- The generate path is **not single-threaded.** `export` runs two prepare
  workers ahead of the writer (`src/lib.rs:835-852`, `Ahead::new` at
  `:1030-1034`, `.clamp(1, 2)`). It uses `std::thread`, not rayon. rayon is
  only used in `anlz/locate.rs:110` and `lib.rs:364`.
- The two-worker cap was a deliberate choice (#160, #170: 352 generated tracks,
  128 s to 54 s). A later comment there says #178 (streaming measure) removed
  the memory reason for the cap; four workers gained 5-10 s on the SSD image
  (`lib.rs:1015-1020`).
- Our "writer waits, workers idle" observation therefore means: two workers at
  ~183 ms/track are slower than the writer on a fast target, so the writer
  waits. Raising the cap would help only on fast sticks and fast sources.

## Draft issue

**Title:** expressport `--generate-analysis`: reuse unchanged generated analysis on re-export

**Body:**

On a re-export of an unchanged library with `--generate-analysis`, every track
is decoded and measured again. Only the write is skipped: `write_anlz` goes
through `write_if_changed` (`lib.rs:1401-1417`) and reports `anlz_unchanged`,
but only after `prepare_analysis` has already run `generate::measure`
(`lib.rs:1217`, `anlz/generate/waveform.rs:229`), a full decode.

Measured with 4.6.1 on 2,747 generated tracks, nothing changed between runs:
all 2,747 sets were regenerated. One 5-minute MP3 takes ~183 ms in `measure`
(MP3 decode ~59 %). With two workers that is ~3-4 min of CPU per run for output
that comes out byte-identical.

Possible change: skip `measure` when the stick already holds the three ANLZ
files for the track and the source is unchanged. `transcode_on_stick`
(`lib.rs:1190-1200`) and `newer_than_source` (`:1170`) already apply that idea
to `--cdjsafe` transcodes, with a margin for FAT time rounding (#231). The
check for generated sets could be: ANLZ files exist and are newer than the
source by the same margin, and source size matches the plan's `file_size`. It
would need a baken version marker in the file (or a plan-level key) so a change
to the generator invalidates old sets.

Smaller points:
- Whether `.clamp(1, 2)` should now rise, given #178.
- Every run walks the stick's trees: `remove_apple_double` over `Contents`,
  `PIONEER/USBANLZ`, `PIONEER/Artwork` (`lib.rs:963-972`) and `prune_tree`
  with `--prune` (`:952-954`). The code already skips the big walks when nothing
  was written without `--prune` (#196), so this only matters with `--prune`;
  could `prune_tree` and the `._` walk share one traversal?

I can send a PR for the reuse check if you want it; say which shape you prefer
(ANLZ-newer-than-source, or a stored source fingerprint).

## Open questions before posting

- Re-measure on v4.8.0 so the numbers match the latest release.
- The audit did not record `--prune`; say whether the walk cost was with it.
- Existing issues read: #160, #171, #196, #197, #231 (all closed). No open
  issue covers reuse of generated sets.
