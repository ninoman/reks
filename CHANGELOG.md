# Changelog

User-facing changes to reks, written by `npm run release` from commit messages.
Versioning rules: [docs/releasing.md](docs/releasing.md#versioning).

## 0.3.0 (2026-10-30)

### Breaking changes

- **core:** An XML import replaces an analysis beatgrid instead of mixing with it
- **core:** XML re-import keeps manual BPMs and occupied hot cue slots; empty analysis keeps BPM
- **export-rb:** UNVERIFIED stick features on by default; warn before deleting exportLibrary.db

### Features

- **ui:** One-channel mixer beside the player deck in 1-deck mode (#85)
- **mix:** Grid mode on the deck replaces the grid editor dialog (#84)
- **ui:** Synced decks show BPM, tempo % and SYNC in cyan
- **mix:** Sync matches beats only; nudge and part-beat jumps switch to BPM sync
- **mix:** Drag a track from one deck to another as it plays
- **mix:** Master tempo plays through Signalsmith Stretch, picked by blind listening
- **core:** RatingRange and tagAll filters; split the rule-tree codec

### Fixes

- **ui:** Beat jump pads no longer show an active outline
- **export-rb:** Stop flagging hot cues D–H as a cue count mismatch after export
- **core:** Run library migrations with foreign keys off and check them per step
- **core:** Every bulk operation backs up the library first, in the app and the CLI
- **export-rb:** Read every export.pdb with our own bounds-checked reader
- **export-rb:** Delete only the files the dry run listed, after flushing the new ones
- **mix:** CUE release no longer drops a pending load; SetGrid refuses other audio
- **ui:** Error boundary per region; a reload stops the running engine

### Performance

- **export-rb:** Flush the stick once instead of per file, and stop leaving ._ files on it
- **export-rb:** Read far less of each source file when planning and building ANLZ
- **export-rb:** Load playlist entries and track fields in batches
- **core:** Read files in parallel during import and scan
- **ui:** Select-all loads its pages in one pass; cap dry-run file lists; parse the import XML once

## 0.2.0 (2026-10-29)

### Features

- Scaffold Cargo workspace, Tauri 2 app and CI (phase 1 task 1)
- **fixtures:** Add sine+click generator, audio fixtures and rekordbox XML
- **core:** Add SQLite schema, migrations and key parsing (phase 1 task 3)
- **audio:** Add probe, tag and artwork reading; artwork cache (phase 1 task 4)
- **core:** Add folder scan with move detection and cancellation (phase 1 task 5)
- **core:** Add rekordbox XML parser and writer (phase 1 task 6)
- **core:** Add rekordbox XML import (phase 1 task 7)
- Add track table and library query API (phase 1 task 8)
- **ui:** One-step rekordbox XML import with locate-missing follow-up
- **core:** Track detail, metadata edit and artwork bytes services
- **ui:** Track detail side panel with inline metadata edit
- **core:** Playlist tree and edits with duplicate handling
- **app:** Playlists_get and playlists_mutate commands
- **ui:** Playlist tree with drag and drop and the duplicate dialog
- **ui:** Rekordbox-style playlist tree and a collapsible filter panel
- **ui:** Switch filters on and off apart from collapsing the panel
- **player:** One-deck preview player on the reks-mix engine
- **import:** Rekordbox XML import wizard
- **ui:** Filter bar above the list, slide-over track info, app header
- **settings:** Library folders, output device, key notation, cache dir
- **app:** App icon, window state persistence, release workflow
- **ui:** Highlight the loaded track's row in the track table
- **ui:** Add base components
- **ui:** Violet accent instead of orange
- **ui:** White text on dark-mode primary buttons
- **core:** Fsname module for FAT32-safe stick paths
- **export-rb:** Audiocheck for player-unsupported audio (phase 2 task 3)
- **export-rb:** Device discovery, eject and volume serial
- **export-rb:** Export planner (phase 2 task 4)
- **export-rb:** Locate rekordbox's ANLZ cache (phase 2 task 5)
- **core:** Export a playlist selection as rekordbox XML (phase 2 task 6)
- **export-rb,core:** History safeguard and export manifest (phase 2 tasks 12-13)
- **export-rb:** BakenWriter over baken-export (phase 2 task 7)
- **export-rb:** Read-back validator (phase 2 task 8)
- **export-rb:** Wire planner to rbcache and history; add reks rb plan/history
- **app:** USB device list and eject (phase 2 task 1)
- **export-rb:** Export orchestration apply() and reks rb export/validate
- **export-rb:** Dry-run view of an export plan for the UI
- **ui:** Export wizard through the dry run (phase 2 task 10)
- **ui:** Export from the wizard with progress, cancel and report (phase 2 task 10)
- **core:** Stick profile tables and storage (phase 2 task 15)
- **export-rb:** Stick profile API: identify, status, record_sync (phase 2 task 15)
- **audio:** Converter building blocks: PCM writers, FLAC, native stream (phase 2.6 tasks 1-3)
- **app:** Stick profiles in the Devices panel and export wizard (phase 2 task 15)
- **audio:** Convert() pipeline with tag copy and read-back verify (phase 2.6 task 4)
- **core:** Converter plan/apply against the library (phase 2.6 task 5)
- **core:** Playlist dividers and play-history schema (phase 2.5 tasks 1-2)
- **export-rb:** History reader for sticks and backups (phase 2.5 task 5)
- **core:** Match, fingerprint and import stick play history (phase 2.5 task 6)
- **app:** Converter dialog, track menu and export link (phase 2.6 task 6)
- **cli:** Reks convert plan/apply (phase 2.6 task 7)
- **ui:** Playlist divider rows, insert, rename and drag (phase 2.5 task 3)
- **ui:** Indigo accent on cool mist neutrals
- **core:** Derived play counts and frozen baseline (phase 2.5 task 7)
- **cli:** Reks history scan/import (phase 2.5 task 10)
- **export-rb:** Export playlist dividers as silent tracks (phase 2.5 task 4)
- **export-rb:** Export guard for unimported play history (phase 2.5 task 8)
- **ui:** History UI: sessions tree, session view, import dialog (phase 2.5 task 9)
- **core:** Adopt a foreign stick (phase 2.5 task 12)
- **audio:** Decode_f32 for analysis, with encoder delay and padding
- **analysis:** Decode conditioning and Beat This! beat tracking (phase 3 tasks 1-2)
- **analysis:** Beatgrid builder (phase 3 task 3)
- **analysis:** Key detection (phase 3 task 5)
- **analysis:** Streaming 3-band waveform columns at 150/s (phase 3 task 6)
- **anlz:** Pioneer waveform encoders PWAV/PWV2-7/PWVC (phase 3 task 7)
- **anlz:** ANLZ builder, validator and golden test (phase 3 task 9)
- **analysis:** Background analysis queue (phase 3 task 11)
- **core:** Beatgrid editing maths, manual grids and grid lock
- **analysis:** On-demand waveform columns with a disk cache
- **ui:** Grid editor (phase 3 task 13)
- **export-rb:** Write reks-built ANLZ in the legacy export (phase 3 task 10)
- **analysis:** Calibrate waveforms against rekordbox (phase 3 task 8)
- **audio:** Put decoded audio on rekordbox's timeline (phase 3 task 4)
- **export-rb:** Reks rb diff, structural stick diff against a reference
- **cli:** Analysis parity harness and baseline report (phase 3 task 12)
- **analysis:** Fit the constant tempo over the whole track (phase 3 task 12a)
- **analysis:** Line the grid up with the kicks (phase 3 task 12a)
- **export-rb:** OneLibrary planner seam and key setting (phase 4 task 8)
- **export-rb:** OneLibrary step in the planner and dry run (phase 4 task 8)
- **ui:** OneLibrary export toggle and key in Settings (phase 4 task 8)
- **export-onelibrary:** SQLCipher build and keyed open (phase 4 task 2, part)
- **export-rb:** OneLibrary step in apply, after export.pdb (phase 4 task 8)
- **export-rb:** Typed export.pdb reader for the OneLibrary writer (phase 4 task 4)
- **export-onelibrary:** Writer, reader, validator and diff (phase 4 tasks 2-7, 10)
- **analysis:** Bar placement by downbeat vote and first kick (phase 3 task 12a)
- **export-onelibrary:** Import OneLibrary play history (phase 4 task 11)
- **cli:** LibKeyFinder C++ check on the parity set; keep its mode decision (phase 3 task 12a)
- **mix:** Engine rate and buffer selection, four decks, deck/sync/mixer controls (phase 5 task 1)
- **mix:** Decks with cue, hot cues, quantise and rubato varispeed (phase 5 task 2)
- **mix:** Beat and bar sync with leader and phase correction (phase 5 task 3)
- **mix:** 4-band asymmetric EQ, knob tapers and parameter smoothing (phase 5 task 4)
- **mix:** X/Y state-variable filters with modes, resonance, LINK and VCF to xfader (phase 5 task 5)
- **mix:** Channel level, crossfader law, assign switch, meter and safety limiter DSP (phase 5 task 6, partial)
- **mix:** Channel strips, crossfader, master and limiter in the graph (phase 5 task 6)
- **mix:** Headphone cue bus and output routing (phase 5 task 7)
- **mix:** Mix_* Tauri commands and typed IPC (phase 5 task 8)
- **ui:** Mix panel with decks, mixer and shortcuts (phase 5 task 9)
- **cli:** Keylock-render for the key lock listening test (phase 5 task 10)
- **mix:** Mixer meters in the UI and per-device cue routing default (phase 5 tasks 8-9)
- **ui:** Themed start-up splash; open the library off the main thread
- **core:** My Tags, tag filter and tags from XML Comments (D29 task 1)
- **ui:** My Tags — manager, context-menu submenu, chip editor, filter picker
- **core:** Rule tree filters with all/any groups and NOT (D29 task 2)
- **core:** Smart playlists resolved everywhere playlists are read (D29 task 3)
- **ui:** Mix toggle in the transport bar, track table fills the width
- **ui:** DJM-style LED meters, lit mixer switches, click/shift-click selection
- **mix:** CDJ tempo ranges, cue preview, drag to skim, decks 2/3 in 2-deck mode
- **ui:** Deck waveform style setting (3-band, RGB, Blue)
- **core:** Analyse rekordbox-gridded tracks last; propose skipping them (D10)
- **export-rb:** Map rekordbox analysis into the reks cache instead of re-analysing (D10)
- **ui:** Rule builder, smart playlists in the tree, toast, mock rules (D29 task 5)
- **ui:** Rule builder tests, smart list tree behaviour, docs for D29 task 5
- **core:** Playlist_rule reads a smart playlist's saved rule (D29 task 5)
- **ui:** Player is deck 2 of the mix engine, RGB waveform by default (D30)
- **ui:** Split track lists - Shift+click opens playlists in stacked panes
- **export-onelibrary:** Build the OneLibrary key into release builds
- **mix:** Master hand-off, beat jump and beat loops (phase 5 task 13)
- **mix:** Knob position mark, shortcut hints; remove VCF to crossfader (phase 5 task 14)
- **mix:** Crisp deck waveforms, per-deck zoom, cue and loop marks
- **mix:** Deck redesign (phase 5 task 16)
- **mix:** One waveform zoom for every deck, on the - and + keys
- **mix:** Cue editing on the decks (phase 5 task 17)
- **mix:** Memory loops with auto entry, × delete; drop auto cue on load (task 17)
- **mix:** Remove the filter LINK switch
- **ui:** My Tags drawer with attach, click-to-rename and drag reorder
- **ui:** Devices section in the sidebar to browse and play sticks
- **ui:** Decks on top, flat player deck, output settings, memory pad mode (phase 5 task 18)
- **ui:** Five colour themes with a Settings picker
- **ui:** Mix keys follow rekordbox's default Performance 1 layout
- **ui:** Split Settings into sidebar tabs
- **mix:** Xone:96 channel EQ with notch-shaped mid cuts
- **app:** In-app bug reports and feedback (D33)
- **ui:** Chips for decks playing unseen and a non-flat mixer in 1-deck mode
- **mix:** Manual loop IN/OUT, reloop and gridless resize
- **ui:** Re-lay the mixer to 344 x 440 with one dB scale and a full-width crossfader
- **core,ui:** Settings → Mixer tab and the mixer's settings button (phase 5 task 24)
- **ui:** Quick-loop block, loop size readout and loop keys on every deck
- **ui:** Size the mix decks to the mixer and the panel to the library
- **ui:** Simple mixer view hides the assigns, crossfader and optionally the filters
- **ui:** Mirror the mixer's filter units and light the ON lamp
- **ui:** Move Filters into the right drawer and drop the My Tags onboarding blurb
- **ui:** Booth is the default theme; segmented control wraps instead of overflowing
- **ui:** Label the WIDE tempo range W
- **ui:** One fluid deck controls row for 2 and 4 decks
- **ui:** Double-click loads onto a spare deck while mixing
- **ui:** Artwork thumbnails in the track list, newest-first default sort
- **mix:** Master tempo (key lock) on the decks via timestretch
- **mix:** 3-band EQ and channel FX mixer layout (#56)
- **ui:** Track list waveforms, preview deck quick play, row density
- **ui:** Settings choices as picture tiles
- **ui:** FX select in the mixer legend, PARAMETER under it
- **ui:** Compact filter row with lamp-column modes
- **ui:** Paged rekordbox-style quick loop block
- **core:** Saved filters (filter_preset table, ops, commands)
- **ui:** One Filters query with default fields (D35, phase 1 task 7)
- **ui:** Add filter searches fields and values (D35, phase 1 task 8)
- **ui:** Saved filters in the Filters drawer and toolbar (D35, phase 1 task 9)
- **ui:** Bulk edit metadata of selected tracks
- **core:** Relocate missing files and remap library folders
- **core:** Automatic library backups and restore
- **export-rb:** Check a stick before a gig
- **core:** Tempo-section grid edits with one beat rule
- **export-rb:** PQTZ from the beat rule; edited grids always reach the stick
- **mix:** Decks follow the library's beat list
- **ui:** Grid editor with tempo sections, snap and undo
- **core,ui:** My Tags from Comments — opt-in import, organizer, write-back
- **ui:** Start the rekordbox XML import wizard with export instructions
- **ui:** Deck waveform opens at 8 s and remembers the last zoom
- **mix:** Signed close-zoom deck waveform at 2 s and closer
- **ui:** Two-channel mixer in 2-deck mode

### Fixes

- **core:** Match Windows XML paths with native separators
- **ui:** Refuse a half-filled folder remap in the XML import form
- **app:** Keep the play button in step with Space playback
- **core:** Undated history sessions import counted (phase 2.5)
- **core:** Every imported history session counts by default
- **ui:** Undated history sessions default to counted (phase 2.5 task 9)
- **ui:** Every imported history session counts by default (phase 2.5 task 9)
- Reconcile phase 2.5 tasks 8 and 9 after merge
- **analysis:** Queue workers off the rayon pool; coordinate with the grid editor
- **phase-3:** Integrate tasks 9, 11 and 13
- **core:** Store beatgrid positions as whole ms everywhere
- **export-rb:** Export user-owned grids with reks ANLZ by default
- **analysis:** Let the kick choose a cluster of beats, not move the grid
- **cli:** Parity report uses a kick curve only with its own record
- **ui:** Double-clicking the playing track keeps it playing
- **mix:** Gentler filter resonance, filter direction tests, default filter assign
- **ui:** Avoid findLastIndex, not in the ES2022 lib target
- **ui:** Plain 'or' in rule summaries, clearer nested groups, screen-reader names for filter toggles
- **core:** Read My Tags from the block that ends Comments (D29)
- **mix:** Knob mark reaches past the cap's rim
- **mix:** Draw the loop from task 13's DeckView.loopRegion
- **core:** Re-extract missing artwork files from the audio on read
- **ui:** Drop the sidebar My Tags row; fix task 18 type errors
- **ui:** Draw a flat centre line through silent stretches of deck waveforms
- **mix:** Play sent during a load fade starts the new track
- **ui:** Add artworkId to TrackDetailPanel test fixture
- **mix:** Fit a whole mixer resync in the engine's control queue
- **ui:** Drop the quick loop page tab's dot
- **ui:** Add preview deck to MixState test fixtures
- **ui:** Rekordbox beat loop sizes, keep resized loop size, no fade on loop page switch
- **ui:** Tone down RGB waveform brightness
- Reconcile merged PRs (renumber D39, backup test for migration 13)
- **ui:** Allow undefined value on ResizeHandle for exactOptionalPropertyTypes
- **mix:** Every filter channel defaults to neutral HP; pad list waveforms
- **ui:** Keep a playing deck playing when a track is dropped on it

### Performance

- **ui:** Own reset instead of Tailwind preflight; cheap table swatches
- **ui:** Track list waveforms from stored analyses; resizable columns, sidebar and drawers
- **ui:** Zoom deck waveform further in; build tiles ahead of the view

