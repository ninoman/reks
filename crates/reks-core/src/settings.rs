//! App settings (the `setting` table), library roots (`library_root`) and
//! moving the cache dir.
//!
//! Each setting is one row whose value is JSON. A missing or unreadable value
//! means the default (an unreadable one is logged), so a bad row never stops
//! the app. Keys: [`AUDIO_DEVICE`], [`AUDIO_SAMPLE_RATE`],
//! [`AUDIO_BUFFER_FRAMES`], [`AUDIO_CUE`], [`KEY_NOTATION`], [`DECK_WAVEFORM`], [`THEME`],
//! [`SYNC_ALIGN`], [`MIXER_VIEW`], [`MIXER_FILTERS`], [`CACHE_DIR`], [`UI_STATE`], [`ANLZ_PREFERENCE`].

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::db::create_dir;
use crate::error::{Error, Result};
use crate::fileinfo::library_path;
use crate::keys::KeyNotation;
use crate::Library;

/// Output device name; absent = the system default.
pub const AUDIO_DEVICE: &str = "audio.device";
/// Output sample rate in Hz; absent = 44.1 kHz when offered, else the device's default.
pub const AUDIO_SAMPLE_RATE: &str = "audio.sample_rate";
/// Output buffer in frames; absent = the device's default.
pub const AUDIO_BUFFER_FRAMES: &str = "audio.buffer_frames";
/// [`CueRouting`] for the headphone cue; absent = the device's own default.
pub const AUDIO_CUE: &str = "audio.cue";
/// [`KeyNotation`] for the table, track info and key picker.
pub const KEY_NOTATION: &str = "display.key_notation";
/// [`DeckWaveform`]: how the Mix view's deck waveforms are drawn.
pub const DECK_WAVEFORM: &str = "display.deck_waveform";
/// [`Theme`]: the app's colours.
pub const THEME: &str = "display.theme";
/// [`SyncAlign`]: what a deck's SYNC lines up with the master.
pub const SYNC_ALIGN: &str = "mix.sync_align";
/// [`MixerView`]: which mixer controls the Mix view shows.
pub const MIXER_VIEW: &str = "mix.mixer_view";
/// [`MixerFilters`]: whether the simple mixer view shows the filters.
pub const MIXER_FILTERS: &str = "mix.mixer_filters";
/// The cache dir the user chose; absent = the app's default.
pub const CACHE_DIR: &str = "storage.cache_dir";
/// [`UiState`].
pub const UI_STATE: &str = "ui.state";
/// [`AnlzPreference`]: where USB exports take a track's analysis from.
pub const ANLZ_PREFERENCE: &str = "export.anlz_preference";

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

/// Where the mixer's headphone cue goes, as saved. The same shape as the
/// UI's `CueRouting` and src-tauri's `CueRoutingArg` (which maps it onto
/// `reks_mix::CueRouting`); reks-core does not depend on reks-mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CueRouting {
    /// Master only.
    Off,
    /// Cue in the left ear, master (mono) in the right.
    SplitMono,
    /// 0-based output channels for the master and the cue.
    Outputs { master: [u16; 2], cue: [u16; 2] },
}

/// The app's colour theme (display only; the palettes live in the UI's
/// `tokens.css`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    /// Dark, warm neutrals with an ivory accent (default, maintainer's call,
    /// 2026-10-22).
    #[default]
    Graphite,
    /// Light: ivory canvas, rose accent.
    Paper,
    /// Dark navy, blue accent.
    Midnight,
    /// Dark plum, lavender accent.
    Dusk,
    /// Black, high contrast, grey accent.
    Booth,
}

/// What SYNC lines up with the master besides the tempo (phase 5 task 16;
/// replaces the deck's BAR button).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncAlign {
    /// The beat.
    Beat,
    /// The bar: the downbeats line up too (default, maintainer's call,
    /// 2026-10-22).
    #[default]
    Bar,
}

/// Which mixer controls the Mix view shows (phase 5 task 24; the engine is
/// the same in both views).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MixerView {
    /// Trim, EQ, CUE, faders, meters and master; the filters per
    /// [`MixerFilters`]; no crossfader or assigns (default: nothing audible
    /// changes, since every crossfader assign starts at THRU).
    #[default]
    Simple,
    /// Everything: crossfader, assigns and both filter units.
    Advanced,
}

/// Whether the simple mixer view shows the filter units; ignored in Advanced,
/// which shows everything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MixerFilters {
    /// Both filter units (default).
    #[default]
    Show,
    /// No filter units; the engine gets them off.
    Hide,
}

/// Everything the Settings dialog and app start-up read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub audio_device: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub audio_buffer_frames: Option<u32>,
    /// `None` until the user picks one: each device gets its own default.
    pub audio_cue: Option<CueRouting>,
    pub key_notation: KeyNotation,
    pub deck_waveform: DeckWaveform,
    pub theme: Theme,
    pub sync_align: SyncAlign,
    pub mixer_view: MixerView,
    pub mixer_filters: MixerFilters,
    /// The cache dir in use; changed only by [`Library::move_cache_dir`].
    pub cache_dir: PathBuf,
    pub ui: UiState,
    pub anlz_preference: AnlzPreference,
    /// This build has the OneLibrary key built in ([`crate::secret::builtin`]).
    pub onelibrary_key_builtin: bool,
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
            filter_bar_open: false,
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
    /// `Some(None)` goes back to automatic.
    #[serde(default, deserialize_with = "present")]
    pub audio_sample_rate: Option<Option<u32>>,
    /// `Some(None)` goes back to the device's default.
    #[serde(default, deserialize_with = "present")]
    pub audio_buffer_frames: Option<Option<u32>>,
    pub audio_cue: Option<CueRouting>,
    pub key_notation: Option<KeyNotation>,
    pub deck_waveform: Option<DeckWaveform>,
    pub theme: Option<Theme>,
    pub sync_align: Option<SyncAlign>,
    pub mixer_view: Option<MixerView>,
    pub mixer_filters: Option<MixerFilters>,
    pub ui: Option<UiState>,
    pub anlz_preference: Option<AnlzPreference>,
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
            audio_sample_rate: get(&conn, AUDIO_SAMPLE_RATE)?,
            audio_buffer_frames: get(&conn, AUDIO_BUFFER_FRAMES)?,
            audio_cue: get(&conn, AUDIO_CUE)?,
            key_notation: get(&conn, KEY_NOTATION)?.unwrap_or_default(),
            deck_waveform: get(&conn, DECK_WAVEFORM)?.unwrap_or_default(),
            theme: get(&conn, THEME)?.unwrap_or_default(),
            sync_align: get(&conn, SYNC_ALIGN)?.unwrap_or_default(),
            mixer_view: get(&conn, MIXER_VIEW)?.unwrap_or_default(),
            mixer_filters: get(&conn, MIXER_FILTERS)?.unwrap_or_default(),
            cache_dir: self.cache_dir(),
            ui: get(&conn, UI_STATE)?.unwrap_or_default(),
            anlz_preference: get(&conn, ANLZ_PREFERENCE)?.unwrap_or_default(),
            onelibrary_key_builtin: crate::secret::builtin::present(),
        })
    }

    /// Applies `patch` in one transaction and returns the new settings.
    pub fn update_settings(&self, patch: SettingsPatch) -> Result<Settings> {
        {
            let mut conn = self.conn()?;
            let tx = conn.transaction()?;
            if let Some(device) = patch.audio_device {
                put(&tx, AUDIO_DEVICE, device.as_ref())?;
            }
            if let Some(rate) = patch.audio_sample_rate {
                put(&tx, AUDIO_SAMPLE_RATE, rate.as_ref())?;
            }
            if let Some(frames) = patch.audio_buffer_frames {
                put(&tx, AUDIO_BUFFER_FRAMES, frames.as_ref())?;
            }
            if let Some(cue) = patch.audio_cue {
                put(&tx, AUDIO_CUE, Some(&cue))?;
            }
            if let Some(n) = patch.key_notation {
                put(&tx, KEY_NOTATION, Some(&n))?;
            }
            if let Some(w) = patch.deck_waveform {
                put(&tx, DECK_WAVEFORM, Some(&w))?;
            }
            if let Some(t) = patch.theme {
                put(&tx, THEME, Some(&t))?;
            }
            if let Some(a) = patch.sync_align {
                put(&tx, SYNC_ALIGN, Some(&a))?;
            }
            if let Some(v) = patch.mixer_view {
                put(&tx, MIXER_VIEW, Some(&v))?;
            }
            if let Some(f) = patch.mixer_filters {
                put(&tx, MIXER_FILTERS, Some(&f))?;
            }
            if let Some(mut ui) = patch.ui {
                ui.volume = clamp_volume(ui.volume);
                put(&tx, UI_STATE, Some(&ui))?;
            }
            if let Some(p) = patch.anlz_preference {
                put(&tx, ANLZ_PREFERENCE, Some(&p))?;
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
        assert_eq!(s.audio_sample_rate, None);
        assert_eq!(s.audio_buffer_frames, None);
        assert_eq!(s.audio_cue, None);
        assert_eq!(s.key_notation, KeyNotation::Classical);
        assert_eq!(s.deck_waveform, DeckWaveform::Rgb);
        assert_eq!(s.theme, Theme::Graphite);
        assert_eq!(s.sync_align, SyncAlign::Bar);
        assert_eq!(s.mixer_view, MixerView::Simple);
        assert_eq!(s.mixer_filters, MixerFilters::Show);
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
            audio_sample_rate: Some(Some(48_000)),
            audio_buffer_frames: Some(Some(256)),
            audio_cue: Some(CueRouting::Outputs {
                master: [0, 1],
                cue: [2, 3],
            }),
            key_notation: Some(KeyNotation::Camelot),
            deck_waveform: Some(DeckWaveform::ThreeBand),
            theme: Some(Theme::Paper),
            sync_align: Some(SyncAlign::Beat),
            mixer_view: Some(MixerView::Advanced),
            mixer_filters: Some(MixerFilters::Hide),
            ui: Some(ui.clone()),
            anlz_preference: Some(AnlzPreference::ReksFirst),
        })
        .unwrap();
        let s = lib
            .update_settings(SettingsPatch {
                key_notation: Some(KeyNotation::OpenKey),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.audio_device.as_deref(), Some("Speakers"));
        assert_eq!(s.audio_sample_rate, Some(48_000));
        assert_eq!(s.audio_buffer_frames, Some(256));
        assert_eq!(
            s.audio_cue,
            Some(CueRouting::Outputs {
                master: [0, 1],
                cue: [2, 3]
            })
        );
        assert_eq!(s.key_notation, KeyNotation::OpenKey);
        assert_eq!(s.deck_waveform, DeckWaveform::ThreeBand);
        assert_eq!(s.theme, Theme::Paper);
        assert_eq!(s.sync_align, SyncAlign::Beat);
        assert_eq!(s.mixer_view, MixerView::Advanced);
        assert_eq!(s.mixer_filters, MixerFilters::Hide);
        assert_eq!(s.ui, ui);
        assert_eq!(s.anlz_preference, AnlzPreference::ReksFirst);
        assert_eq!(lib.settings().unwrap(), s);

        let s = lib
            .update_settings(SettingsPatch {
                audio_device: Some(None),
                audio_sample_rate: Some(None),
                audio_buffer_frames: Some(None),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.audio_device, None);
        assert_eq!(s.audio_sample_rate, None);
        assert_eq!(s.audio_buffer_frames, None);
        assert!(s.audio_cue.is_some());
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
        let p: SettingsPatch = serde_json::from_str(r#"{"theme":"booth"}"#).unwrap();
        assert_eq!(p.theme, Some(Theme::Booth));
        let p: SettingsPatch = serde_json::from_str(r#"{"syncAlign":"beat"}"#).unwrap();
        assert_eq!(p.sync_align, Some(SyncAlign::Beat));
        let p: SettingsPatch =
            serde_json::from_str(r#"{"mixerView":"advanced","mixerFilters":"hide"}"#).unwrap();
        assert_eq!(p.mixer_view, Some(MixerView::Advanced));
        assert_eq!(p.mixer_filters, Some(MixerFilters::Hide));
        let p: SettingsPatch =
            serde_json::from_str(r#"{"audioSampleRate":null,"audioBufferFrames":512}"#).unwrap();
        assert_eq!(p.audio_sample_rate, Some(None));
        assert_eq!(p.audio_buffer_frames, Some(Some(512)));
        let p: SettingsPatch =
            serde_json::from_str(r#"{"audioCue":{"type":"outputs","master":[0,1],"cue":[2,3]}}"#)
                .unwrap();
        assert_eq!(
            p.audio_cue,
            Some(CueRouting::Outputs {
                master: [0, 1],
                cue: [2, 3]
            })
        );
        let p: SettingsPatch =
            serde_json::from_str(r#"{"audioCue":{"type":"splitMono"}}"#).unwrap();
        assert_eq!(p.audio_cue, Some(CueRouting::SplitMono));
    }

    #[test]
    fn unreadable_values_fall_back_to_defaults() {
        let (_dir, lib) = open_temp();
        lib.conn()
            .unwrap()
            .execute_batch(
                "INSERT INTO setting VALUES ('display.key_notation', '\"sideways\"');
                 INSERT INTO setting VALUES ('display.deck_waveform', '\"plaid\"');
                 INSERT INTO setting VALUES ('display.theme', '\"solarized\"');
                 INSERT INTO setting VALUES ('ui.state', '{\"volume\": 0.25, \"infoOpen\": 7}');
                 INSERT INTO setting VALUES ('audio.device', 'not json');",
            )
            .unwrap();
        let s = lib.settings().unwrap();
        assert_eq!(s.key_notation, KeyNotation::Classical);
        assert_eq!(s.deck_waveform, DeckWaveform::Rgb);
        assert_eq!(s.theme, Theme::Graphite);
        assert_eq!(s.ui, UiState::default());
        assert_eq!(s.audio_device, None);
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
        assert!(ui.info_open && !ui.filter_bar_open && ui.filters_enabled);
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
