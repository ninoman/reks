//! The file converter's library half (docs/phases/phase-2.6.md, task 5):
//! [`plan`] decides what happens to each selected track (a dry run, nothing
//! touched), [`apply`] converts and updates the library.
//!
//! The audio half is [`reks_audio::convert()`]: it writes and verifies one temp
//! file. Per file, `apply` then
//! 1. converts the source into `<dest>.reks-tmp` next to the destination
//!    (written, fsynced and read back by `reks_audio`),
//! 2. checks the frame count and, when the samples should be unchanged, the
//!    PCM hash against the source,
//! 3. renames the temp file to `dest`, never over an existing file (a taken
//!    name gets ` (2)`, ` (3)`, …),
//! 4. updates the library in one transaction: Replace points the track at
//!    the new file (cues and grids are in ms, so they stay, as do playlists
//!    and play history) and records the original in `superseded_file`, so the
//!    folder scan doesn't add it back; Add as new inserts a copy of the track
//!    with its cues and grid,
//! 5. moves the original to the OS Trash, only if asked.
//!
//! Cancel or failure at any step removes what this file created and leaves
//! the track as it was. Originals are only ever read, except for step 5.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rusqlite::{named_params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::Library;
use crate::error::{Error, Result};
use crate::fileinfo::{self, library_path, probe_cols, ProbeCols};
use crate::scan::TrackId;
use crate::timefmt;

pub use reks_audio::{CancelToken, ConvertSpec, Depth, Rate, Target};

/// Suffix of the file `apply` writes before the final rename.
pub const TEMP_SUFFIX: &str = ".reks-tmp";

/// Where converted files go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// The original's folder.
    NextToOriginal,
    /// One folder for all files (created if needed).
    Folder(PathBuf),
}

/// What the converted file does in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryMode {
    /// The track points at the new file; everything attached to it stays.
    Replace,
    /// A new track with the same metadata, cues and grid.
    AddAsNew,
}

/// What happens to the original file after a successful conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Originals {
    Keep,
    /// Move to the OS Trash (Replace only). Never a permanent delete.
    Trash,
}

/// The settings beside [`ConvertSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertOptions {
    pub destination: Destination,
    pub library: LibraryMode,
    pub originals: Originals,
    /// With [`Rate::Keep`], sources above 48 kHz go to 44.1 kHz (multiples of
    /// 44.1) or 48 kHz. The Older players preset sets it.
    pub cap_rate_48k: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            destination: Destination::NextToOriginal,
            library: LibraryMode::Replace,
            originals: Originals::Keep,
            cap_rate_48k: false,
        }
    }
}

/// The two presets of the converter dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// AIFF, depth kept (max 24), rate capped at 48 kHz.
    OlderPlayers,
    /// FLAC, depth and rate kept.
    SaveSpace,
}

impl Preset {
    pub fn spec(self) -> ConvertSpec {
        let target = match self {
            Preset::OlderPlayers => Target::Aiff,
            Preset::SaveSpace => Target::Flac,
        };
        ConvertSpec {
            target,
            depth: Depth::Keep,
            rate: Rate::Keep,
        }
    }

    /// Default options with this preset's rate cap.
    pub fn options(self) -> ConvertOptions {
        ConvertOptions {
            cap_rate_48k: self == Preset::OlderPlayers,
            ..ConvertOptions::default()
        }
    }
}

/// An audio file's format, as the plan shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFormat {
    /// `track.kind` string (`flac`, `wav`, `aiff`, `alac`, …).
    pub kind: &'static str,
    pub sample_rate: u32,
    pub bit_depth: u8,
    pub channels: u8,
}

/// One file the plan converts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedConversion {
    pub track_id: TrackId,
    /// The original (library path).
    pub source: String,
    /// The final name as planned; `apply` takes the next free name if it is
    /// taken by then.
    pub dest: String,
    /// The spec for this file, with the rate resolved (never `Keep` when the
    /// rate changes).
    pub spec: ConvertSpec,
    pub from: AudioFormat,
    pub to: AudioFormat,
    pub bytes_before: u64,
    pub bytes_after_est: u64,
}

/// Why a selected track is not converted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SkipReason {
    /// No track with this id.
    NotInLibrary,
    /// Selected more than once.
    Duplicate,
    /// The file is not where the library says.
    FileMissing,
    /// The file could not be read as audio.
    Unreadable { message: String },
    /// MP3 or AAC (D25).
    Lossy,
    /// More than 2 channels.
    TooManyChannels { channels: u8 },
    /// The file is already in the target format, rate and depth.
    AlreadyTarget,
}

/// Something the dry run should show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Warning {
    /// The natural name was taken; the file gets `dest`.
    Renamed {
        track_id: TrackId,
        wanted: String,
        dest: String,
    },
    /// Going to 16 bits (TPDF dither): precision is lost.
    ReducedDepth { track_id: TrackId, from: u8 },
    /// Imported from rekordbox, whose analysis files name the old path.
    /// Until CONV1 is checked, exports use generated analysis for it.
    RekordboxAnalysisDropped { track_id: TrackId },
}

/// The dry run: what [`apply`] would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertPlan {
    pub items: Vec<PlannedConversion>,
    pub skipped: Vec<(TrackId, SkipReason)>,
    pub bytes_before: u64,
    /// Exact for WAV and AIFF (give or take headers and tags), a guess for FLAC.
    pub bytes_after_est: u64,
    pub warnings: Vec<Warning>,
    pub options: ConvertOptions,
}

/// Progress from [`apply`] (sent to the UI over a Tauri channel).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ConvertEvent {
    /// Item `index` (0-based) of `total` started.
    Started {
        track_id: TrackId,
        index: usize,
        total: usize,
    },
    /// `fraction` (0..=1) of the current file done.
    Progress {
        track_id: TrackId,
        fraction: f32,
    },
    Converted {
        track_id: TrackId,
        new_track_id: TrackId,
        dest: String,
    },
    Failed {
        track_id: TrackId,
        message: String,
    },
}

/// What became of an original after its conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OriginalFate {
    Kept,
    Trashed,
    /// Asked for the Trash, but the move failed; the file is still there.
    TrashFailed {
        message: String,
    },
}

/// One finished conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Converted {
    pub track_id: TrackId,
    /// Same as `track_id` on Replace.
    pub new_track_id: TrackId,
    pub source: String,
    pub dest: String,
    pub bytes_after: u64,
    pub original: OriginalFate,
}

/// One conversion that failed; its track is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failed {
    pub track_id: TrackId,
    pub message: String,
}

/// The outcome of [`apply`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertReport {
    pub converted: Vec<Converted>,
    pub failed: Vec<Failed>,
    /// From the plan.
    pub skipped: Vec<(TrackId, SkipReason)>,
    /// Items after a cancel were not started.
    pub cancelled: bool,
    /// Originals of the converted files.
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Plans converting `tracks`. Reads file headers; writes nothing.
pub fn plan(
    lib: &Library,
    tracks: &[TrackId],
    spec: &ConvertSpec,
    opts: &ConvertOptions,
) -> Result<ConvertPlan> {
    if opts.originals == Originals::Trash && opts.library != LibraryMode::Replace {
        return Err(Error::InvalidField {
            field: "originals",
            reason: "moving originals to the Trash needs Replace in library".into(),
        });
    }
    let folder = match &opts.destination {
        Destination::NextToOriginal => None,
        Destination::Folder(dir) => {
            let dir = std::path::absolute(dir).map_err(|source| Error::Io {
                path: dir.clone(),
                source,
            })?;
            if library_path(&dir).is_none() {
                return Err(Error::InvalidField {
                    field: "destination",
                    reason: "folder name is not valid UTF-8".into(),
                });
            }
            Some(dir)
        }
    };

    let mut out = ConvertPlan {
        items: Vec::new(),
        skipped: Vec::new(),
        bytes_before: 0,
        bytes_after_est: 0,
        warnings: Vec::new(),
        options: opts.clone(),
    };
    let mut seen = HashSet::new();
    let mut reserved: HashSet<String> = HashSet::new();
    for &id in tracks {
        if !seen.insert(id) {
            out.skipped.push((id, SkipReason::Duplicate));
            continue;
        }
        let row = {
            let conn = lib.conn()?;
            source_row(&conn, id)?
        };
        let Some(row) = row else {
            out.skipped.push((id, SkipReason::NotInLibrary));
            continue;
        };
        let item = match plan_one(&row, spec, opts) {
            Ok(item) => item,
            Err(reason) => {
                out.skipped.push((id, reason));
                continue;
            }
        };
        let (from, to, resolved, size) = item;

        let src = Path::new(&row.path);
        let dir = match &folder {
            Some(d) => d.clone(),
            None => src.parent().map(Path::to_path_buf).unwrap_or_default(),
        };
        let stem = src
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("track")
            .to_owned();
        let ext = extension(spec.target);
        let wanted = dest_name(&dir, &stem, 1, ext);
        let dest = {
            let conn = lib.conn()?;
            next_free(&conn, &dir, &stem, ext, &reserved)?
        };
        if dest != wanted {
            out.warnings.push(Warning::Renamed {
                track_id: id,
                wanted,
                dest: dest.clone(),
            });
        }
        reserved.insert(dest.clone());
        if to.bit_depth < from.bit_depth && to.bit_depth == 16 {
            out.warnings.push(Warning::ReducedDepth {
                track_id: id,
                from: from.bit_depth,
            });
        }
        if opts.library == LibraryMode::Replace && row.rb_location.is_some() {
            out.warnings
                .push(Warning::RekordboxAnalysisDropped { track_id: id });
        }
        let after = estimate_size(spec.target, row.duration_ms, &to);
        out.bytes_before += size;
        out.bytes_after_est += after;
        out.items.push(PlannedConversion {
            track_id: id,
            source: row.path,
            dest,
            spec: resolved,
            from,
            to,
            bytes_before: size,
            bytes_after_est: after,
        });
    }
    Ok(out)
}

/// Converts the plan's items in order. One item's failure is reported and
/// the rest carry on; a cancel stops after cleaning up the current file.
/// `Err` only if the library itself cannot be reached.
pub fn apply(
    lib: &Library,
    plan: &ConvertPlan,
    events: &dyn Fn(ConvertEvent),
    cancel: &CancelToken,
) -> Result<ConvertReport> {
    let mut report = ConvertReport {
        skipped: plan.skipped.clone(),
        ..ConvertReport::default()
    };
    let total = plan.items.len();
    for (index, item) in plan.items.iter().enumerate() {
        if cancel.is_cancelled() {
            report.cancelled = true;
            break;
        }
        events(ConvertEvent::Started {
            track_id: item.track_id,
            index,
            total,
        });
        match convert_one(lib, plan, item, events, cancel) {
            Ok(done) => {
                events(ConvertEvent::Converted {
                    track_id: done.track_id,
                    new_track_id: done.new_track_id,
                    dest: done.dest.clone(),
                });
                report.bytes_before += item.bytes_before;
                report.bytes_after += done.bytes_after;
                report.converted.push(done);
            }
            Err(Error::Audio(reks_audio::Error::Cancelled { .. })) => {
                report.cancelled = true;
                break;
            }
            Err(Error::LockPoisoned) => return Err(Error::LockPoisoned),
            Err(e) => {
                let message = e.to_string();
                tracing::warn!(track = item.track_id, %message, "conversion failed");
                events(ConvertEvent::Failed {
                    track_id: item.track_id,
                    message: message.clone(),
                });
                report.failed.push(Failed {
                    track_id: item.track_id,
                    message,
                });
            }
        }
    }
    Ok(report)
}

/// The track columns `plan` needs.
struct SourceRow {
    path: String,
    missing: bool,
    duration_ms: Option<i64>,
    rb_location: Option<String>,
}

fn source_row(conn: &Connection, id: TrackId) -> Result<Option<SourceRow>> {
    Ok(conn
        .query_row(
            "SELECT path, missing, duration_ms, rb_location FROM track WHERE id = ?1",
            [id],
            |r| {
                Ok(SourceRow {
                    path: r.get(0)?,
                    missing: r.get(1)?,
                    duration_ms: r.get(2)?,
                    rb_location: r.get(3)?,
                })
            },
        )
        .optional()?)
}

/// Formats, resolved spec and source size for one track, or why it is skipped.
fn plan_one(
    row: &SourceRow,
    spec: &ConvertSpec,
    opts: &ConvertOptions,
) -> std::result::Result<(AudioFormat, AudioFormat, ConvertSpec, u64), SkipReason> {
    let path = Path::new(&row.path);
    let size = match std::fs::metadata(path) {
        Ok(m) if m.is_file() => m.len(),
        _ => return Err(SkipReason::FileMissing),
    };
    if row.missing {
        tracing::debug!(
            path = row.path,
            "track flagged missing, but the file is there"
        );
    }
    let probe = reks_audio::probe(path).map_err(|e| SkipReason::Unreadable {
        message: e.to_string(),
    })?;
    if !probe.kind.is_lossless() {
        return Err(SkipReason::Lossy);
    }
    if probe.channels > 2 {
        return Err(SkipReason::TooManyChannels {
            channels: probe.channels,
        });
    }
    let src_depth = probe.bit_depth.unwrap_or(24);
    let from = AudioFormat {
        kind: probe.kind.as_str(),
        sample_rate: probe.sample_rate,
        bit_depth: src_depth,
        channels: probe.channels,
    };
    let rate = match spec.rate {
        Rate::Hz44100 => 44_100,
        Rate::Hz48000 => 48_000,
        Rate::Keep if opts.cap_rate_48k && probe.sample_rate > 48_000 => {
            if probe.sample_rate % 44_100 == 0 {
                44_100
            } else {
                48_000
            }
        }
        Rate::Keep => probe.sample_rate,
    };
    let depth = match spec.depth {
        Depth::Keep if src_depth <= 16 => 16,
        Depth::Keep | Depth::Bits24 => 24,
        Depth::Bits16 => 16,
    };
    let to = AudioFormat {
        kind: target_kind(spec.target),
        sample_rate: rate,
        bit_depth: depth,
        channels: probe.channels,
    };
    if to.kind == from.kind && rate == from.sample_rate && depth == from.bit_depth {
        let plain = plain_pcm_header(path, probe.kind).map_err(|e| SkipReason::Unreadable {
            message: e.to_string(),
        })?;
        if plain {
            return Err(SkipReason::AlreadyTarget);
        }
    }
    let resolved = ConvertSpec {
        target: spec.target,
        depth: spec.depth,
        rate: match rate {
            r if r == probe.sample_rate => Rate::Keep,
            44_100 => Rate::Hz44100,
            _ => Rate::Hz48000,
        },
    };
    Ok((from, to, resolved, size))
}

/// False for a WAV that is float or `WAVE_FORMAT_EXTENSIBLE` and for AIFF-C:
/// the same kind, rate and depth still need converting for older players
/// (R§4). FLAC and ALAC have no such variants.
fn plain_pcm_header(path: &Path, kind: reks_audio::Kind) -> io::Result<bool> {
    let mut f = File::open(path)?;
    let mut head = [0u8; 12];
    f.read_exact(&mut head)?;
    match kind {
        reks_audio::Kind::Wav => {
            // Chunks after "RIFF" size "WAVE": find "fmt " and its format tag.
            let mut pos = 12u64;
            loop {
                let mut hdr = [0u8; 8];
                f.seek(SeekFrom::Start(pos))?;
                f.read_exact(&mut hdr)?;
                let len = u64::from(u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]));
                if &hdr[..4] == b"fmt " {
                    let mut tag = [0u8; 2];
                    f.read_exact(&mut tag)?;
                    return Ok(u16::from_le_bytes(tag) == 1);
                }
                pos += 8 + len + (len & 1);
            }
        }
        reks_audio::Kind::Aiff => Ok(&head[8..12] == b"AIFF"),
        _ => Ok(true),
    }
}

fn target_kind(target: Target) -> &'static str {
    match target {
        Target::Wav => "wav",
        Target::Aiff => "aiff",
        Target::Flac => "flac",
    }
}

fn extension(target: Target) -> &'static str {
    target_kind(target)
}

/// PCM size for WAV and AIFF; FLAC typically lands near 60 % of it.
fn estimate_size(target: Target, duration_ms: Option<i64>, to: &AudioFormat) -> u64 {
    let ms = u64::try_from(duration_ms.unwrap_or(0)).unwrap_or(0);
    let frames = ms * u64::from(to.sample_rate) / 1000;
    let pcm = frames * u64::from(to.channels) * u64::from(to.bit_depth / 8);
    match target {
        Target::Wav | Target::Aiff => pcm + 64,
        Target::Flac => pcm * 3 / 5,
    }
}

/// `<dir>/<stem>.<ext>`, or `<stem> (n).<ext>` for n > 1.
fn dest_name(dir: &Path, stem: &str, n: u32, ext: &str) -> String {
    let name = if n == 1 {
        format!("{stem}.{ext}")
    } else {
        format!("{stem} ({n}).{ext}")
    };
    dir.join(name).to_string_lossy().into_owned()
}

fn temp_path(dest: &str) -> PathBuf {
    PathBuf::from(format!("{dest}{TEMP_SUFFIX}"))
}

/// The first of `<stem>.<ext>`, `<stem> (2).<ext>`, … that no file, temp
/// file, track row or other planned item uses.
fn next_free(
    conn: &Connection,
    dir: &Path,
    stem: &str,
    ext: &str,
    reserved: &HashSet<String>,
) -> Result<String> {
    let mut n = 1;
    loop {
        let candidate = dest_name(dir, stem, n, ext);
        let p = Path::new(&candidate);
        let in_library: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM track WHERE path = ?1)",
            [&candidate],
            |r| r.get(0),
        )?;
        if !p.exists()
            && !temp_path(&candidate).exists()
            && !in_library
            && !reserved.contains(&candidate)
        {
            return Ok(candidate);
        }
        n += 1;
    }
}

/// Removes a file this module created unless disarmed.
struct Created(Option<PathBuf>);

impl Created {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for Created {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            if let Err(e) = std::fs::remove_file(&p) {
                tracing::warn!(path = %p.display(), error = %e, "could not remove converter file");
            }
        }
    }
}

fn convert_err(path: &str, reason: impl Into<String>) -> Error {
    Error::Convert {
        path: path.to_owned(),
        reason: reason.into(),
    }
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn convert_one(
    lib: &Library,
    plan: &ConvertPlan,
    item: &PlannedConversion,
    events: &dyn Fn(ConvertEvent),
    cancel: &CancelToken,
) -> Result<Converted> {
    let id = item.track_id;
    let src = Path::new(&item.source);
    let current: Option<String> = lib
        .conn()?
        .query_row("SELECT path FROM track WHERE id = ?1", [id], |r| r.get(0))
        .optional()?;
    if current.as_deref() != Some(item.source.as_str()) {
        return Err(convert_err(
            &item.source,
            "the track changed since the plan",
        ));
    }
    let size = std::fs::metadata(src).map_err(io_err(src))?.len();
    if size != item.bytes_before {
        return Err(convert_err(&item.source, "the file changed since the plan"));
    }

    // Same folder and naming as planned; the next free name if it was taken since.
    let planned = Path::new(&item.dest);
    let dir = planned.parent().unwrap_or(Path::new("")).to_path_buf();
    crate::db::create_dir(&dir)?;
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("track")
        .to_owned();
    let dest = next_free(
        &*lib.conn()?,
        &dir,
        &stem,
        extension(item.spec.target),
        &HashSet::new(),
    )?;
    let tmp = temp_path(&dest);

    let progress = |fraction: f32| {
        events(ConvertEvent::Progress {
            track_id: id,
            fraction,
        })
    };
    // `convert` removes `tmp` itself on any error or cancel.
    let out = reks_audio::convert(src, &tmp, &item.spec, &progress, cancel)?;
    let mut created = Created(Some(tmp.clone()));

    verify(src, item, &out)?;
    if cancel.is_cancelled() {
        return Err(reks_audio::Error::Cancelled {
            path: src.to_path_buf(),
        }
        .into());
    }

    rename_no_clobber(&tmp, Path::new(&dest))?;
    created.0 = Some(PathBuf::from(&dest));

    let dest_path = Path::new(&dest);
    let (new_size, mtime) = fileinfo::stat(&dest).map_err(io_err(dest_path))?;
    let hash = fileinfo::content_hash(dest_path).map_err(io_err(dest_path))?;
    let probe = reks_audio::probe(dest_path)?;
    let ProbeCols {
        kind,
        duration_ms,
        sample_rate,
        bit_depth,
        bit_rate_kbps,
        channels,
    } = probe_cols(&probe);
    let now = timefmt::now_utc();

    if plan.options.library == LibraryMode::Replace {
        lib.drop_overview(id)?;
    }
    let new_id = {
        let mut conn = lib.conn()?;
        let tx = conn.transaction()?;
        let new_id = match plan.options.library {
            LibraryMode::Replace => {
                tx.execute(
                    "UPDATE track SET
                       path = :path, kind = :kind, file_size = :size, file_mtime = :mtime,
                       content_hash = :hash, sample_rate = :rate, bit_depth = :depth,
                       bit_rate_kbps = :kbps, missing = 0, updated_at = :now
                     WHERE id = :id",
                    named_params! {
                        ":id": id, ":path": dest, ":kind": kind, ":size": new_size,
                        ":mtime": mtime, ":hash": hash, ":rate": sample_rate,
                        ":depth": bit_depth, ":kbps": bit_rate_kbps, ":now": now,
                    },
                )?;
                tx.execute(
                    "INSERT OR REPLACE INTO superseded_file (old_path, new_track_id, converted_at)
                     VALUES (?1, ?2, ?3)",
                    (&item.source, id, &now),
                )?;
                id
            }
            LibraryMode::AddAsNew => {
                tx.execute(
                    "INSERT INTO track (
                       path, file_size, file_mtime, content_hash, kind, duration_ms,
                       sample_rate, bit_depth, bit_rate_kbps, channels,
                       title, artist, album, album_artist, genre, composer, remixer, label,
                       mix, grouping, track_no, disc_no, year, comment, rating, color,
                       key, key_source, bpm_x100, bpm_source, artwork_id,
                       date_added, created_at, updated_at)
                     SELECT :path, :size, :mtime, :hash, :kind, :duration_ms,
                       :rate, :depth, :kbps, :channels,
                       title, artist, album, album_artist, genre, composer, remixer, label,
                       mix, grouping, track_no, disc_no, year, comment, rating, color,
                       key, key_source, bpm_x100, bpm_source, artwork_id,
                       :today, :now, :now
                     FROM track WHERE id = :id",
                    named_params! {
                        ":id": id, ":path": dest, ":size": new_size, ":mtime": mtime,
                        ":hash": hash, ":kind": kind, ":duration_ms": duration_ms,
                        ":rate": sample_rate, ":depth": bit_depth, ":kbps": bit_rate_kbps,
                        ":channels": channels, ":today": timefmt::today_utc(), ":now": now,
                    },
                )?;
                let new_id = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO cue (track_id, slot, kind, start_ms, end_ms, name, color_rgb, source)
                     SELECT ?2, slot, kind, start_ms, end_ms, name, color_rgb, source
                     FROM cue WHERE track_id = ?1 ORDER BY id",
                    (id, new_id),
                )?;
                tx.execute(
                    "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, meter_num, meter_den,
                                           beat_in_bar, source)
                     SELECT ?2, position_ms, bpm_x100, meter_num, meter_den, beat_in_bar, source
                     FROM beatgrid WHERE track_id = ?1 ORDER BY id",
                    (id, new_id),
                )?;
                new_id
            }
        };
        // The new file's path is a track again, not a skipped original.
        tx.execute("DELETE FROM superseded_file WHERE old_path = ?1", [&dest])?;
        tx.commit()?;
        new_id
    };
    created.disarm();

    let original = match plan.options.originals {
        Originals::Keep => OriginalFate::Kept,
        Originals::Trash => match move_to_trash(src) {
            Ok(()) => OriginalFate::Trashed,
            Err(e) => {
                tracing::warn!(path = item.source, error = %e, "original not moved to the Trash");
                OriginalFate::TrashFailed {
                    message: e.to_string(),
                }
            }
        },
    };
    tracing::info!(track = id, new_track = new_id, dest, "converted");
    Ok(Converted {
        track_id: id,
        new_track_id: new_id,
        source: item.source.clone(),
        dest,
        bytes_after: new_size.unsigned_abs(),
        original,
    })
}

/// Checks the written file against the source on top of `convert`'s own
/// read-back: the planned rate and depth, the frame count, and the PCM hash
/// whenever the samples should be unchanged (same rate, same depth, integer
/// source).
fn verify(src: &Path, item: &PlannedConversion, out: &reks_audio::Output) -> Result<()> {
    let fail = |reason: String| convert_err(&item.source, reason);
    if out.sample_rate != item.to.sample_rate || out.bit_depth != item.to.bit_depth {
        return Err(fail(format!(
            "wrote {} Hz / {} bit, planned {} Hz / {} bit",
            out.sample_rate, out.bit_depth, item.to.sample_rate, item.to.bit_depth
        )));
    }
    let native = *reks_audio::NativeStream::open(src)?.format();
    if let Some(frames) = native.frames {
        let expected = if native.sample_rate == out.sample_rate {
            frames
        } else {
            let n = (u128::from(frames) * u128::from(out.sample_rate))
                .div_ceil(u128::from(native.sample_rate));
            u64::try_from(n).unwrap_or(u64::MAX)
        };
        if out.frames != expected {
            return Err(fail(format!(
                "wrote {} frames, expected {expected}",
                out.frames
            )));
        }
    }
    let exact = native.sample_rate == out.sample_rate
        && native.bits_per_sample == out.bit_depth
        && !native.float;
    if exact && reks_audio::pcm_hash(src)? != out.pcm_hash {
        return Err(fail(
            "the converted samples differ from the original".into(),
        ));
    }
    Ok(())
}

/// Renames `tmp` to `dest`, failing if `dest` exists. A hard link refuses an
/// existing name atomically; filesystems without hard links (FAT32, exFAT)
/// fall back to a check, then a rename.
fn rename_no_clobber(tmp: &Path, dest: &Path) -> Result<()> {
    match std::fs::hard_link(tmp, dest) {
        Ok(()) => {
            std::fs::remove_file(tmp).map_err(io_err(tmp))?;
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Err(io_err(dest)(e));
        }
        Err(e) => {
            tracing::debug!(error = %e, "no hard link; renaming");
            if dest.exists() {
                return Err(io_err(dest)(io::Error::from(io::ErrorKind::AlreadyExists)));
            }
            std::fs::rename(tmp, dest).map_err(io_err(dest))?;
        }
    }
    sync_dir(dest);
    Ok(())
}

/// Makes the rename durable (POSIX). Best effort: the file data itself is
/// already fsynced.
fn sync_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
            tracing::debug!(error = %e, "folder not synced");
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn move_to_trash(path: &Path) -> std::result::Result<(), trash::Error> {
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        // NSFileManager: no Finder automation prompt.
        let mut ctx = trash::TrashContext::default();
        ctx.set_delete_method(DeleteMethod::NsFileManager);
        ctx.delete(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        trash::delete(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dest_names_count_up() {
        let dir = Path::new("/music");
        assert_eq!(
            dest_name(dir, "Track", 1, "aiff"),
            Path::new("/music/Track.aiff").to_string_lossy()
        );
        assert_eq!(
            dest_name(dir, "Track", 2, "aiff"),
            Path::new("/music/Track (2).aiff").to_string_lossy()
        );
    }

    #[test]
    fn presets() {
        assert_eq!(Preset::OlderPlayers.spec().target, Target::Aiff);
        assert!(Preset::OlderPlayers.options().cap_rate_48k);
        assert_eq!(Preset::SaveSpace.spec().target, Target::Flac);
        assert!(!Preset::SaveSpace.options().cap_rate_48k);
    }

    #[test]
    fn size_estimate() {
        let to = AudioFormat {
            kind: "aiff",
            sample_rate: 44_100,
            bit_depth: 16,
            channels: 2,
        };
        assert_eq!(estimate_size(Target::Aiff, Some(1000), &to), 176_400 + 64);
        assert_eq!(estimate_size(Target::Flac, Some(1000), &to), 105_840);
    }
}
