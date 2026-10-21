//! App settings (the `setting` table), library roots (`library_root`) and
//! moving the cache dir.
//!
//! Each setting is one row whose value is JSON. A missing or unreadable value
//! means the default (an unreadable one is logged), so a bad row never stops
//! the app. Keys: [`AUDIO_DEVICE`], [`KEY_NOTATION`], [`DECK_WAVEFORM`], [`CACHE_DIR`],
//! [`UI_STATE`], [`ANLZ_PREFERENCE`], [`ONELIBRARY_KEY`].
//!
//! The OneLibrary key is write-only from the UI's side: [`Settings`] says
//! only whether one is stored, and [`Library::onelibrary_key`] is how the
//! export reads it. Its value is never logged.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::db::create_dir;
use crate::error::{Error, Result};
use crate::fileinfo::library_path;
use crate::keys::KeyNotation;
use crate::secret::OneLibraryKey;
use crate::Library;

/// Output device name; absent = the system default.
pub const AUDIO_DEVICE: &str = "audio.device";
/// [`KeyNotation`] for the table, track info and key picker.
pub const KEY_NOTATION: &str = "display.key_notation";
/// [`DeckWaveform`]: how the Mix view's deck waveforms are drawn.
pub const DECK_WAVEFORM: &str = "display.deck_waveform";
/// The cache dir the user chose; absent = the app's default.
pub const CACHE_DIR: &str = "storage.cache_dir";
/// [`UiState`].
pub const UI_STATE: &str = "ui.state";
/// [`AnlzPreference`]: where USB exports take a track's analysis from.
pub const ANLZ_PREFERENCE: &str = "export.anlz_preference";
/// The OneLibrary passphrase the user pasted (Phase 4 key policy, D14).
pub const ONELIBRARY_KEY: &str = "onelibrary.key";

/// Where a USB export takes a track's ANLZ files (waveforms, beatgrid, cues)
/// from when both rekordbox's local analysis cache and reks's own analysis
/// have the track (Phase 3 task 10). Tracks with neither follow the export's
/// "missing analysis" option. reks-built ANLZ is UNVERIFIED on players until
/// the Phase 3 hardware check (task 14) passes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnlzPreference {
    /// rekordbox's analysis when it has the track, else reks's (default:
    /// rekordbox's files are the ones tested on players). A grid the user
    /// owns (locked or edited) takes reks's first, so the edit is exported.
    #[default]
    RekordboxFirst,
    /// reks's analysis when it has the track, else rekordbox's.
    ReksFirst,
    /// rekordbox's analysis only; reks's is never written (Phase 2 behaviour).
    RekordboxOnly,
}

/// How the decks (the player deck and the Mix view's) draw their waveforms
/// (display only). Every style is drawn from the same analysis columns (low,
/// mid, high, mono), so switching needs no re-analysis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeckWaveform {
    /// Low, mid and high bands overlaid in their own colours (rekordbox's
    /// "3Band").
    ThreeBand,
    /// Mono height, coloured by the band mix (lows red, mids green, highs
    /// blue; rekordbox's "RGB"). The default (maintainer's call, 2026-10-21).
    #[default]
    Rgb,
    /// Mono height in blue, lighter where highs dominate (the classic
    /// Pioneer blue waveform).
    Blue,
}

/// Everything the Settings dialog and app start-up read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub audio_device: Option<String>,
    pub key_notation: KeyNotation,
    pub deck_waveform: DeckWaveform,
    /// The cache dir in use; changed only by [`Library::move_cache_dir`].
    pub cache_dir: PathBuf,
    pub ui: UiState,
    pub anlz_preference: AnlzPreference,
    /// A OneLibrary key is stored (the key itself never leaves Rust).
    pub onelibrary_key_set: bool,
}

/// Library window state kept across launches. Window size and position are
/// Phase 1 task 14.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiState {
    /// Hidden column ids; `None` until the user changes them (UI defaults).
    pub hidden_columns: Option<Vec<String>>,
    pub filter_bar_open: bool,
    pub filters_enabled: bool,
    pub info_open: bool,
    /// Preview volume, 0..=1.
    pub volume: f32,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            hidden_columns: None,
            filter_bar_open: true,
            filters_enabled: true,
            info_open: false,
            volume: 1.0,
        }
    }
}

/// A partial update; absent fields stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    /// `Some(None)` (JSON `null`) goes back to the system default.
    #[serde(default, deserialize_with = "present")]
    pub audio_device: Option<Option<String>>,
    pub key_notation: Option<KeyNotation>,
    pub deck_waveform: Option<DeckWaveform>,
    pub ui: Option<UiState>,
    pub anlz_preference: Option<AnlzPreference>,
    /// `Some(None)` (JSON `null`) removes the stored key.
    #[serde(default, deserialize_with = "present")]
    pub onelibrary_key: Option<Option<OneLibraryKey>>,
}

/// Lets `null` mean "set to none" rather than "absent".
fn present<'de, D, T>(d: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// A watched music folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRoot {
    pub id: i64,
    pub path: String,
    /// UTC ISO-8601 of the last finished scan over it.
    pub last_scan: Option<String>,
}

impl Library {
    /// Current settings, defaults filled in.
    pub fn settings(&self) -> Result<Settings> {
        let conn = self.conn()?;
        Ok(Settings {
            audio_device: get(&conn, AUDIO_DEVICE)?,
            key_notation: get(&conn, KEY_NOTATION)?.unwrap_or_default(),
            deck_waveform: get(&conn, DECK_WAVEFORM)?.unwrap_or_default(),
            cache_dir: self.cache_dir(),
            ui: get(&conn, UI_STATE)?.unwrap_or_default(),
            anlz_preference: get(&conn, ANLZ_PREFERENCE)?.unwrap_or_default(),
            onelibrary_key_set: read_onelibrary_key(&conn)?.is_some(),
        })
    }

    /// The stored OneLibrary key, if any. The export's first key source
    /// (Phase 4 key policy); the caller falls back to `REKS_ONELIBRARY_KEY`.
    pub fn onelibrary_key(&self) -> Result<Option<OneLibraryKey>> {
        let conn = self.conn()?;
        read_onelibrary_key(&conn)
    }

    /// Applies `patch` in one transaction and returns the new settings.
    pub fn update_settings(&self, patch: SettingsPatch) -> Result<Settings> {
        {
            let mut conn = self.conn()?;
            let tx = conn.transaction()?;
            if let Some(device) = patch.audio_device {
                put(&tx, AUDIO_DEVICE, device.as_ref())?;
            }
            if let Some(n) = patch.key_notation {
                put(&tx, KEY_NOTATION, Some(&n))?;
            }
            if let Some(w) = patch.deck_waveform {
                put(&tx, DECK_WAVEFORM, Some(&w))?;
            }
            if let Some(mut ui) = patch.ui {
                ui.volume = clamp_volume(ui.volume);
                put(&tx, UI_STATE, Some(&ui))?;
            }
            if let Some(p) = patch.anlz_preference {
                put(&tx, ANLZ_PREFERENCE, Some(&p))?;
            }
            if let Some(key) = patch.onelibrary_key {
                put(&tx, ONELIBRARY_KEY, key.as_ref().map(OneLibraryKey::expose))?;
            }
            tx.commit()?;
        }
        self.settings()
    }

    /// Switches to the cache dir stored in the settings, if any, without
    /// moving anything (app start-up). Returns the dir in use.
    pub fn use_stored_cache_dir(&self) -> Result<PathBuf> {
        let conn = self.conn()?;
        let stored: Option<PathBuf> = get(&conn, CACHE_DIR)?;
        if let Some(dir) = stored {
            create_dir(&dir)?;
            *self.cache_dir.write().map_err(|_| Error::LockPoisoned)? = dir;
        }
        drop(conn);
        Ok(self.cache_dir())
    }

    /// Moves the cache (artwork, waveforms) to `new` and stores it as the
    /// cache dir. `new` must not exist or be an empty folder, and must not be
    /// inside the current dir or contain it. Files are copied, counted, then
    /// the old dir is removed; a failure before the switch removes the copy
    /// and leaves everything as it was. Artwork rows hold paths relative to
    /// the cache dir, so the database does not change.
    pub fn move_cache_dir(&self, new: &Path) -> Result<PathBuf> {
        let refused = |reason| Error::CacheDirRefused {
            path: new.to_path_buf(),
            reason,
        };
        let new = std::path::absolute(new).map_err(|source| Error::Io {
            path: new.to_path_buf(),
            source,
        })?;
        // Lock order as everywhere else: connection, then cache dir.
        let conn = self.conn()?;
        let mut current = self.cache_dir.write().map_err(|_| Error::LockPoisoned)?;
        let old = current.clone();
        let old_real = old.canonicalize().unwrap_or_else(|_| old.clone());
        let new_real = real_prefix(&new);
        if new_real == old_real {
            return Err(refused("it is already the cache folder"));
        }
        if new_real.starts_with(&old_real) {
            return Err(refused("it is inside the current cache folder"));
        }
        if old_real.starts_with(&new_real) {
            return Err(refused("it contains the current cache folder"));
        }
        let existed = new.exists();
        if existed {
            if !new.is_dir() {
                return Err(refused("it is a file"));
            }
            let mut entries = std::fs::read_dir(&new).map_err(|source| Error::Io {
                path: new.clone(),
                source,
            })?;
            if entries.next().is_some() {
                return Err(refused("the folder is not empty"));
            }
        }

        let copied = copy_tree(&old, &new).and_then(|copied| {
            let found = count_files(&new)?;
            if found == copied {
                Ok(copied)
            } else {
                Err(Error::Io {
                    path: new.clone(),
                    source: std::io::Error::other(format!(
                        "copied {copied} files but found {found}"
                    )),
                })
            }
        });
        let copied = match copied {
            Ok(n) => n,
            Err(e) => {
                remove_copy(&new, existed);
                return Err(e);
            }
        };
        let stored = new.to_string_lossy().into_owned();
        if let Err(e) = put(&conn, CACHE_DIR, Some(&stored)) {
            remove_copy(&new, existed);
            return Err(e);
        }
        *current = new.clone();
        drop(current);
        drop(conn);
        tracing::info!(from = %old.display(), to = %new.display(), files = copied, "cache dir moved");
        if let Err(err) = std::fs::remove_dir_all(&old) {
            tracing::warn!(dir = %old.display(), %err, "old cache dir not removed");
        }
        Ok(new)
    }

    /// Watched music folders, by path.
    pub fn library_roots(&self) -> Result<Vec<LibraryRoot>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT id, path, last_scan FROM library_root ORDER BY path, id")?;
        let roots = stmt
            .query_map([], |r| {
                Ok(LibraryRoot {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    last_scan: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(roots)
    }

    /// Adds a watched folder (absolute, NFC as the scan stores paths). Adding
    /// one that is already there returns it unchanged.
    pub fn add_library_root(&self, path: &Path) -> Result<LibraryRoot> {
        let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let key = library_path(&abs).ok_or_else(|| Error::InvalidField {
            field: "path",
            reason: "folder name is not valid UTF-8".into(),
        })?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO library_root (path) VALUES (?1) ON CONFLICT (path) DO NOTHING",
            [&key],
        )?;
        Ok(conn.query_row(
            "SELECT id, path, last_scan FROM library_root WHERE path = ?1",
            [&key],
            |r| {
                Ok(LibraryRoot {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    last_scan: r.get(2)?,
                })
            },
        )?)
    }

    /// Stops watching a folder. Its tracks stay in the library as they are.
    pub fn remove_library_root(&self, id: i64) -> Result<()> {
        let n = self
            .conn()?
            .execute("DELETE FROM library_root WHERE id = ?1", [id])?;
        if n == 0 {
            return Err(Error::LibraryRootNotFound(id));
        }
        Ok(())
    }
}

fn get<T: DeserializeOwned>(conn: &Connection, key: &str) -> Result<Option<T>> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(raw.and_then(|v| match serde_json::from_str(&v) {
        Ok(t) => Some(t),
        Err(err) => {
            tracing::warn!(key, value = %v, %err, "unreadable setting, using the default");
            None
        }
    }))
}

/// Like [`get`] for [`ONELIBRARY_KEY`], without logging the value.
fn read_onelibrary_key(conn: &Connection) -> Result<Option<OneLibraryKey>> {
    let raw: Option<zeroize::Zeroizing<String>> = conn
        .query_row(
            "SELECT value FROM setting WHERE key = ?1",
            [ONELIBRARY_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .map(zeroize::Zeroizing::new);
    let Some(raw) = raw else { return Ok(None) };
    match serde_json::from_str::<OneLibraryKey>(&raw) {
        Ok(key) => Ok(Some(key)),
        Err(_) => {
            tracing::warn!(
                key = ONELIBRARY_KEY,
                "unreadable setting, treating it as unset"
            );
            Ok(None)
        }
    }
}

/// Stores `value` as JSON; `None` removes the row (back to the default).
fn put<T: Serialize + ?Sized>(conn: &Connection, key: &str, value: Option<&T>) -> Result<()> {
    match value {
        Some(v) => {
            let json = serde_json::to_string(v).map_err(|e| Error::InvalidField {
                field: "setting",
                reason: e.to_string(),
            })?;
            conn.execute(
                "INSERT INTO setting (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                (key, json),
            )?;
        }
        None => {
            conn.execute("DELETE FROM setting WHERE key = ?1", [key])?;
        }
    }
    Ok(())
}

fn clamp_volume(v: f32) -> f32 {
    if v.is_nan() {
        1.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

/// `path` with its longest existing prefix resolved (symlinks, `/tmp` →
/// `/private/tmp`), so a dir that does not exist yet compares correctly.
fn real_prefix(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut p = path;
    loop {
        if let Ok(real) = p.canonicalize() {
            return rest.iter().rev().fold(real, |acc, c| acc.join(c));
        }
        match (p.parent(), p.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                p = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Copies the files under `from` into `to` (created), returning the count.
fn copy_tree(from: &Path, to: &Path) -> Result<usize> {
    create_dir(to)?;
    let mut n = 0;
    for entry in walkdir::WalkDir::new(from).follow_links(false) {
        let entry = entry.map_err(|e| Error::Io {
            path: e.path().unwrap_or(from).to_path_buf(),
            source: e.into(),
        })?;
        let Ok(rel) = entry.path().strip_prefix(from) else {
            continue;
        };
        let dest = to.join(rel);
        if entry.file_type().is_dir() {
            create_dir(&dest)?;
        } else if entry.file_type().is_file() {
            std::fs::copy(entry.path(), &dest).map_err(|source| Error::Io {
                path: dest.clone(),
                source,
            })?;
            n += 1;
        }
    }
    Ok(n)
}

fn count_files(dir: &Path) -> Result<usize> {
    let mut n = 0;
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| Error::Io {
            path: e.path().unwrap_or(dir).to_path_buf(),
            source: e.into(),
        })?;
        if entry.file_type().is_file() {
            n += 1;
        }
    }
    Ok(n)
}

/// Undoes a failed copy: empties `dir`, and removes it if we created it.
fn remove_copy(dir: &Path, existed: bool) {
    let result = if existed {
        std::fs::read_dir(dir).and_then(|entries| {
            entries.flatten().try_for_each(|e| {
                if e.file_type()?.is_dir() {
                    std::fs::remove_dir_all(e.path())
                } else {
                    std::fs::remove_file(e.path())
                }
            })
        })
    } else {
        std::fs::remove_dir_all(dir)
    };
    if let Err(err) = result {
        tracing::warn!(dir = %dir.display(), %err, "partial cache copy not removed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JPEG header up to a 3×2 SOF0 frame (as in artwork.rs tests).
    const JPEG: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x02, 0x00, 0x03, 0x01, 0x01, 0x11, 0x00,
        0xFF, 0xD9,
    ];

    fn open_temp() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let lib =
            Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
        (dir, lib)
    }

    #[test]
    fn defaults_on_an_empty_table() {
        let (dir, lib) = open_temp();
        let s = lib.settings().unwrap();
        assert_eq!(s.audio_device, None);
        assert_eq!(s.key_notation, KeyNotation::Classical);
        assert_eq!(s.deck_waveform, DeckWaveform::Rgb);
        assert_eq!(s.cache_dir, dir.path().join("cache"));
        assert_eq!(s.ui, UiState::default());
        assert_eq!(s.anlz_preference, AnlzPreference::RekordboxFirst);
    }

    #[test]
    fn patch_round_trips_and_leaves_absent_fields() {
        let (_dir, lib) = open_temp();
        let ui = UiState {
            hidden_columns: Some(vec!["album".into()]),
            filter_bar_open: false,
            filters_enabled: false,
            info_open: true,
            volume: 0.5,
        };
        lib.update_settings(SettingsPatch {
            audio_device: Some(Some("Speakers".into())),
            key_notation: Some(KeyNotation::Camelot),
            deck_waveform: Some(DeckWaveform::ThreeBand),
            ui: Some(ui.clone()),
            anlz_preference: Some(AnlzPreference::ReksFirst),
            onelibrary_key: None,
        })
        .unwrap();
        let s = lib
            .update_settings(SettingsPatch {
                key_notation: Some(KeyNotation::OpenKey),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.audio_device.as_deref(), Some("Speakers"));
        assert_eq!(s.key_notation, KeyNotation::OpenKey);
        assert_eq!(s.deck_waveform, DeckWaveform::ThreeBand);
        assert_eq!(s.ui, ui);
        assert_eq!(s.anlz_preference, AnlzPreference::ReksFirst);
        assert_eq!(lib.settings().unwrap(), s);

        let s = lib
            .update_settings(SettingsPatch {
                audio_device: Some(None),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.audio_device, None);
    }

    #[test]
    fn patch_json_null_means_system_default() {
        let p: SettingsPatch = serde_json::from_str(r#"{"audioDevice":null}"#).unwrap();
        assert_eq!(p.audio_device, Some(None));
        let p: SettingsPatch = serde_json::from_str(r#"{"keyNotation":"openKey"}"#).unwrap();
        assert_eq!(p.audio_device, None);
        assert_eq!(p.key_notation, Some(KeyNotation::OpenKey));
        let p: SettingsPatch = serde_json::from_str(r#"{"anlzPreference":"reksFirst"}"#).unwrap();
        assert_eq!(p.anlz_preference, Some(AnlzPreference::ReksFirst));
        let p: SettingsPatch = serde_json::from_str(r#"{"deckWaveform":"threeBand"}"#).unwrap();
        assert_eq!(p.deck_waveform, Some(DeckWaveform::ThreeBand));
        let p: SettingsPatch = serde_json::from_str(r#"{"deckWaveform":"blue"}"#).unwrap();
        assert_eq!(p.deck_waveform, Some(DeckWaveform::Blue));
    }

    #[test]
    fn unreadable_values_fall_back_to_defaults() {
        let (_dir, lib) = open_temp();
        lib.conn()
            .unwrap()
            .execute_batch(
                "INSERT INTO setting VALUES ('display.key_notation', '\"sideways\"');
                 INSERT INTO setting VALUES ('display.deck_waveform', '\"plaid\"');
                 INSERT INTO setting VALUES ('ui.state', '{\"volume\": 0.25, \"infoOpen\": 7}');
                 INSERT INTO setting VALUES ('audio.device', 'not json');",
            )
            .unwrap();
        let s = lib.settings().unwrap();
        assert_eq!(s.key_notation, KeyNotation::Classical);
        assert_eq!(s.deck_waveform, DeckWaveform::Rgb);
        assert_eq!(s.ui, UiState::default());
        assert_eq!(s.audio_device, None);
    }

    #[test]
    fn onelibrary_key_is_stored_but_not_shown() {
        let (_dir, lib) = open_temp();
        assert!(!lib.settings().unwrap().onelibrary_key_set);
        let p: SettingsPatch = serde_json::from_str(r#"{"onelibraryKey":" k3y "}"#).unwrap();
        let s = lib.update_settings(p).unwrap();
        assert!(s.onelibrary_key_set);
        assert!(!format!("{s:?}").contains("k3y"));
        assert!(!serde_json::to_string(&s).unwrap().contains("k3y"));
        assert_eq!(lib.onelibrary_key().unwrap().unwrap().expose(), "k3y");

        let p: SettingsPatch = serde_json::from_str(r#"{"onelibraryKey":null}"#).unwrap();
        assert!(!lib.update_settings(p).unwrap().onelibrary_key_set);
        assert!(lib.onelibrary_key().unwrap().is_none());
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"onelibraryKey":""}"#).is_err());
    }

    #[test]
    fn partial_ui_state_keeps_other_defaults_and_volume_is_clamped() {
        let (_dir, lib) = open_temp();
        lib.conn()
            .unwrap()
            .execute(
                "INSERT INTO setting VALUES ('ui.state', '{\"infoOpen\": true}')",
                [],
            )
            .unwrap();
        let ui = lib.settings().unwrap().ui;
        assert!(ui.info_open && ui.filter_bar_open && ui.filters_enabled);
        let s = lib
            .update_settings(SettingsPatch {
                ui: Some(UiState { volume: 3.0, ..ui }),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.ui.volume, 1.0);
    }

    #[test]
    fn roots_add_dedupe_remove() {
        let (dir, lib) = open_temp();
        let music = dir.path().join("Music");
        let a = lib.add_library_root(&music).unwrap();
        let again = lib.add_library_root(&music).unwrap();
        assert_eq!(a, again);
        let b = lib.add_library_root(&dir.path().join("Crates")).unwrap();
        let roots = lib.library_roots().unwrap();
        assert_eq!(roots, vec![b.clone(), a.clone()]);
        lib.remove_library_root(a.id).unwrap();
        assert_eq!(lib.library_roots().unwrap(), vec![b]);
        assert!(matches!(
            lib.remove_library_root(a.id),
            Err(Error::LibraryRootNotFound(_))
        ));
    }

    #[test]
    fn cache_move_keeps_artwork_and_removes_the_old_dir() {
        let (dir, lib) = open_temp();
        let id = lib.store_artwork(JPEG).unwrap().unwrap();
        let old = lib.cache_dir();
        let new = dir.path().join("elsewhere/cache");
        assert_eq!(lib.move_cache_dir(&new).unwrap(), new);
        assert_eq!(lib.cache_dir(), new);
        assert!(!old.exists());
        assert_eq!(lib.artwork(id).unwrap(), JPEG);
        assert!(lib.artwork_path(id).unwrap().unwrap().starts_with(&new));

        // Stored: a fresh library on the same DB switches on start-up.
        drop(lib);
        let lib = Library::open(&dir.path().join("reks.sqlite"), &old).unwrap();
        assert_eq!(lib.use_stored_cache_dir().unwrap(), new);
        assert_eq!(lib.settings().unwrap().cache_dir, new);
        assert_eq!(lib.artwork(id).unwrap(), JPEG);
    }

    #[test]
    fn cache_move_into_an_empty_existing_folder() {
        let (dir, lib) = open_temp();
        lib.store_artwork(JPEG).unwrap().unwrap();
        let new = dir.path().join("empty");
        std::fs::create_dir(&new).unwrap();
        lib.move_cache_dir(&new).unwrap();
        assert_eq!(count_files(&new).unwrap(), 1);
    }

    #[test]
    fn cache_move_refusals_change_nothing() {
        let (dir, lib) = open_temp();
        lib.store_artwork(JPEG).unwrap().unwrap();
        let old = lib.cache_dir();
        let full = dir.path().join("full");
        std::fs::create_dir(&full).unwrap();
        std::fs::write(full.join("x"), b"x").unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        for target in [
            old.clone(),
            old.join("sub"),
            dir.path().to_path_buf(),
            full,
            file,
        ] {
            let r = lib.move_cache_dir(&target);
            assert!(
                matches!(r, Err(Error::CacheDirRefused { .. })),
                "{target:?}: {r:?}"
            );
        }
        assert_eq!(lib.cache_dir(), old);
        assert_eq!(count_files(&old).unwrap(), 1);
        assert!(lib.use_stored_cache_dir().unwrap() == old);
    }
}
