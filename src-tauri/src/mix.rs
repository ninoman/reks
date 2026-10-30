//! Mixer commands (Phase 5 task 8): start and stop the `reks-mix` engine,
//! load library tracks or a plugged-in stick's tracks onto decks, forward controls, and stream the engine
//! state to the UI on a `Channel` at about 60 Hz, and edit the loaded
//! track's cues (task 17). Decoding and grid conversion live in
//! `reks_core::deck` (`reks_export_rb::browse` for stick tracks), cue rows in
//! `reks_core::cue`, the engine in
//! `reks_mix`. This file only maps between their types and the UI's serde
//! shapes.

use std::fmt::Display;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reks_core::{Beatgrid, CueMark, DeckAudio, DeckBeat, GridEdit, Library, TrackId};
use reks_mix::{DeckSource, DeckState, Engine, EngineConfig, DECKS, PREVIEW_DECK};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, Response};
use tauri::State;

use crate::commands::{blocking, CmdError, CmdResult};
use crate::state::AppState;

/// State snapshots sent to the UI per second (the engine makes about 60).
const FEED_HZ: u64 = 60;

/// The mixer's decks and the preview deck (`PREVIEW_DECK`, last).
const SLOTS: usize = DECKS + 1;

/// The mixer engine, when started, and what is on its decks.
#[derive(Debug, Default)]
pub struct Mix {
    slot: Arc<Mutex<MixSlot>>,
}

#[derive(Debug, Default)]
struct MixSlot {
    engine: Option<Engine>,
    decks: [Option<LoadedDeck>; SLOTS],
    /// Bumped by every load request per deck; a decode that finishes after
    /// a newer request is dropped.
    generation: [u64; SLOTS],
    /// Cleared to stop the state feed of the running engine.
    feed: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone)]
struct LoadedDeck {
    track: DeckTrack,
    /// The engine's source, for where a cue set at the head goes.
    src: Arc<DeckSource>,
    /// The `generation` that loaded it; the UI's handle on this load.
    load: u64,
}

/// Where a deck's track comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeckTrack {
    Library(TrackId),
    /// Played off a plugged-in stick, with the stick's grid and cues; those
    /// are the stick's, so they can't be edited here.
    Stick,
}

impl DeckTrack {
    fn library_id(self) -> Option<TrackId> {
        match self {
            DeckTrack::Library(id) => Some(id),
            DeckTrack::Stick => None,
        }
    }
}

impl MixSlot {
    fn engine(&mut self) -> CmdResult<&mut Engine> {
        self.engine
            .as_mut()
            .ok_or_else(|| CmdError::new("the mixer's audio output is not started".into()))
    }

    fn stop(&mut self) {
        if let Some(feed) = self.feed.take() {
            feed.store(false, Ordering::Relaxed);
        }
        self.engine = None;
        self.decks = Default::default();
    }
}

/// Headphone cue routing, as the UI sends it.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum CueRoutingArg {
    Off,
    SplitMono,
    Outputs { master: [u16; 2], cue: [u16; 2] },
}

impl From<CueRoutingArg> for reks_mix::CueRouting {
    fn from(c: CueRoutingArg) -> Self {
        match c {
            CueRoutingArg::Off => Self::Off,
            CueRoutingArg::SplitMono => Self::SplitMono,
            CueRoutingArg::Outputs { master, cue } => Self::Outputs { master, cue },
        }
    }
}

impl From<reks_mix::CueRouting> for CueRoutingArg {
    fn from(c: reks_mix::CueRouting) -> Self {
        match c {
            reks_mix::CueRouting::Off => Self::Off,
            reks_mix::CueRouting::SplitMono => Self::SplitMono,
            reks_mix::CueRouting::Outputs { master, cue } => Self::Outputs { master, cue },
        }
    }
}

/// How to open the mixer's output.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixConfig {
    /// `None`: the system default.
    pub device: Option<String>,
    /// `None`: 44.1 kHz when the device offers it, else its default.
    pub sample_rate: Option<u32>,
    /// `None`: the device's default buffer.
    pub buffer_frames: Option<u32>,
    pub cue: CueRoutingArg,
}

/// What a device offers, for the start form.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixDeviceOptions {
    pub rates: Vec<u32>,
    pub default_rate: u32,
    /// Output channels the stream opens with.
    pub channels: u16,
    /// The cue routing to preselect for this device.
    pub default_cue: CueRoutingArg,
    /// `[min, max]` frames, when the platform reports them.
    pub buffer_frames: Option<[u32; 2]>,
}

/// One deck as the UI shows it. Times in ms of the track (source time).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeckView {
    pub track_id: Option<TrackId>,
    pub loaded: bool,
    pub playing: bool,
    pub position_ms: f64,
    pub duration_ms: f64,
    pub cue_ms: f64,
    pub tempo: f64,
    pub nudge: f64,
    pub speed: f64,
    /// Tempo heard at the playhead (grid tempo × speed); `None` without a grid.
    pub bpm: Option<f64>,
    /// Beat number at the playhead; `None` without a grid.
    pub beat: Option<f64>,
    /// CUE is held from the cue point (the deck plays until it is let go).
    pub cue_preview: bool,
    pub sync: bool,
    pub sync_bar: bool,
    pub locked: bool,
    pub quantise: bool,
    /// Master tempo (key lock) is on.
    pub master_tempo: bool,
    /// This deck is the sync master.
    pub master: bool,
    /// The active loop.
    pub loop_region: Option<LoopView>,
    /// A pending loop IN, waiting for OUT.
    pub loop_in: Option<f64>,
    /// The loop RELOOP enters: the last one left, or a memory loop called.
    pub last_loop: Option<LoopView>,
}

/// A loop (`reks_mix::LoopRegion`) in milliseconds.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoopView {
    pub start_ms: f64,
    pub end_ms: f64,
    /// Length along the grid (fractional for a manual loop); `None` without
    /// a grid.
    pub beats: Option<f64>,
}

/// A peak meter: `level` linear (1.0 = 0 dBFS) with a falling display;
/// `clip` when a sample reached 0 dBFS since the last state.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MeterView {
    pub level: f32,
    pub clip: bool,
}

impl From<reks_mix::MeterState> for MeterView {
    fn from(m: reks_mix::MeterState) -> Self {
        Self {
            level: m.level,
            clip: m.clip,
        }
    }
}

/// The mixer state streamed to the UI.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MixState {
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: Option<u32>,
    /// The device went away; stop and start again.
    pub device_lost: bool,
    pub xruns: u64,
    /// The sync master deck; `None` when nobody is master.
    pub master_deck: Option<u8>,
    pub decks: Vec<DeckView>,
    /// The preview deck (`PREVIEW_DECK`): the track list's quick play.
    pub preview: DeckView,
    /// Channel meters, after EQ and filter, before the fader.
    pub meters: Vec<MeterView>,
    /// Master meter, before the limiter.
    pub master_meter: MeterView,
}

fn frames_to_ms(frames: i64, rate: u32) -> f64 {
    frames as f64 * 1000.0 / f64::from(rate.max(1))
}

fn deck_view(d: &DeckState, track_id: Option<TrackId>, rate: u32) -> DeckView {
    DeckView {
        track_id,
        loaded: d.loaded,
        playing: d.playing,
        position_ms: frames_to_ms(d.position, rate),
        duration_ms: frames_to_ms(d.frames, rate),
        cue_ms: frames_to_ms(d.cue, rate),
        tempo: d.tempo,
        nudge: d.nudge,
        speed: d.speed,
        bpm: d.bpm,
        beat: d.beat,
        cue_preview: d.cue_preview,
        sync: d.sync,
        sync_bar: d.sync_bar,
        locked: d.locked,
        quantise: d.quantise,
        master_tempo: d.master_tempo,
        master: d.master,
        loop_region: d.loop_region.map(|l| loop_view(l, rate)),
        loop_in: d.loop_in.map(|f| frames_to_ms(f, rate)),
        last_loop: d.last_loop.map(|l| loop_view(l, rate)),
    }
}

fn loop_view(l: reks_mix::LoopRegion, rate: u32) -> LoopView {
    LoopView {
        start_ms: frames_to_ms(l.start, rate),
        end_ms: frames_to_ms(l.end, rate),
        beats: l.beats,
    }
}

fn snapshot(slot: &mut MixSlot) -> Option<MixState> {
    let ids = slot
        .decks
        .each_ref()
        .map(|d| d.as_ref().and_then(|l| l.track.library_id()));
    let engine = slot.engine.as_mut()?;
    let st = engine.poll_state();
    let info = engine.info();
    let rate = info.sample_rate;
    Some(MixState {
        device: info.device.clone(),
        sample_rate: rate,
        channels: info.channels,
        buffer_frames: info.buffer_frames,
        device_lost: engine.device_lost(),
        xruns: engine.xruns(),
        master_deck: st.master_deck,
        decks: st
            .decks
            .iter()
            .zip(ids)
            .map(|(d, id)| deck_view(d, id, rate))
            .collect(),
        preview: deck_view(&st.preview, ids[DECKS], rate),
        meters: st.channels.iter().map(|&m| m.into()).collect(),
        master_meter: st.master.into(),
    })
}

fn lock(mix: &Mix) -> CmdResult<std::sync::MutexGuard<'_, MixSlot>> {
    mix.slot
        .lock()
        .map_err(|_| CmdError::new("mixer state poisoned".into()))
}

/// Rates and buffer sizes `device` (`None`: system default) offers.
#[tauri::command]
pub async fn mix_device_options(device: Option<String>) -> CmdResult<MixDeviceOptions> {
    let o =
        tauri::async_runtime::spawn_blocking(move || reks_mix::device_options(device.as_deref()))
            .await??;
    Ok(MixDeviceOptions {
        rates: o.rates,
        default_rate: o.default_rate,
        channels: o.channels,
        default_cue: reks_mix::default_cue_routing(o.channels).into(),
        buffer_frames: o.buffer_frames.map(|(a, b)| [a, b]),
    })
}

/// Opens the output (replacing a running engine, whose decks are emptied)
/// and streams [`MixState`] to `on_state` until [`mix_stop`].
#[tauri::command]
pub async fn mix_start(
    state: State<'_, AppState>,
    config: MixConfig,
    on_state: Channel<MixState>,
) -> CmdResult<MixState> {
    let slot = Arc::clone(&state.mix.slot);
    let first = tauri::async_runtime::spawn_blocking(move || -> CmdResult<MixState> {
        let mut s = slot.lock().map_err(|_| "mixer state poisoned")?;
        s.stop();
        s.engine = Some(Engine::start(EngineConfig {
            device: config.device,
            sample_rate: config.sample_rate,
            buffer_frames: config.buffer_frames,
            cue: config.cue.into(),
        })?);
        let running = Arc::new(AtomicBool::new(true));
        s.feed = Some(Arc::clone(&running));
        let first = snapshot(&mut s).ok_or("the mixer did not start")?;
        drop(s);
        let feed_slot = Arc::clone(&slot);
        std::thread::Builder::new()
            .name("reks-mix-feed".into())
            .spawn(move || feed(&feed_slot, &running, &on_state))?;
        Ok(first)
    })
    .await??;
    Ok(first)
}

/// Sends a snapshot every 1/[`FEED_HZ`] s while `running`.
fn feed(slot: &Mutex<MixSlot>, running: &AtomicBool, channel: &Channel<MixState>) {
    let period = Duration::from_micros(1_000_000 / FEED_HZ);
    while running.load(Ordering::Relaxed) {
        let next = match slot.lock() {
            Ok(mut s) if running.load(Ordering::Relaxed) => snapshot(&mut s),
            _ => None,
        };
        let Some(st) = next else { break };
        if let Err(err) = channel.send(st) {
            tracing::debug!(%err, "mixer state channel closed");
            break;
        }
        std::thread::sleep(period);
    }
}

/// Stops the output and empties the decks.
#[tauri::command]
pub fn mix_stop(state: State<'_, AppState>) -> CmdResult<()> {
    lock(&state.mix)?.stop();
    Ok(())
}

/// A loaded deck's track details.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckLoaded {
    pub deck: u8,
    /// This load, for `mix_waveform_detail`.
    pub load: u64,
    /// `None` for a stick track.
    pub track_id: Option<TrackId>,
    pub duration_ms: f64,
    /// Hot cues set on the deck.
    pub hot_cues: Vec<HotCueView>,
    /// Memory cues and loops (by their start), by position.
    pub memory_cues: Vec<MemoryCueView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotCueView {
    /// The library cue, to rename or delete it.
    pub id: i64,
    /// 0..7 = A..H.
    pub slot: u8,
    pub position_ms: f64,
    /// 0xRRGGBB from the library, if set.
    pub color_rgb: Option<u32>,
    /// The cue's name, if set.
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCueView {
    /// The library cue, to rename or delete it.
    pub id: i64,
    pub position_ms: f64,
    /// A memory loop's end.
    pub end_ms: Option<f64>,
    /// A memory loop playback enters by itself.
    pub active_loop: bool,
    /// 0xRRGGBB from the library, if set.
    pub color_rgb: Option<u32>,
    pub name: Option<String>,
}

/// A deck's cues after an edit, as `mix_load` returns them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckCues {
    pub hot_cues: Vec<HotCueView>,
    /// By position.
    pub memory_cues: Vec<MemoryCueView>,
}

fn mix_beats(beats: &[DeckBeat]) -> Vec<reks_mix::GridBeat> {
    beats
        .iter()
        .map(|b| reks_mix::GridBeat {
            frame: b.frame,
            bpm: b.bpm,
            beat_in_bar: b.beat_in_bar,
            beats_per_bar: b.beats_per_bar,
        })
        .collect()
}

fn deck_source(audio: DeckAudio) -> DeckSource {
    DeckSource::new(audio.pcm.samples, audio.pcm.sample_rate)
        .with_grid(&mix_beats(&audio.beats))
        .with_hot_cues(audio.hot_cues)
        .with_auto_loops(reks_core::deck::auto_loops(&audio.marks))
}

fn deck_index(deck: u8) -> CmdResult<usize> {
    let i = usize::from(deck);
    if i < DECKS || deck == PREVIEW_DECK {
        Ok(i)
    } else {
        Err(CmdError::new(format!("no deck {}", i + 1)))
    }
}

/// Decodes library track `id` and loads it on `deck` ([`load_deck`]).
#[tauri::command]
pub async fn mix_load(
    state: State<'_, AppState>,
    deck: u8,
    id: TrackId,
) -> CmdResult<Option<DeckLoaded>> {
    load_deck(&state, deck, DeckTrack::Library(id), move |lib, rate| {
        lib.deck_audio(id, rate)
    })
    .await
}

/// Decodes track `track_id` of the stick at `mount_point` and loads it on
/// `deck` ([`load_deck`]), with the grid and cues of the stick's ANLZ files.
#[tauri::command]
pub async fn mix_load_stick(
    state: State<'_, AppState>,
    deck: u8,
    mount_point: PathBuf,
    track_id: u32,
) -> CmdResult<Option<DeckLoaded>> {
    load_deck(&state, deck, DeckTrack::Stick, move |_, rate| {
        reks_export_rb::browse::deck_audio(&mount_point, track_id, rate)
    })
    .await
}

/// Decodes a track with `decode` at the engine rate on a worker thread,
/// builds its deck source there, then loads it on `deck` (stopped at the
/// start) unless a newer load for that deck came in meanwhile. Returns
/// `None` then.
async fn load_deck<E, F>(
    state: &State<'_, AppState>,
    deck: u8,
    track: DeckTrack,
    decode: F,
) -> CmdResult<Option<DeckLoaded>>
where
    E: Display + Send + 'static,
    F: FnOnce(&Library, u32) -> Result<DeckAudio, E> + Send + 'static,
{
    let i = deck_index(deck)?;
    let (generation, rate) = {
        let mut s = lock(&state.mix)?;
        let rate = s.engine()?.info().sample_rate;
        s.generation[i] += 1;
        (s.generation[i], rate)
    };
    let (src, (hot_cues, memory_cues)) = blocking(state, move |lib| {
        let audio = decode(lib, rate)?;
        let views = cue_views(&audio.marks, rate);
        Ok::<_, E>((Arc::new(deck_source(audio)), views))
    })
    .await?;
    let mut s = lock(&state.mix)?;
    // A newer load, or a restart at another rate, won meanwhile.
    if s.generation[i] != generation || s.engine()?.info().sample_rate != rate {
        return Ok(None);
    }
    let duration_ms = frames_to_ms(src.frames(), rate);
    s.engine()?.send(reks_mix::Control::Load {
        deck,
        src: Arc::clone(&src),
    })?;
    s.decks[i] = Some(LoadedDeck {
        track,
        src,
        load: generation,
    });
    Ok(Some(DeckLoaded {
        deck,
        load: generation,
        track_id: track.library_id(),
        duration_ms,
        hot_cues,
        memory_cues,
    }))
}

/// Most points one `mix_waveform_detail` request builds (60 s).
const MAX_DETAIL_POINTS: u64 = 180_000;

/// Close-zoom waveform points `first..first + count` of the track that load
/// `load` put on `deck` (`reks_analysis::detail`), built from the deck's PCM
/// on a worker thread and sent raw (an `ArrayBuffer` in the UI). Empty when
/// the deck holds another load by now.
#[tauri::command]
pub async fn mix_waveform_detail(
    state: State<'_, AppState>,
    deck: u8,
    load: u64,
    first: u64,
    count: u64,
) -> CmdResult<Response> {
    let i = deck_index(deck)?;
    let src = match &lock(&state.mix)?.decks[i] {
        Some(d) if d.load == load => Arc::clone(&d.src),
        _ => return Ok(Response::new(Vec::new())),
    };
    let count = count.min(MAX_DETAIL_POINTS);
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        reks_analysis::detail::stereo_detail(src.pcm(), src.sample_rate(), first, count)
    })
    .await??;
    Ok(Response::new(bytes))
}

/// Splits a deck's cue marks into hot and memory cue views in ms.
fn cue_views(marks: &[CueMark], rate: u32) -> (Vec<HotCueView>, Vec<MemoryCueView>) {
    let mut hot = Vec::new();
    let mut memory = Vec::new();
    for m in marks {
        let position_ms = frames_to_ms(m.frame, rate);
        match m.slot {
            Some(slot) => hot.push(HotCueView {
                id: m.id,
                slot,
                position_ms,
                color_rgb: m.color_rgb,
                name: m.name.clone(),
            }),
            None => memory.push(MemoryCueView {
                id: m.id,
                position_ms,
                end_ms: m.end_frame.map(|e| frames_to_ms(e, rate)),
                active_loop: m.active_loop,
                color_rgb: m.color_rgb,
                name: m.name.clone(),
            }),
        }
    }
    (hot, memory)
}

/// A cue edit on a loaded deck (task 17), as the UI sends it.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum CueEdit {
    /// Hot cue `slot` (empty) at the playhead, on the beat with quantise.
    SetHotCue {
        slot: u8,
    },
    /// A memory cue at the playhead, on the beat with quantise; while a
    /// loop plays, a memory loop of it.
    AddMemoryCue,
    /// `None` or blank clears the name.
    Rename {
        cue_id: i64,
        name: Option<String>,
    },
    /// A memory loop playback enters by itself (or not).
    SetActiveLoop {
        cue_id: i64,
        on: bool,
    },
    Delete {
        cue_id: i64,
    },
}

/// The track's cues as the deck shows them, in ms at the engine rate, and
/// its active memory loops in frames for the engine.
fn deck_cues(
    lib: &Library,
    id: TrackId,
    rate: u32,
) -> reks_core::Result<(DeckCues, Vec<(i64, i64)>)> {
    let marks = reks_core::deck::cue_marks(&lib.track(id)?.cues, rate);
    let (hot_cues, memory_cues) = cue_views(&marks, rate);
    let cues = DeckCues {
        hot_cues,
        memory_cues,
    };
    Ok((cues, reks_core::deck::auto_loops(&marks)))
}

/// Applies `edit` to the track on `deck`: the library first (so the cue
/// reaches the XML and USB exports), then the engine's hot cues. Cues set
/// at the playhead take its position from the latest engine state, so a
/// playing deck's cue can land up to one state (~17 ms) early; with
/// quantise it snaps to the beat either way. Returns the deck's cues.
#[tauri::command]
pub async fn mix_cue_edit(
    state: State<'_, AppState>,
    deck: u8,
    edit: CueEdit,
) -> CmdResult<DeckCues> {
    let i = deck_index(deck)?;
    let (id, generation, rate, at, looping) = {
        let mut s = lock(&state.mix)?;
        let (id, src) = match &s.decks[i] {
            Some(LoadedDeck {
                track: DeckTrack::Library(id),
                src,
                ..
            }) => (*id, Arc::clone(src)),
            Some(_) => {
                return Err(CmdError::new(
                    "The cues of a track on a stick can't be changed here.".into(),
                ))
            }
            None => return Err(CmdError::new(format!("deck {} is empty", i + 1))),
        };
        let generation = s.generation[i];
        let engine = s.engine()?;
        let rate = engine.info().sample_rate;
        let d = engine.poll_state().decks[i];
        let at = src.cue_frame(d.position, d.quantise);
        (id, generation, rate, at, d.loop_region)
    };
    let ms = move |f: i64| frames_to_ms(f, rate);
    let (cues, auto, hot) = blocking(&state, move |lib| {
        let hot = match edit {
            CueEdit::SetHotCue { slot } => {
                lib.set_hot_cue(id, slot, ms(at))?;
                Some((slot, Some(at)))
            }
            CueEdit::AddMemoryCue => {
                lib.save_memory(id, ms(at), looping.map(|l| (ms(l.start), ms(l.end))))?;
                None
            }
            CueEdit::Rename { cue_id, name } => {
                lib.rename_cue(id, cue_id, name.as_deref())?;
                None
            }
            CueEdit::SetActiveLoop { cue_id, on } => {
                lib.set_active_loop(id, cue_id, on)?;
                None
            }
            CueEdit::Delete { cue_id } => {
                let slot = lib.delete_cue(id, cue_id)?;
                slot.map(|slot| (slot, None))
            }
        };
        let (cues, auto) = deck_cues(lib, id, rate)?;
        Ok::<_, reks_core::Error>((cues, auto, hot))
    })
    .await?;
    let mut s = lock(&state.mix)?;
    // The deck still holds this load (not reloaded or ejected meanwhile).
    if s.generation[i] == generation {
        let engine = s.engine()?;
        if let Some((slot, frame)) = hot {
            engine.send(reks_mix::Control::SetHotCue { deck, slot, frame })?;
        }
        for index in 0..reks_mix::AUTO_LOOP_SLOTS {
            engine.send(reks_mix::Control::AutoLoop {
                deck,
                index: index as u8,
                region: auto.get(index).copied(),
            })?;
        }
    }
    Ok(cues)
}

/// One grid edit on the library track on `deck` (the deck's grid mode),
/// snapped first with `snap`; the deck plays on with the new grid. Stored
/// like the editor's (`beatgrid_edit`).
#[tauri::command]
pub async fn mix_grid_edit(
    state: State<'_, AppState>,
    deck: u8,
    mut edit: GridEdit,
    snap: Option<bool>,
) -> CmdResult<Beatgrid> {
    let i = deck_index(deck)?;
    let (id, src, generation, rate) = {
        let mut s = lock(&state.mix)?;
        let (id, src) = match &s.decks[i] {
            Some(LoadedDeck {
                track: DeckTrack::Library(id),
                src,
                ..
            }) => (*id, Arc::clone(src)),
            Some(_) => {
                return Err(CmdError::new(
                    "The grid of a track on a stick can't be changed here.".into(),
                ))
            }
            None => return Err(CmdError::new(format!("deck {} is empty", i + 1))),
        };
        let generation = s.generation[i];
        (id, src, generation, s.engine()?.info().sample_rate)
    };
    let (grid, next) = blocking(&state, move |lib| {
        let grid = crate::grid::apply_edit(lib, id, &mut edit, snap == Some(true))?;
        let frames = usize::try_from(src.frames()).unwrap_or(0);
        let beats = reks_core::deck_beats(&grid.anchors, rate, frames);
        let next = Arc::new(src.with_new_grid(&mix_beats(&beats)));
        Ok::<_, String>((grid, next))
    })
    .await?;
    let mut s = lock(&state.mix)?;
    // The deck still holds this load (not reloaded or ejected meanwhile).
    if s.generation[i] == generation {
        s.engine()?.send(reks_mix::Control::SetGrid {
            deck,
            src: Arc::clone(&next),
        })?;
        if let Some(d) = s.decks[i].as_mut() {
            d.src = next;
        }
    }
    Ok(grid)
}

/// A channel strip control (`reks_mix::ChannelParam`).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelParamArg {
    Trim,
    EqHi,
    EqMid1,
    EqMid2,
    EqLo,
    Eq3Hi,
    Eq3Mid,
    Eq3Lo,
    Fx,
    Fader,
    FilterAssign,
    XfadeAssign,
    Cue,
}

/// A filter unit control (`reks_mix::FilterParam`).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterParamArg {
    Freq,
    Resonance,
    HighPass,
    BandPass,
    LowPass,
    On,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterUnitArg {
    X,
    Y,
}

/// A channel FX effect (`reks_mix::FxKind`).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FxKindArg {
    Filter,
    Echo,
    Reverb,
}

/// A master section control (`reks_mix::MasterParam`).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MasterParamArg {
    Level,
    CueLevel,
    CueMix,
    SplitCue,
    CuePostEq,
}

/// `reks_mix::Control` as the UI sends it. `Load` is [`mix_load`]; `Seek`
/// takes milliseconds, converted at the engine rate.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum MixControl {
    Unload {
        deck: u8,
    },
    Play {
        deck: u8,
    },
    Pause {
        deck: u8,
    },
    Cue {
        deck: u8,
    },
    CueRelease {
        deck: u8,
    },
    Seek {
        deck: u8,
        ms: f64,
        /// Without quantise (grid mode).
        #[serde(default)]
        exact: bool,
    },
    HotCue {
        deck: u8,
        slot: u8,
    },
    /// Memory cue call: cue point and playhead to `ms`, exactly. A memory
    /// loop's `end_ms` arms RELOOP.
    CallCue {
        deck: u8,
        ms: f64,
        #[serde(default)]
        end_ms: Option<f64>,
    },
    Tempo {
        deck: u8,
        ratio: f64,
    },
    Nudge {
        deck: u8,
        ratio: f64,
    },
    Sync {
        deck: u8,
        on: bool,
        bar: bool,
    },
    TakeMaster {
        deck: u8,
    },
    /// Signed; fractions for 1/8 … 1/2.
    BeatJump {
        deck: u8,
        beats: f64,
    },
    FineJump {
        deck: u8,
        forward: bool,
    },
    BeatLoop {
        deck: u8,
        beats: f64,
    },
    LoopExit {
        deck: u8,
    },
    LoopIn {
        deck: u8,
    },
    LoopOut {
        deck: u8,
    },
    Reloop {
        deck: u8,
    },
    LoopResize {
        deck: u8,
        double: bool,
    },
    Quantise {
        deck: u8,
        on: bool,
    },
    MasterTempo {
        deck: u8,
        on: bool,
    },
    Channel {
        ch: u8,
        param: ChannelParamArg,
        value: f32,
    },
    Filter {
        unit: FilterUnitArg,
        param: FilterParamArg,
        value: f32,
    },
    /// The effect the channel FX knobs drive.
    ChannelFx {
        kind: FxKindArg,
    },
    /// The channel FX PARAMETER knob.
    FxParameter {
        value: f32,
    },
    Crossfader {
        pos: f32,
        curve: f32,
    },
    Master {
        param: MasterParamArg,
        value: f32,
    },
    /// The player's volume slider, on top of the master level.
    Volume {
        value: f32,
    },
}

impl MixControl {
    fn into_control(self, rate: u32) -> reks_mix::Control {
        use reks_mix::{ChannelParam as C, Control, FilterParam as F, MasterParam as M};
        match self {
            Self::Unload { deck } => Control::Unload { deck },
            Self::Play { deck } => Control::Play { deck },
            Self::Pause { deck } => Control::Pause { deck },
            Self::Cue { deck } => Control::Cue { deck },
            Self::CueRelease { deck } => Control::CueRelease { deck },
            Self::Seek { deck, ms, exact } => {
                let frame = reks_core::deck::ms_to_frame(ms.max(0.0), rate);
                if exact {
                    Control::SeekExact { deck, frame }
                } else {
                    Control::Seek { deck, frame }
                }
            }
            Self::HotCue { deck, slot } => Control::HotCue { deck, slot },
            Self::CallCue { deck, ms, end_ms } => Control::CallCue {
                deck,
                frame: reks_core::deck::ms_to_frame(ms.max(0.0), rate),
                end: end_ms.map(|e| reks_core::deck::ms_to_frame(e.max(0.0), rate)),
            },
            Self::Tempo { deck, ratio } => Control::Tempo { deck, ratio },
            Self::Nudge { deck, ratio } => Control::Nudge { deck, ratio },
            Self::Sync { deck, on, bar } => Control::Sync { deck, on, bar },
            Self::TakeMaster { deck } => Control::TakeMaster { deck },
            Self::BeatJump { deck, beats } => Control::BeatJump { deck, beats },
            Self::FineJump { deck, forward } => Control::FineJump { deck, forward },
            Self::BeatLoop { deck, beats } => Control::BeatLoop { deck, beats },
            Self::LoopExit { deck } => Control::LoopExit { deck },
            Self::LoopIn { deck } => Control::LoopIn { deck },
            Self::LoopOut { deck } => Control::LoopOut { deck },
            Self::Reloop { deck } => Control::Reloop { deck },
            Self::LoopResize { deck, double } => Control::LoopResize { deck, double },
            Self::Quantise { deck, on } => Control::Quantise { deck, on },
            Self::MasterTempo { deck, on } => Control::MasterTempo { deck, on },
            Self::Channel { ch, param, value } => Control::Channel {
                ch,
                param: match param {
                    ChannelParamArg::Trim => C::Trim,
                    ChannelParamArg::EqHi => C::EqHi,
                    ChannelParamArg::EqMid1 => C::EqMid1,
                    ChannelParamArg::EqMid2 => C::EqMid2,
                    ChannelParamArg::EqLo => C::EqLo,
                    ChannelParamArg::Eq3Hi => C::Eq3Hi,
                    ChannelParamArg::Eq3Mid => C::Eq3Mid,
                    ChannelParamArg::Eq3Lo => C::Eq3Lo,
                    ChannelParamArg::Fx => C::Fx,
                    ChannelParamArg::Fader => C::Fader,
                    ChannelParamArg::FilterAssign => C::FilterAssign,
                    ChannelParamArg::XfadeAssign => C::XfadeAssign,
                    ChannelParamArg::Cue => C::Cue,
                },
                value,
            },
            Self::Filter { unit, param, value } => Control::Filter {
                unit: match unit {
                    FilterUnitArg::X => reks_mix::FilterUnit::X,
                    FilterUnitArg::Y => reks_mix::FilterUnit::Y,
                },
                param: match param {
                    FilterParamArg::Freq => F::Freq,
                    FilterParamArg::Resonance => F::Resonance,
                    FilterParamArg::HighPass => F::HighPass,
                    FilterParamArg::BandPass => F::BandPass,
                    FilterParamArg::LowPass => F::LowPass,
                    FilterParamArg::On => F::On,
                },
                value,
            },
            Self::ChannelFx { kind } => Control::ChannelFx {
                kind: match kind {
                    FxKindArg::Filter => reks_mix::FxKind::Filter,
                    FxKindArg::Echo => reks_mix::FxKind::Echo,
                    FxKindArg::Reverb => reks_mix::FxKind::Reverb,
                },
            },
            Self::FxParameter { value } => Control::FxParameter { value },
            Self::Crossfader { pos, curve } => Control::Crossfader { pos, curve },
            Self::Master { param, value } => Control::Master {
                param: match param {
                    MasterParamArg::Level => M::Level,
                    MasterParamArg::CueLevel => M::CueLevel,
                    MasterParamArg::CueMix => M::CueMix,
                    MasterParamArg::SplitCue => M::SplitCue,
                    MasterParamArg::CuePostEq => M::CuePostEq,
                },
                value,
            },
            Self::Volume { value } => Control::Volume { value },
        }
    }
}

/// Queues one control. Never blocks on the audio thread; a full queue is an
/// error the UI can retry.
#[tauri::command]
pub fn mix_control(state: State<'_, AppState>, control: MixControl) -> CmdResult<()> {
    let mut s = lock(&state.mix)?;
    if let MixControl::Unload { deck } = control {
        s.decks[deck_index(deck)?] = None;
    }
    let engine = s.engine()?;
    let rate = engine.info().sample_rate;
    engine.send(control.into_control(rate))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> reks_mix::Control {
        serde_json::from_str::<MixControl>(json)
            .unwrap()
            .into_control(48_000)
    }

    #[test]
    fn controls_parse_from_the_ui_shapes() {
        assert!(matches!(
            parse(r#"{"type":"seek","deck":1,"ms":1500}"#),
            reks_mix::Control::Seek {
                deck: 1,
                frame: 72_000
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"hotCue","deck":0,"slot":3}"#),
            reks_mix::Control::HotCue { deck: 0, slot: 3 }
        ));
        assert!(matches!(
            parse(r#"{"type":"callCue","deck":1,"ms":500}"#),
            reks_mix::Control::CallCue {
                deck: 1,
                frame: 24_000,
                end: None
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"callCue","deck":1,"ms":500,"endMs":1000}"#),
            reks_mix::Control::CallCue {
                deck: 1,
                frame: 24_000,
                end: Some(48_000)
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"callCue","deck":1,"ms":500,"endMs":null}"#),
            reks_mix::Control::CallCue { end: None, .. }
        ));
        assert!(matches!(
            parse(r#"{"type":"takeMaster","deck":2}"#),
            reks_mix::Control::TakeMaster { deck: 2 }
        ));
        assert!(matches!(
            parse(r#"{"type":"beatJump","deck":1,"beats":-0.125}"#),
            reks_mix::Control::BeatJump { deck: 1, beats } if beats == -0.125
        ));
        assert!(matches!(
            parse(r#"{"type":"fineJump","deck":1,"forward":true}"#),
            reks_mix::Control::FineJump {
                deck: 1,
                forward: true
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"beatLoop","deck":3,"beats":4}"#),
            reks_mix::Control::BeatLoop { deck: 3, beats } if beats == 4.0
        ));
        assert!(matches!(
            parse(r#"{"type":"loopExit","deck":3}"#),
            reks_mix::Control::LoopExit { deck: 3 }
        ));
        assert!(matches!(
            parse(r#"{"type":"loopIn","deck":0}"#),
            reks_mix::Control::LoopIn { deck: 0 }
        ));
        assert!(matches!(
            parse(r#"{"type":"loopOut","deck":1}"#),
            reks_mix::Control::LoopOut { deck: 1 }
        ));
        assert!(matches!(
            parse(r#"{"type":"reloop","deck":2}"#),
            reks_mix::Control::Reloop { deck: 2 }
        ));
        assert!(matches!(
            parse(r#"{"type":"loopResize","deck":3,"double":false}"#),
            reks_mix::Control::LoopResize {
                deck: 3,
                double: false
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"channel","ch":2,"param":"eqMid1","value":0.25}"#),
            reks_mix::Control::Channel {
                ch: 2,
                param: reks_mix::ChannelParam::EqMid1,
                ..
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"filter","unit":"y","param":"highPass","value":1}"#),
            reks_mix::Control::Filter {
                unit: reks_mix::FilterUnit::Y,
                param: reks_mix::FilterParam::HighPass,
                ..
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"channel","ch":1,"param":"eq3Mid","value":0.25}"#),
            reks_mix::Control::Channel {
                ch: 1,
                param: reks_mix::ChannelParam::Eq3Mid,
                ..
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"channel","ch":0,"param":"fx","value":0}"#),
            reks_mix::Control::Channel {
                param: reks_mix::ChannelParam::Fx,
                ..
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"channelFx","kind":"reverb"}"#),
            reks_mix::Control::ChannelFx {
                kind: reks_mix::FxKind::Reverb
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"fxParameter","value":0.75}"#),
            reks_mix::Control::FxParameter { value } if value == 0.75
        ));
        assert!(matches!(
            parse(r#"{"type":"master","param":"cuePostEq","value":1}"#),
            reks_mix::Control::Master {
                param: reks_mix::MasterParam::CuePostEq,
                ..
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"sync","deck":3,"on":true,"bar":false}"#),
            reks_mix::Control::Sync {
                deck: 3,
                on: true,
                bar: false
            }
        ));
        assert!(matches!(
            parse(r#"{"type":"volume","value":0.5}"#),
            reks_mix::Control::Volume { value } if value == 0.5
        ));
    }

    #[test]
    fn cue_edits_parse_from_the_ui_shapes() {
        let edit = |json: &str| serde_json::from_str::<CueEdit>(json).unwrap();
        assert!(matches!(
            edit(r#"{"type":"setHotCue","slot":2}"#),
            CueEdit::SetHotCue { slot: 2 }
        ));
        assert!(matches!(
            edit(r#"{"type":"addMemoryCue"}"#),
            CueEdit::AddMemoryCue
        ));
        assert!(matches!(
            edit(r#"{"type":"rename","cueId":5,"name":"Drop"}"#),
            CueEdit::Rename { cue_id: 5, name: Some(n) } if n == "Drop"
        ));
        assert!(matches!(
            edit(r#"{"type":"rename","cueId":5,"name":null}"#),
            CueEdit::Rename {
                cue_id: 5,
                name: None
            }
        ));
        assert!(matches!(
            edit(r#"{"type":"setActiveLoop","cueId":4,"on":true}"#),
            CueEdit::SetActiveLoop {
                cue_id: 4,
                on: true
            }
        ));
        assert!(matches!(
            edit(r#"{"type":"delete","cueId":9}"#),
            CueEdit::Delete { cue_id: 9 }
        ));
    }

    #[test]
    fn cue_routing_parses() {
        let c: CueRoutingArg =
            serde_json::from_str(r#"{"type":"outputs","master":[0,1],"cue":[2,3]}"#).unwrap();
        assert_eq!(
            reks_mix::CueRouting::from(c),
            reks_mix::CueRouting::Outputs {
                master: [0, 1],
                cue: [2, 3]
            }
        );
    }

    #[test]
    fn deck_view_reports_milliseconds() {
        let d = DeckState {
            loaded: true,
            position: 24_000,
            frames: 480_000,
            cue: 4_800,
            bpm: Some(126.0),
            loop_in: Some(96_000),
            last_loop: Some(reks_mix::LoopRegion {
                start: 48_000,
                end: 72_000,
                beats: None,
            }),
            ..DeckState::default()
        };
        let v = deck_view(&d, Some(7), 48_000);
        assert_eq!(v.loop_in, Some(2_000.0));
        assert_eq!(
            v.last_loop,
            Some(LoopView {
                start_ms: 1_000.0,
                end_ms: 1_500.0,
                beats: None
            })
        );
        assert_eq!(v.track_id, Some(7));
        assert_eq!(v.position_ms, 500.0);
        assert_eq!(v.duration_ms, 10_000.0);
        assert_eq!(v.cue_ms, 100.0);
        assert_eq!(v.bpm, Some(126.0));
    }
}
