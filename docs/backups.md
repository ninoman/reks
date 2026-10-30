# Library backups

Why and how: [D36](decisions.md#d36-automatic-library-backups-and-restore).

Cues, beatgrids, tags, playlists and play history live only in reks's library
(`reks.sqlite`). reks keeps copies of it automatically.

- **When:** at start-up (at most once a day), before a library update changes
  the database, before a rekordbox XML import, a conversion that replaces your
  files, a play history import, an edit of many tracks at once, adopting a
  stick's playlists, relocating missing files, and reading or writing My Tags
  in comments, when you press **Back up now**, and before a restore. The
  `reks` command line tool backs up before the same operations.
- **Where:** the `library-backups` folder next to the library, in the app's data
  folder (Settings → Library → Backups → **Show in folder**). Each backup is one
  `.sqlite` file named with its UTC time and reason. It is on the same disk as
  the library, so also copy the folder somewhere else now and then.
- **How many:** the newest 10 (change under Settings → Library → Backups, 1 to
  100), plus the newest of each of the last 8 weeks. Older ones are deleted.
- **Checked:** each backup is verified when it is made. If one fails, reks
  shows a warning (a message at start-up and in Settings) and keeps working.
- **Restore:** Settings → Library → Backups lists each backup with its date,
  reason, tracks, playlists and size. **Restore…** saves your current library as
  a "Before a restore" backup first, replaces it, and restarts reks. Restoring
  gives back the library as it was then: changes made since are gone (restore
  the "Before a restore" backup to undo). Damaged backups and ones made by a
  newer reks are listed but cannot be restored. A restore waits while a scan,
  import, export or conversion is running.
- Music files, artwork and analysis caches are not in a backup: they are
  rebuilt from your files, or are the files themselves.
