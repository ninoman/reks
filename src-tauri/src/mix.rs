//! Mixer commands (Phase 5 task 8): start and stop the `reks-mix` engine,
//! load library tracks onto decks, forward controls, and stream the engine
//! state to the UI on a `Channel` at about 60 Hz. Decoding and grid
//! conversion live in `reks_core::deck`; the engine in `reks_mix`. This file
//! only maps between their types and the UI's serde shapes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reks_core::{DeckAudio, TrackId};
use reks_mix::{DeckSource, DeckState, Engine, EngineConfig, DECKS};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::{blocking, CmdError, CmdResult};
use crate::state::AppState;

/// State snapshots sent to the UI per second (the engine makes about 60).
const FEED_HZ: u64 = 60;

/// The mixer engine, when started, and what is on its decks.
#[derive(Debug, Default)]
pub struct Mix {
    slot: Arc<Mutex<MixSlot>>,
}

#[derive(Debug, Default)]
struct MixSlot {
    engine: Option<Engine>,
    decks: [Option<LoadedDeck>; DECKS],
    /// Bumped by every load request per deck; a decode that finishes after
    /// a newer request is dropped.
    generation: [u64; DECKS],
    /// Cleared to stop the state feed of the running engine.
    feed: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Copy)]
struct LoadedDeck {
    track_id: TrackId,
}

impl MixSlot {
    fn engine(&mut self) -> CmdResult<&mut Engine> {
        self.engine
            .as_mut()
            .ok_or_else(|| CmdError("the mixer's audio output is not started".into()))
    }

    fn stop(&mut self) {
        if let Some(feed) = self.feed.take() {
            feed.store(false, Ordering::Relaxed);
        }
        self.engine = None;
        self.decks = [None; DECKS];
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
    pub leader: Option<u8>,
    pub leader_chosen: bool,
    pub decks: Vec<DeckView>,
    /// Channel meters, after EQ and filter, before the fader.
    pub meters: Vec<MeterView>,
    /// Master meter, before the limiter.
    pub master_meter: MeterView,
}

fn frames_to_ms(frames: i64, rate: u32) -> f64 {
    frames as f64 * 1000.0 / f64::from(rate.max(1))
}

fn deck_view(d: &DeckState, loaded: Option<LoadedDeck>, rate: u32) -> DeckView {
    DeckView {
        track_id: loaded.map(|l| l.track_id),
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
    }
}

fn snapshot(slot: &mut MixSlot) -> Option<MixState> {
    let decks = slot.decks;
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
        leader: st.leader,
        leader_chosen: st.leader_chosen,
        decks: st
            .decks
            .iter()
            .zip(decks)
            .map(|(d, l)| deck_view(d, l, rate))
            .collect(),
        meters: st.channels.iter().map(|&m| m.into()).collect(),
        master_meter: st.master.into(),
    })
}

fn lock(mix: &Mix) -> CmdResult<std::sync::MutexGuard<'_, MixSlot>> {
    mix.slot
        .lock()
        .map_err(|_| CmdError("mixer state poisoned".into()))
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
    pub track_id: TrackId,
    pub duration_ms: f64,
    /// Hot cues set on the deck.
    pub hot_cues: Vec<HotCueView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotCueView {
    /// 0..7 = A..H.
    pub slot: u8,
    pub position_ms: f64,
}

fn deck_source(audio: DeckAudio) -> DeckSource {
    let anchors: Vec<reks_mix::GridAnchor> = audio
        .anchors
        .iter()
        .map(|a| reks_mix::GridAnchor {
            frame: a.frame,
            bpm: a.bpm,
            beat_in_bar: a.beat_in_bar,
            beats_per_bar: a.beats_per_bar,
        })
        .collect();
    DeckSource::new(audio.pcm.samples, audio.pcm.sample_rate)
        .with_grid(&anchors)
        .with_hot_cues(audio.hot_cues)
}

fn deck_index(deck: u8) -> CmdResult<usize> {
    let i = usize::from(deck);
    if i < DECKS {
        Ok(i)
    } else {
        Err(CmdError(format!("no deck {}", i + 1)))
    }
}

/// Decodes track `id` at the engine rate on a worker thread, builds its deck
/// source there, then loads it on `deck` (stopped at the start) unless a
/// newer load for that deck came in meanwhile. Returns `None` then.
#[tauri::command]
pub async fn mix_load(
    state: State<'_, AppState>,
    deck: u8,
    id: TrackId,
) -> CmdResult<Option<DeckLoaded>> {
    let i = deck_index(deck)?;
    let (generation, rate) = {
        let mut s = lock(&state.mix)?;
        let rate = s.engine()?.info().sample_rate;
        s.generation[i] += 1;
        (s.generation[i], rate)
    };
    let (src, hot_cues) = blocking(&state, move |lib| {
        let audio = lib.deck_audio(id, rate)?;
        let hot_cues = audio
            .hot_cues
            .iter()
            .map(|&(slot, frame)| HotCueView {
                slot,
                position_ms: frames_to_ms(frame, rate),
            })
            .collect::<Vec<_>>();
        Ok::<_, reks_core::Error>((Arc::new(deck_source(audio)), hot_cues))
    })
    .await?;
    let mut s = lock(&state.mix)?;
    // A newer load, or a restart at another rate, won meanwhile.
    if s.generation[i] != generation || s.engine()?.info().sample_rate != rate {
        return Ok(None);
    }
    let duration_ms = frames_to_ms(src.frames(), rate);
    s.engine()?.send(reks_mix::Control::Load { deck, src })?;
    s.decks[i] = Some(LoadedDeck { track_id: id });
    Ok(Some(DeckLoaded {
        deck,
        track_id: id,
        duration_ms,
        hot_cues,
    }))
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
    VcfToXfader,
    Link,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterUnitArg {
    X,
    Y,
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
    },
    HotCue {
        deck: u8,
        slot: u8,
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
    Leader {
        deck: Option<u8>,
    },
    Quantise {
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
            Self::Seek { deck, ms } => Control::Seek {
                deck,
                frame: reks_core::deck::ms_to_frame(ms.max(0.0), rate),
            },
            Self::HotCue { deck, slot } => Control::HotCue { deck, slot },
            Self::Tempo { deck, ratio } => Control::Tempo { deck, ratio },
            Self::Nudge { deck, ratio } => Control::Nudge { deck, ratio },
            Self::Sync { deck, on, bar } => Control::Sync { deck, on, bar },
            Self::Leader { deck } => Control::Leader { deck },
            Self::Quantise { deck, on } => Control::Quantise { deck, on },
            Self::Channel { ch, param, value } => Control::Channel {
                ch,
                param: match param {
                    ChannelParamArg::Trim => C::Trim,
                    ChannelParamArg::EqHi => C::EqHi,
                    ChannelParamArg::EqMid1 => C::EqMid1,
                    ChannelParamArg::EqMid2 => C::EqMid2,
                    ChannelParamArg::EqLo => C::EqLo,
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
                    FilterParamArg::VcfToXfader => F::VcfToXfader,
                    FilterParamArg::Link => F::Link,
                },
                value,
            },
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
            parse(r#"{"type":"leader","deck":null}"#),
            reks_mix::Control::Leader { deck: None }
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
            parse(r#"{"type":"filter","unit":"y","param":"vcfToXfader","value":1}"#),
            reks_mix::Control::Filter {
                unit: reks_mix::FilterUnit::Y,
                param: reks_mix::FilterParam::VcfToXfader,
                ..
            }
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
            ..DeckState::default()
        };
        let v = deck_view(&d, Some(LoadedDeck { track_id: 7 }), 48_000);
        assert_eq!(v.track_id, Some(7));
        assert_eq!(v.position_ms, 500.0);
        assert_eq!(v.duration_ms, 10_000.0);
        assert_eq!(v.cue_ms, 100.0);
        assert_eq!(v.bpm, Some(126.0));
    }
}
