# rekordbox pain points and reks gap analysis

Research date: 2026-10-24. Raw evidence (about 280 rows, every URL):
[`raw-7-rekordbox-pain-points.md`](raw-7-rekordbox-pain-points.md). Source IDs
below (V1, P3, …) resolve in [Sources](#sources).

**Coverage limits.** Reddit refused every tool we have (search, fetch, the
in-app browser), so Reddit signal is only second-hand. The Pioneer DJ forum
returned 403, so forum items are undated search snippets. Digital DJ Tips
comments did not render. Lexicon, Mixed In Key, Digital DJ Pool and DJ.Studio
pages are vendor pitches. Signal strengths here therefore lean on rekordbox's
own release notes and notices, the DJ press and non-Pioneer forums; repeat the
Reddit sweep from a browser that can reach it before a ranking decision rests
on signal alone.

## Summary

The loudest complaint is not a missing feature but trust: within five months
(2025-10 to 2026-03) a CDJ-3000 firmware blanked USB playlists at gigs and
was pulled (V3, P2, P3), rekordbox 7.2.12 shipped a USB export that players
could not load and was pulled (V2, P1), and the Device Library / OneLibrary
split left sticks that one player reads and another doesn't (V4, P5, C8).
Next come the subscription model (paywalls, hardware no longer unlocking v7,
a five-week billing outage; P10–P12, V7, R1) and library hygiene
(duplicates, missing files, database loss), which people pay Lexicon, RCT and
Mixed In Key to fix (W1, W7, W8). reks already answers the trust problem
better than rekordbox on paper (dry run, read-back validation, history and
stick backups, free) but can't claim it until the hardware logs exist. The
cheapest high-signal gaps are pre-gig stick checks, player targets per stick,
automatic library backups and a relocate tool; duplicates, bulk edit and
dynamic-grid editing follow.

## Method

Four sweeps (vendor notes and forum, DJ press and reviews, workaround tools
and migrations, Reddit) collected paraphrased items with URL, date and
signal. **Confirmed** = at least 3 independent sources on at least 2
platforms. Platforms: vendor notices, vendor forum, DJ press, independent
forums (DJ TechTools, Algoriddim, Engine DJ, Gearspace, Dropbox), blogs,
review sites and app stores, workaround-tool vendors and GitHub. Everything
else is **anecdotal**. 2024–2026 items weigh more; older threads only
support a theme that recent items also raise.

## Themes

Status keys: Covered, Planned, Partial, Gap, Out of scope (see the task brief).
"UNVERIFIED" = no logged player test (`docs/hw-test-log.md` is empty).

| # | Theme | Triangulation | Signal | reks status | Evidence |
|---|---|---|---|---|---|
| T1 | USB export breaks after an update; two stick formats (Device Library vs OneLibrary) read by different players | Confirmed | Very high: 3 vendor notices 2025-11 to 2026-03, export fixes in 6.8.0/7.0.1/7.0.4/7.2.4/7.2.13, 5 press pieces | **Partial.** Legacy export with plan → dry run → apply → read-back validation (phase 2 tasks 4, 8, 9; D7, D8); both formats when the OneLibrary flag is on (phase 4, D14). Missing: choosing target players per stick, so the planner knows which formats it must write. UNVERIFIED on players (phase 2 task 14, phase 4 task 9) | V1–V4, V11, P1–P6, P21, C8, F1 |
| T2 | Files a player rejects ("unsupported file", E-8305) with no warning | Confirmed | Medium: tools sold for it (GBP 149.99 GigSafe), Lexicon checker 2026-09 | **Partial.** `audiocheck` blocks or warns per target in the dry run (phase 2 task 3); lossless converter (phase 2.6, D25). Missing: a check of a stick reks did not write; convert-on-export (rejected without a decision, D25) | C20, C21, V5, W2, W8, W9 |
| T3 | Library database loss; cues and grids live only in the DB | Confirmed (mostly pre-2024 threads, still advised in 2024–25 press) | Medium | **Gap** for the library: no automatic snapshots of `reks.sqlite`. Covered for sticks: every write backs up the stick DBs first, free (D23); rekordbox's Device Library Backup needs the Professional plan (V6). Open SQLite + XML export lower the stakes (D3, D11) | F2, F3, F16, C3, C4, P13, P16, V6 |
| T4 | Duplicates after imports, moves and cloud sync | Confirmed | High: rekordbox added view (7.0.4), merge (7.2.8), status (7.2.14); 3 paid/free tools | **Gap.** Scan follows moved files by content hash (phase 1 task 5) and "add tracks" asks about duplicates, but there is no duplicate finder or merge | V1, F4, P14, W1, W8, W10, W13 |
| T5 | Missing files ("!" tracks) after moving folders; Relocate picks the wrong match | Confirmed | High: official FAQ entry, 4 tools | **Partial.** Moves inside library roots are followed by content hash, missing rows keep cues and playlists, "missing file" filter (phase 1 task 5); XML import maps a moved music folder (phase 1 task 12). Missing: relocate to a folder outside the roots and remapping a root | V5, F11, F13, C5, C22, W10, W11, W20 |
| T6 | Beatgrids wrong on variable-tempo music; high-precision mode slow, crashing, breaking auto-loops | Confirmed | High: 3 release fixes (7.1.3, 7.2.6, 7.2.14), 2026 press, MIT thesis | **Partial.** Own analysis with measured parity, dynamic grids where tempo really varies, analysis never replaces an edited or locked grid (D10), grid editor (phase 3 task 13). Missing: editing tempo sections (phase 3 non-goal "dynamic-grid editing beyond anchor nudge") | V1, F5, F6, P7, C2, C15, W5, W19 |
| T7 | Key detection less accurate than dedicated tools | Confirmed (weak) | Low–medium | **Covered** with limits: libKeyFinder port, identical to the C++ library, confidence shown (D12); agreement with rekordbox 62–67% exact (phase 3 task 12a) | V1, C1, C13, W7 |
| T8 | Auto cue points (intro, drop, outro) | Confirmed | High: Mixed In Key USD 58/99, Lexicon's top tier, rekordbox 7 "Intelligent Cue" and 7.2.14 SMART CUE | **Gap.** Phrase analysis is a phase 3 non-goal | W1, W7, P15, P24, C9, V1 |
| T9 | Cues lost or capped between apps (8 hot cues via XML, memory-cue colours, hot cue OR memory cue on Engine import) | Confirmed | Medium | **Partial.** XML import/export keeps hot cues 0–7, memory cues, loops and colours (phase 1 task 7). Missing: hot-cue banks (phase 2 non-goal; OneLibrary `hotCueBankList` written empty, phase 4 non-goal) | F7, P8, C7, C10, W4, W16 |
| T10 | Grids and cues shift 26–30 ms when a library moves between apps (MP3 encoder delay) | Confirmed | Medium: Algoriddim thread, 27 posts | **Covered.** reks positions are rekordbox positions per codec (D4) | C6, P5, C19 |
| T11 | Finding tracks: weak intelligent playlists, no playlist search until 7.1.3 | Confirmed | Medium | **Covered.** Nested all/any/NOT rule trees and smart playlists (D29), FTS5 search (D3) | F8, C16, V1, W1 |
| T12 | Metadata cleanup: bulk edit, My Tag portability, tags only in rekordbox's DB | Confirmed | Medium | **Partial.** My Tags, imported from XML Comments (D29); edit one track at a time (phase 1 task 9). Missing: multi-track edit. Tag write-back to files is a phase 1 non-goal | V5, C17, C18, W12, W17, R5 |
| T13 | Seeing and pruning what's on a stick; folder tree lost; convert overwrites OneLibrary-only playlists and history | Confirmed | Medium | **Covered.** Stick profiles with per-playlist sync status and folder tree (D27, phase 2 task 15), adopt a foreign stick (phase 2.5 task 12), no export loses player history (D23) | V1, V5, F14, C8 |
| T14 | Play history: naming, playlist info, import without deleting | Anecdotal (vendor platform only) | Low | **Covered** (D23, phase 2.5). Missing: setlist export to text or CSV | F9, V1, V5 |
| T15 | Subscription, paywalls, hardware no longer unlocking v7, billing outage, cancellation | Confirmed | Very high: largest press volume; Trustpilot 1.8/5 (15 reviews) | **Covered** by design: GPL, free, no account, nothing paywalled | P10–P12, P15–P17, P19, P20, V7, V10, R1, R2, W14 |
| T16 | Cloud Library Sync: split audio/metadata, duplicates, deleted Dropbox files (6.7.2), slow app | Confirmed | High | **Out of scope** for live sync: one library per computer (D28), and a synced SQLite file is a corruption risk. **Gap**: moving a library to another computer | V1, V5, V12, F10, P9, C14, W6 |
| T17 | Slow with large libraries | Confirmed | Medium: fixes in 7.0.3/7.0.8, forum | **Partial.** Targets set (10,000 rows at 60 fps, search < 100 ms) but phase 1 acceptance (#16) not signed off; rekordbox analysis mapped in 10 s instead of re-run (D10) | V1, F12, F15, P16, W15 |
| T18 | Streaming services: churn forces updates and cue redo | Confirmed | High in release notes | **Out of scope** (phase 1 and phase 5 non-goals) | V1, V8, V9, P18 |
| T19 | Migrating from or to Serato, Traktor, Engine | Confirmed | High: 6 conversion tools, paid and free | **Partial.** rekordbox XML in and out (D11); Engine export planned (phase 6, D15). Missing: Serato and Traktor import | W1, W16, W21, P22, P23, C9–C12 |
| T20 | CDJ settings (MYSETTING) still need rekordbox | Anecdotal | Low | **Gap.** Phase 2 copies settings files, never writes them | C17, W3 |
| T21 | OS updates break the app; no Linux | Confirmed | Low–medium | **Covered**: macOS, Windows and Linux builds (D17). No claim beyond that | V1, R2 |
| T22 | Busy UI; export vs performance mode confusing | Anecdotal | Low | Not mapped: subjective | P20, R1, P25 |
| T23 | Mobile app, stems, DVS, live performance | Confirmed (mobile: app store) | Medium | **Out of scope**: desktop app (D1); live performance rejected (D21) | R3, R4, P15, P19 |

## Gaps worth adding

Score = signal × fit ÷ effort, each 1–3 (effort S = 1, M = 2, L = 3). Stick
risk is the chance the feature makes a stick fail on stage.

| Rank | Feature | Theme | Signal | Fit | Effort | Score |
|---|---|---|---|---|---|---|
| 1 | Check a stick | T1, T2 | 3 | 3 | S | 9 |
| 2 | Target players per stick | T1 | 3 | 3 | S | 9 |
| 3 | Automatic library backups | T3 | 3 | 3 | S | 9 |
| 4 | Relocate missing files | T5 | 3 | 3 | S | 9 |
| 5 | Bulk metadata edit | T12 | 2 | 3 | S | 6 |
| 6 | Large-library benchmark | T17 | 2 | 3 | S | 6 |
| 7 | Duplicate finder and merge | T4 | 3 | 3 | M | 4.5 |
| 8 | Tempo-section grid editing | T6 | 3 | 3 | M | 4.5 |
| 9 | Auto cue suggestions | T8 | 3 | 2 | M | 3 |
| 10 | Move a library to another computer | T16 | 3 | 2 | M | 3 |
| 11 | Setlist export (text, CSV) | T14 | 1 | 3 | S | 3 |
| 12 | Serato and Traktor import | T19 | 3 | 2 | L | 2 |
| 13 | Convert on export | T2 | 2 | 2 | M | 2 |
| 14 | CDJ settings writer | T20 | 1 | 2 | M | 1 |
| 15 | Hot-cue banks | T9 | 1 | 2 | M | 1 |
| 16 | Tag write-back to files | T12 | 2 | 1 | M | 1 |

**1. Check a stick.** Problem: DJs find out at the gig that a stick won't
load (V2 told users to hunt for tracks without a waveform preview; P3
advises testing every stick). Feature: a read-only report for any mounted
stick, from rekordbox, reks or another tool: which library formats it holds
and which player families read them, missing audio or ANLZ files, tracks
without a waveform preview, `audiocheck` issues, playlists that differ
between the two formats, unimported history. S, `reks-export-rb` (`browse`,
`pdbread`, `audiocheck`, validator), phase 2.5 area. Stick risk: none (reads
only). Constraint: reading a rekordbox-written OneLibrary needs the key (D14);
the player-format table comes from V4 and stays UNVERIFIED per player.

**2. Target players per stick.** Problem: a stick works on a CDJ-3000 and is
blank on a CDJ-3000X, or the reverse (V4, C8). Feature: the stick profile
(D27) records the players it is for; the planner then requires OneLibrary
when a OneLibrary-only player is targeted, refuses exFAT for players that
need FAT32 (D16), and sets `audiocheck` severity from the oldest target. S,
`reks-export-rb` planner, `reks-core::device`. Stick risk: lowers it, but it
makes the experimental OneLibrary writer (D14) mandatory for some targets, so
it needs a decision (Proposed follow-ups).

**3. Automatic library backups.** Problem: one bad update or disk loses
every cue and grid (F3, C3, P13), and rekordbox's own backup is paywalled
(V6). Feature: SQLite online backup of `reks.sqlite` on start, before every
migration and before bulk operations; keep the last N in `app_data_dir`;
Settings → Library → Restore. S, `reks-core::db` + a Tauri command. Stick
risk: none.

**4. Relocate missing files.** Problem: folder reorganisations leave
hundreds of "!" tracks, and rekordbox's Relocate picks the first name match
(F11, W11). Feature: "Find missing files in…" scans a chosen folder, matches
by content hash first, then size and duration, never by name alone; a dry
run lists matches and ambiguities; also remap a library root. S,
`reks-core::scan`. Stick risk: low; relocated tracks keep their export
manifest rows.

**5. Bulk metadata edit.** Problem: cleaning genre, tags, colour or rating
one track at a time (W12, R5). Feature: the detail panel edits every
selected track, showing mixed values. S, `reks-core::track`, UI. Stick risk:
none.

**6. Large-library benchmark.** Problem: rekordbox slowed down on big
libraries twice in 7.0.x (V1); users report 20,000-track libraries (R5).
Feature: finish phase 1's performance criteria with a 50,000-track
synthetic library (scroll, search, smart playlist query, export plan time)
and track it in CI. S, `reks-core`, `fixtures/gen`. Stick risk: none.

**7. Duplicate finder and merge.** Problem: rekordbox took until 7.2.14 to
grow duplicate tools, and people still buy fingerprint-based ones (W8, W10).
Feature: group by content hash (exact) and by PCM hash or duration plus
tags (likely); merge into one track keeping the union of cues, tags,
playlist entries and history, with a dry run. M, `reks-core`. Stick risk:
low if the merge keeps `export_manifest` and history links (D23); otherwise
history matching breaks.

**8. Tempo-section grid editing.** Problem: tracks that change tempo come
out wrong and must be fixed by hand, section by section (C15, P7). Feature:
add, move and delete tempo markers in the grid editor; keep the grid locked
afterwards. M, phase 3 grid editor + `reks-core::beatgrid`. Stick risk:
medium: multi-tempo PQTZ is written by rekordbox and baken, but reks has no
player test of its own (UNVERIFIED). Needs a phase 3 scope change.

**9. Auto cue suggestions.** Problem: people pay Mixed In Key for cue points
(W7, P24). Feature: suggest memory cues at the first downbeat, energy
changes and the outro, from the grid and the band columns reks already
computes; suggestions are a separate cue source and never replace user
cues. M, `reks-analysis`. Stick risk: low (ordinary memory cues). Needs a
phase 3 scope change (phrase analysis is a non-goal).

**10. Move a library to another computer.** Problem: live cloud sync loses
files and duplicates playlists (P9, V1, C14), but DJs with two laptops still
need their prep on both. Feature: export a library bundle (database, settings,
analysis cache index) and import it on another machine with root remapping
(uses 4); no live sync. M, `reks-core`. Stick risk: stick IDs are per OS
(D28), so sticks relink once.

Below the top ten: setlist export is cheap but rarely asked for; Serato and
Traktor import has high signal but needs Serato's tag format reverse
engineered (Traktor NML is plain XML); convert-on-export puts a transcoder in
the export path, which D25 rejected without a decision; the CDJ settings
writer and hot-cue banks need format work on files players read; tag
write-back conflicts with "the library DB is the source of truth" (phase 1).

## Where reks is already ahead

For the README and positioning. No item claims player compatibility.

- **Free and open.** GPL, no subscription, no account, no hardware unlock,
  nothing paywalled (T15).
- **Every stick write is a dry run first,** then read back by an independent
  parser (D8). rekordbox shipped a point release whose exports players
  couldn't load (V2).
- **Stick backups and history are free and automatic** (D23); rekordbox's
  Device Library Backup needs the Professional plan (V6) and its convert step
  can drop OneLibrary-only history (V5).
- **Sticks are managed, not rewritten blind:** per-stick playlist sets with
  sync status, and foreign sticks can be adopted (D27).
- **An open library:** unencrypted SQLite plus rekordbox XML in and out
  (D3, D11). rekordbox's encrypted `master.db` is why pyrekordbox, Lexicon
  and RCT exist (W18, W1, W8).
- **Explicit timing convention:** positions match rekordbox per codec (D4),
  the cause of the 26–30 ms drift other apps show (C6).
- **Analysis respects your edits:** grids you edited or locked are never
  replaced, and rekordbox's analysis is reused instead of redone (D10).
- **Rule-tree smart playlists** with nested any/all/NOT (D29).
- **Format check and lossless converter** before export (phase 2 task 3,
  D25): what GigSafe and RCT sell.
- **Local files only:** no streaming platform change can force a cue redo
  (V8). **Linux** builds (D17). **No telemetry** (D34).

## Proposed follow-ups

None of these is a decision yet; each needs the maintainer's call.

1. **Target players on stick profiles** (gap 2). Proposed: profiles carry a
   player list; targeting a OneLibrary-only player requires the OneLibrary
   writer (D14 experimental flag) and says so in the dry run. Touches D14,
   D16, D27.
2. **Library snapshots** (gap 3). Proposed: SQLite backup API, last 10 kept
   in `app_data_dir/library-backups/`, taken on start and before migrations
   and bulk operations. New entry next to D3/D19.
3. **Phase 3 scope** for tempo-section editing (gap 8) and auto cue
   suggestions (gap 9); both are phase 3 non-goals today.
4. **Convert on export** (T2): revisit D25's rejection, since the CDJ
   compatibility tools people buy (W2, W9) fix files on the stick copy.
5. **Reddit sweep**: repeat from a browser that can reach Reddit before
   reweighting the ranking.

## Sources

All accessed 2026-10-24. "snippet" = seen only as a search result.

Vendor (rekordbox.com, AlphaTheta, Pioneer DJ)
- V1 rekordbox release notes 6.6.11–7.2.19, 2023-03 to 2026-09: https://rekordbox.com/en/support/releasenote/
- V2 7.2.12 withdrawn, 2026-03-17: https://rekordbox.com/en/2026/03/important-notice-regarding-rekordbox-v7212/
- V3 CDJ-3000 firmware 3.30 notice, 2026-01-07: https://www.pioneerdj.com/en-gb/news/2026/cdj-3000-firmware-ver330-important-notice
- V4 USB devices notice (two library formats), 2026-03-18: https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/
- V5 Troubleshooting FAQ (v7), undated: https://rekordbox.com/en/support/faq/trouble-shooting-7/
- V6 Device Library Backup FAQ, undated (snippet): https://rekordbox.com/en/support/faq/device-library-backup-7/
- V7 Subscription outage, 2025-01-25 to 2025-03-17: https://rekordbox.com/en/2025/02/subscription-problem-caused-by-server-issues/
- V8 SoundCloud update notice, 2026-05: https://rekordbox.com/en/2026/05/important-notice-for-soundcloud-users-request-to-update-rekordbox/
- V9 Apple Music update notice, 2025-12: https://rekordbox.com/en/2025/12/important-update-using-apple-music-with-our-products/
- V10 Studio Plan, 2026-10-13: https://rekordbox.com/en/2026/10/introducing-studio-plan-take-djing-beyond-the-booth/
- V11 OneLibrary launch, 2025-10: https://rekordbox.com/en/2025/10/dj-brands-unite-to-launch-onelibrary/
- V12 Cloud Option, 2023-12: https://rekordbox.com/en/2023/12/cloud-option/

Pioneer DJ forum (all snippets, undated)
- F1 Device Library Plus issues: https://forums.pioneerdj.com/hc/en-us/community/posts/16571566965913-issues-with-usb-device-after-upgrading-to-device-library-plus
- F2 Device library corrupted: https://forums.pioneerdj.com/hc/en-us/community/posts/360030612011-Device-library-is-corrupted-and-ask-for-delete
- F3 Database crash, three automatic backups: https://forums.pioneerdj.com/hc/en-us/community/posts/360035220651-Record-Box-Data-Base-Crash
- F4 Duplicate files: https://forums.pioneerdj.com/hc/en-us/community/posts/14747218582041-Duplicate-Files
- F5 Cloud Analysis gives wrong BPM: https://forums.pioneerdj.com/hc/en-us/community/posts/19738611593369-PSA-Cloud-Analysis-is-problematic-you-may-want-to-disable-it-as-it-causes-variable-tempo-incorrect-BPM-results
- F6 Incorrect BPM values: https://forums.pioneerdj.com/hc/en-us/community/posts/360022149911-Incorrect-BPM-Values
- F7 Hot cues not exported to USB: https://forums.pioneerdj.com/hc/en-us/community/posts/16336695021465-Rekordbox-not-correctly-exporting-hot-cues-to-USB
- F8 Intelligent playlists upgrade request: https://forums.pioneerdj.com/hc/en-us/community/posts/115017700703-Intelligent-Playlists-GET-THIS-FUNCTION-UPGRADED-PLEASE
- F9 History requests: https://forums.pioneerdj.com/hc/en-us/community/posts/360016129403-Rekordbox-History
- F10 Duplicate tracks in Dropbox: https://forums.pioneerdj.com/hc/en-us/community/posts/21776906899097-Duplicate-tracks-in-Dropbox-and-Rekordbox
- F11 Relocating missing files: https://community.pioneerdj.com/hc/en-us/community/posts/22978953776409-Rekordbox-Relocating-missing-files-easily
- F12 Slow loading library: https://forums.pioneerdj.com/hc/en-us/community/posts/360009687486-Rekordbox-slow-loading-library
- F13 Display Missing Tracks not working: https://forums.pioneerdj.com/hc/en-us/community/posts/16862578582041-Display-Missing-Tracks-NOT-Working
- F14 Keep folder structure on export: https://forums.pioneerdj.com/hc/en-us/community/posts/360048845931-RekordBox-Export-individual-playlists-to-the-same-location-in-structure-tree-as-where-they-re-located-in-collection
- F15 7.0.7 crashing, ~2024: https://community.pioneerdj.com/hc/en-us/community/posts/41343351607833-Rekordbox-7-0-7-doesn-t-work
- F16 Restoring a corrupt database: https://forums.pioneerdj.com/hc/en-us/community/posts/12766013558681-Restoring-corrupt-database

DJ press
- P1 Digital DJ Tips, 7.2.12 pulled, 2026-03-22: https://www.digitaldjtips.com/alphatheta-pulls-rekordbox-7-2-12-over-usb-export-bug/
- P2 Digital DJ Tips, CDJ-3000 firmware pulled, 2025-11-10: https://www.digitaldjtips.com/alphatheta-pulls-cdj-3000-firmware-after-playlist-issues/
- P3 MusicTech, firmware bug at gigs, 2026-01-09: https://musictech.com/features/pioneer-dj-cdj-3000-firmware-bug-jaguar-mojaxx/
- P4 DJ Mag, firmware suspended, 2025-11 (snippet): https://djmag.com/tech/alphatheta-suspends-cdj-3000-firmware-update-distribution-following-playlist-issues
- P5 Digital DJ Tips, OneLibrary five things, 2026-04-17: https://www.digitaldjtips.com/alphatheta-onelibrary-5-things-you-need-to-know/
- P6 Digital DJ Tips, Device Library Plus, 2024-07-10: https://www.digitaldjtips.com/rekordbox-device-library-plus/
- P7 Digital DJ Tips, auto-loop bug, 2026-04-17: https://www.digitaldjtips.com/rekordbox-auto-loop-bug-how-to-fix-it/
- P8 Digital DJ Tips, two cue sets per track, 2026-04-14: https://www.digitaldjtips.com/rekordbox-trick-cues-same-track/
- P9 Digital DJ Tips, Cloud Library Sync advice, 2026-04-16: https://www.digitaldjtips.com/rekordbox-cloud-library-sync-advice/
- P10 Digital DJ Tips, older controllers must subscribe, 2024-07-11: https://www.digitaldjtips.com/owners-of-older-pioneer-dj-controllers-must-now-subscribe-to-use-rekordbox-7/
- P11 Digital DJ Tips, subscription system down, 2025-02-10: https://www.digitaldjtips.com/rekordbox-subscription-system-down-what-users-need-to-know/
- P12 Digital DJ Tips, subscriptions return at a higher price, 2025-03-19: https://www.digitaldjtips.com/rekordbox-subs-return-but-youll-pay-more-for-now-at-least/
- P13 Digital DJ Tips, DJ software secrets, 2025-05-23: https://www.digitaldjtips.com/dj-software-secrets/
- P14 Digital DJ Tips, RCT, 2023-11-29: https://digitaldjtips.com/?p=2780750
- P15 Digital DJ Tips, rekordbox 7 launched, 2024-05-14: https://www.digitaldjtips.com/rekordbox-7-launched/
- P16 CDM, rekordbox 7, 2024-05-14: https://cdm.link/rekordbox-7/
- P17 GearNews, subscription issues, 2025-02-06: https://gearnews.com/alphatheta-rekordbox-subscription-issues
- P18 DJ TechTools, Spotify in rekordbox, 2025-09-24: https://djtechtools.com/2025/09/24/spotify-finally-slides-into-rekordbox-serato-and-algoriddim-djay-again-for-better-or-worse/
- P19 We Are Crossfader, rekordbox 7, 2024-05-14: https://wearecrossfader.co.uk/blog/rekordbox-7/
- P20 Violet Recording review, 2026-08-06: https://violetrecording.com/rekordbox-review/
- P21 Digital DJ Tips, OneLibrary launch, 2025-10-21: https://www.digitaldjtips.com/onelibrary/
- P22 Digital DJ Tips, VirtualDJ joins OneLibrary, 2026-09-22: https://www.digitaldjtips.com/virtualdj-now-officially-supports-alphathetas-onelibrary/
- P23 Digital DJ Tips, DJCU 4 review, 2020-07-13 (updated 2026-01-20): https://www.digitaldjtips.com/reviews/dj-conversion-utility-4-0/
- P24 Digital DJ Tips, Mixed In Key 11 Pro, 2025-03-31: https://www.digitaldjtips.com/mixed-in-key-11-pro/
- P25 MusicRadar, rekordbox 7 news, 2024-05: https://www.musicradar.com/news/rekordbox-7-dj-software

Independent forums and blogs
- C1 DJ TechTools, key detection, 2026-05-25: https://forum.djtechtools.com/t/music-key-detection-alternative-to-mixed-in-key/154785
- C2 DJ TechTools, grid with changing tempo, 2020-08-13: https://forum.djtechtools.com/t/how-to-set-beatgrid-when-tempo-is-not-consistent/78385
- C3 DJ TechTools, cues lost after reinstall, 2015-05-07: https://forum.djtechtools.com/t/rekordbox-problem-i-think-i-have-screwed-up/71133
- C4 DJ TechTools, moving computers, 2017-04-04: https://forum.djtechtools.com/t/moving-computers/75847
- C5 DJ TechTools, media is missing, 2014-06-04: https://forum.djtechtools.com/t/rekordbox-media-is-missing/66428
- C6 Algoriddim, OneLibrary misaligned grids, 2025-12-06: https://community.algoriddim.com/t/rekordbox-onelibrary-misaligned-beatgrids/40391
- C7 Algoriddim, OneLibrary deleting hot cues, 2025-11-21: https://community.algoriddim.com/t/onelibrary-is-deleting-my-hot-cues/40032
- C8 Algoriddim, rekordbox 5–7 backwards compatibility, 2025-10-27: https://community.algoriddim.com/t/top-tip-rekordbox-5-7-users-backwards-compatibility/39409
- C9 Engine DJ forum, rekordbox as master library, 2024-03-26: https://community.enginedj.com/t/using-rekordbox-instead-of-engine-dj-for-library-management/55477
- C10 Engine DJ forum, XML import doesn't update, 2025-02-28: https://community.enginedj.com/t/rekordbox-library-xml-doesnt-even-try-to-update/62380/17
- C11 Engine DJ forum, BPM changes lost, 2025-06-19: https://community.enginedj.com/t/automatic-bpm-jump-from-rekordbox-library/64189
- C12 Engine DJ forum, moving to Engine, 2024-08-06: https://community.enginedj.com/t/new-user-the-pros-of-transitioning-to-engine-dj/58034
- C13 Engine DJ forum, best analysis software, 2022-02-26: https://community.enginedj.com/t/best-analytical-software-for-bpm-key-downbeat-cues/40389
- C14 Dropbox forum, duplicate playlists from Cloud Sync, ~2026 (snippet): https://www.dropboxforum.com/discussions/101001017/rekordbox-cloud-sync-issue---duplicate-playlists/831719
- C15 Really Chrism, gridding impossible tracks, 2024-12-23: https://reallychrism.substack.com/p/how-to-grid-impossible-tracks
- C16 Really Chrism, picking your own tracks, 2025-06-16: https://reallychrism.substack.com/p/youre-not-picking-your-own-tracks/comments
- C17 Really Chrism, not cheering OneLibrary, 2025-10-27: https://reallychrism.substack.com/p/why-im-not-cheering-onelibrary
- C18 Really Chrism, tagging the wrong thing, 2025-07-28: https://reallychrism.substack.com/p/youre-tagging-the-wrong-thing
- C19 Midnight Rebels, OneLibrary, 2025-10-22: https://midnightrebels.com/onelibrary-how-dj-software-rivals-finally-ended-the-format-wars-but-not-really/
- C20 r/Beatmatch via a mirror, unsupported file on CDJ, date unknown (snippet): https://redlib.hbubli.cc/r/Beatmatch/comments/1kxa5bd/cdj_unsupported_file_bugfix
- C21 Gearspace, CDJ-3000 FAT32 only (snippet): https://gearspace.com/threads/pioneer-cdj-cdj-3000-once-again-lacking-exfat-ntfs-support.1325406/
- C22 Gearspace, migrating to a new Mac (snippet): https://gearspace.com/board/electronic-music-instruments-and-electronic-music-production/958921-migration-rekordbox-new-mac-computer.html

Reviews and app stores
- R1 Trustpilot, rekordbox.com (1.8/5, 15 reviews), 2024-11 to 2026-08: https://www.trustpilot.com/review/rekordbox.com
- R2 Trustpilot, alphatheta.com (2.3/5, 8 reviews), 2025-09 to 2026-06: https://www.trustpilot.com/review/alphatheta.com
- R3 App Store, rekordbox for iOS (4.6/5, 3.5K ratings): https://apps.apple.com/us/app/rekordbox-dj-app-dj-mixer/id1487048203
- R4 Google Play, rekordbox (~4.5/5, snippet): https://play.google.com/store/apps/details?id=com.pioneerdj.rekordbox
- R5 Trustpilot, Lexicon (vendor-invited, 4.8/5, 181 reviews), 2026-06 to 2026-10: https://www.trustpilot.com/review/rekord.cloud

Workaround tools and vendors (pitches unless noted)
- W1 Lexicon DJ: https://www.lexicondj.com
- W2 Lexicon, CDJ compatibility check, 2026-09-04: https://www.lexicondj.com/blog/will-your-tracks-play-on-the-club-cdjs-lexicons-cdj-compatibility-tool
- W3 Lexicon, both stick formats, 2026-08-28: https://www.lexicondj.com/blog/one-usb-every-pioneer-player-exporting-for-both-devicelibrary-and-onelibrary-hardware
- W4 Lexicon, library comparison 2026, 2026-07-08: https://www.lexicondj.com/blog/serato-vs-rekordbox-vs-engine-dj-vs-traktor-in-2026-which-library-should-you-build-on
- W5 Lexicon, rekordbox beatgrid analysis, 2025-06-16: https://www.lexicondj.com/blog/understanding-rekordbox-beatgrid-analysis
- W6 Lexicon, one library on two computers, ~2026-10: https://www.lexicondj.com/blog/one-dj-library-on-two-computers-rekordbox-cloud-vs-dropbox-vs-lexicon
- W7 Mixed In Key, rekordbox workflow: https://mixedinkey.com/workflows/use-mixed-in-key-with-rekordbox/
- W8 Rekordbox Collection Tool (RCT): https://atgr-production-team.sellfy.store/p/rct/
- W9 GigSafe (snippet): https://alexeneas.gumroad.com/l/GigSafe
- W10 rekordbox-library-fixer (GitHub): https://github.com/koraysels/rekordbox-library-fixer
- W11 rekordbox-repair (GitHub, MIT): https://github.com/edkennard/rekordbox-repair
- W12 LibTools4DJs, 2025-12-14: https://www.nuget.org/packages/libtools4djs
- W13 Digital DJ Pool, library tips, 2026-03-28: https://digitaldjpool.com/blog/rekordbox-library-management-tips/
- W14 Digital DJ Pool, Serato vs rekordbox, 2026-07-06: https://digitaldjpool.com/blog/serato-vs-rekordbox/
- W15 DJ.Studio, community summary, 2026-04-03: https://dj.studio/blog/mixing-software-djs-trust-community
- W16 rb2engine, 2026-08-01: https://pypi.org/project/rb2engine/
- W17 MusicTech on Lexicon, 2025-09-30: https://musictech.com/news/gear/dj-library-management-lexicon/
- W18 pyrekordbox, master.db format: https://pyrekordbox.readthedocs.io/en/stable/formats/db6.html
- W19 MIT thesis on GridFix (snippet): https://dspace.mit.edu/handle/1721.1/162981
- W20 Music Library Doctor (snippet): https://musiclibrarydoctor.com/why-rekordbox-shows-missing-tracks.html
- W21 Traktor Bridge, NI forum (snippet): https://community.native-instruments.com/discussion/47624/traktor-bridge-free-export-nml-playlist-to-recordbox-database-xml-m3u-or-directly-on-usb-stick
