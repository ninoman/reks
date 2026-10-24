# Check a stick before a gig

reks reads a plugged-in USB stick and tells you what could go wrong on the
players, whoever made the stick: rekordbox, reks, Lexicon, djay or another
tool. It only reads: nothing on the stick changes. Decision: D38 in
[`decisions.md`](decisions.md).

**In the app:** sidebar → Devices → hover a drive → the clipboard button
(**Check …**). The report replaces the track list; **Check again** after you
fix something, the cross closes it.

**On the command line:**

```sh
reks rb check /Volumes/GIG [--target nxs|nxs2|3000|xdj-xz|generic] [--library <library.sqlite>] [--json]
```

It exits non-zero when the report has problems, so a script can stop on it.

## The result

- **OK**: nothing wrong found (notes may still be listed).
- **Warnings**: something may fail or look wrong on some players.
- **Problems**: something is likely to fail on a player. Fix it or export
  the stick again before the gig.

Each finding says what it means and lists the tracks, playlists or files
concerned.

## What it checks

| Section | What |
|---|---|
| Library files | Which of `export.pdb` (Device Library), `exportExt.pdb` and `exportLibrary.db` (OneLibrary) are on the stick, and which players read each (below). A stick with only one of them shows nothing on the other players. A damaged `export.pdb` (missing tables, rows that don't decode) or an `exportLibrary.db` that fails its integrity check is a problem. |
| File system | FAT32 works on every player. exFAT is a warning unless the stick is for a CDJ-3000 or XDJ-XZ; HFS+ is a warning; anything else (NTFS, APFS) is a problem. Free space is shown. |
| Playlists | Playlists in one library but not the other, or with other tracks or another order. |
| Track files | Audio file missing or unreadable; paths that leave the stick or that FAT32 or players may not handle; analysis (`ANLZ0000.DAT`) missing, damaged or naming another file; no waveform preview; no `.EXT`; no `.2EX` (a note: rekordbox before 6 never wrote one). |
| Audio formats | What the file headers say the players may refuse: `WAVE_FORMAT_EXTENSIBLE` or float WAV, bit depth, sample rate, AIFF-C, VBR MP3 without a seek header, FLAC/ALAC, files over 4 GB. |
| Play history | Sessions players logged on the stick that your library has not imported yet (import them from Histories → +). |

**Which players:** audio is checked against the stick profile's oldest
player (Export to USB → Options → "Oldest player the stick must work on").
For a stick without a profile,
or on the command line without `--target`, it is checked against the
CDJ-2000NXS, the strictest: FLAC and ALAC are problems there.

**Player list** (AlphaTheta notice of 2026-03-18, rekordbox FAQ;
**UNVERIFIED**: reks has not tested these players):

| File | Read by |
|---|---|
| `export.pdb` | CDJ-3000, CDJ-2000NXS2 and older CDJ-2000 models, CDJ-900NXS2, XDJ-XZ, XDJ-RX3, XDJ-RR, XDJ-1000MK2, XDJ-700 |
| `exportLibrary.db` | CDJ-3000X, CDJ-1500X, OPUS-QUAD, OMNIS-DUO, XDJ-AZ, XDJ-AN |
| `exportExt.pdb` | written by rekordbox beside `export.pdb`; the notice names no player that needs it |

## Limits

- A clean report is not a guarantee: no check here has been confirmed on a
  player yet. Test a new stick on a player when you can.
- `exportLibrary.db` is read with reks's built-in OneLibrary key (D14).
  Without it (a build from source with no key), the report says it was not
  read and checks only `export.pdb`.
- Every track's audio header is read, so a large stick takes a while.
