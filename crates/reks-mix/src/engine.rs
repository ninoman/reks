//! The real-time engine: a cpal output stream driving the [`Graph`].
//!
//! The control side talks to the audio thread only through three wait-free
//! `rtrb` queues: controls in, state snapshots out, and replaced sources out
//! so they are freed off the audio thread. The cpal stream lives on its own
//! thread because `cpal::Stream` is not `Send` on every platform.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

use crate::control::{Control, CueRouting};
use crate::error::{Error, Result};
use crate::graph::{DeckSource, EngineState, Graph};

/// Frames rendered per graph call; bounds the scratch buffer.
const BLOCK: usize = 256;
/// Capacity of each queue. Retired sources share it with controls, so the
/// retire queue cannot fill while the control side drains it on every send.
const QUEUE: usize = 64;
/// State snapshots per second.
const STATE_HZ: u32 = 60;
/// Output rate tried first. Most DJ music is 44.1 kHz, so running the
/// device there spares those tracks the resampler. On macOS this sets the
/// device's clock (as rekordbox does); where the device does not offer it
/// (e.g. WASAPI shared mode), the device's default rate is used.
pub const PREFERRED_RATE: u32 = 44_100;

/// Sample rates offered for selection, when the device supports them.
pub const COMMON_RATES: [u32; 4] = [44_100, 48_000, 88_200, 96_000];

/// How to open the output.
#[derive(Debug, Clone, Default)]
pub struct EngineConfig {
    /// Output device name from [`output_devices`]; `None` for the system default.
    pub device: Option<String>,
    /// Output rate. `None`: [`PREFERRED_RATE`] when the device offers it, else
    /// the device's default. A rate the device does not offer is an error.
    pub sample_rate: Option<u32>,
    /// Frames per device buffer. `None`: the device's default. A size outside
    /// the device's range is an error.
    pub buffer_frames: Option<u32>,
    /// Headphone cue routing (phase 5 task 7; only `Off` so far).
    pub cue: CueRouting,
}

/// What a device can be opened with, for the settings UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceOptions {
    /// The [`COMMON_RATES`] the device offers.
    pub rates: Vec<u32>,
    pub default_rate: u32,
    /// Smallest and largest buffer in frames, when the platform reports them.
    pub buffer_frames: Option<(u32, u32)>,
}

/// Rates and buffer sizes `device` (`None`: system default) offers.
pub fn device_options(device: Option<&str>) -> Result<DeviceOptions> {
    let dev = find_device(device)?;
    let default = dev.default_output_config()?;
    let ranges: Vec<_> = dev
        .supported_output_configs()?
        .filter(|c| {
            c.channels() == default.channels() && c.sample_format() == default.sample_format()
        })
        .collect();
    let rates = COMMON_RATES
        .into_iter()
        .filter(|&r| {
            ranges
                .iter()
                .any(|c| (c.min_sample_rate()..=c.max_sample_rate()).contains(&r))
        })
        .collect();
    let buffer_frames = match default.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => Some((*min, *max)),
        cpal::SupportedBufferSize::Unknown => None,
    };
    Ok(DeviceOptions {
        rates,
        default_rate: default.sample_rate(),
        buffer_frames,
    })
}

/// The buffer setting for a request, checked against what the device reports.
fn buffer_size(
    requested: Option<u32>,
    supported: &cpal::SupportedBufferSize,
    device: &str,
) -> Result<cpal::BufferSize> {
    match (requested, supported) {
        (None, _) => Ok(cpal::BufferSize::Default),
        (Some(n), cpal::SupportedBufferSize::Range { min, max }) if !(*min..=*max).contains(&n) => {
            Err(Error::BufferUnsupported(n, device.to_owned()))
        }
        // An unknown range: cpal rejects a bad size when the stream opens.
        (Some(n), _) => Ok(cpal::BufferSize::Fixed(n)),
    }
}

/// Output devices by name; the system default first when there is one.
pub fn output_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let default = host.default_output_device().and_then(|d| device_name(&d));
    let mut names: Vec<String> = host
        .output_devices()?
        .filter_map(|d| device_name(&d))
        .collect();
    names.dedup();
    if let Some(def) = default {
        names.retain(|n| *n != def);
        names.insert(0, def);
    }
    Ok(names)
}

fn device_name(d: &cpal::Device) -> Option<String> {
    d.description().ok().map(|desc| desc.name().to_owned())
}

fn find_device(device: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    match device {
        None => host.default_output_device(),
        Some(name) => host
            .output_devices()?
            .find(|d| device_name(d).as_deref() == Some(name)),
    }
    .ok_or_else(|| Error::NoDevice(device.map(str::to_owned)))
}

/// A running output stream. Dropping it stops the stream.
#[derive(Debug)]
pub struct Engine {
    controls: Producer<Control>,
    states: Consumer<EngineState>,
    retired: Consumer<Arc<DeckSource>>,
    state: EngineState,
    info: StreamInfo,
    health: Arc<Health>,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

/// What the device gave us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// The fixed buffer size asked for; `None` when the device chose.
    pub buffer_frames: Option<u32>,
}

/// Written by cpal's error callback.
#[derive(Debug, Default)]
struct Health {
    lost: AtomicBool,
    xruns: AtomicU64,
}

impl Engine {
    /// Opens the device and starts the stream (silent until a deck plays)
    /// at the configured rate, or with none configured at
    /// [`PREFERRED_RATE`] when the device takes it, else at its default.
    pub fn start(cfg: EngineConfig) -> Result<Self> {
        if let Some(rate) = cfg.sample_rate {
            return Self::start_at(&cfg, Some(rate));
        }
        match Self::start_at(&cfg, Some(PREFERRED_RATE)) {
            Ok(e) => Ok(e),
            Err(e @ Error::RateUnsupported(..)) => {
                tracing::debug!(error = %e, "using the device's default rate");
                Self::start_at(&cfg, None)
            }
            Err(e) => {
                tracing::warn!(error = %e, rate = PREFERRED_RATE, "falling back to the device's default rate");
                Self::start_at(&cfg, None)
            }
        }
    }

    /// `rate`: `None` for the device's default.
    fn start_at(cfg: &EngineConfig, rate: Option<u32>) -> Result<Self> {
        let device = cfg.device.clone();
        let buffer_frames = cfg.buffer_frames;
        let (ctl_tx, ctl_rx) = RingBuffer::new(QUEUE);
        let (state_tx, state_rx) = RingBuffer::new(QUEUE);
        let (retire_tx, retire_rx) = RingBuffer::new(QUEUE);
        let health = Arc::new(Health::default());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        let thread_health = Arc::clone(&health);
        let thread = std::thread::Builder::new()
            .name("reks-audio-out".into())
            .spawn(move || {
                let queues = Queues {
                    controls: ctl_rx,
                    states: state_tx,
                    retired: retire_tx,
                };
                match open_stream(device, rate, buffer_frames, queues, thread_health) {
                    Ok((stream, info)) => {
                        let _ = ready_tx.send(Ok(info));
                        // Parks until the engine is dropped.
                        let _ = stop_rx.recv();
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|_| Error::ThreadFailed)?;
        let info = ready_rx.recv().map_err(|_| Error::ThreadFailed)??;
        tracing::info!(device = %info.device, rate = info.sample_rate, channels = info.channels, "audio output started");
        Ok(Self {
            controls: ctl_tx,
            states: state_rx,
            retired: retire_rx,
            state: EngineState::default(),
            info,
            health,
            stop: Some(stop_tx),
            thread: Some(thread),
        })
    }

    /// Queues a control. Never blocks.
    pub fn send(&mut self, c: Control) -> Result<()> {
        self.drain_retired();
        self.controls
            .push(c)
            .map_err(|PushError::Full(_)| Error::QueueFull)
    }

    /// The latest state snapshot (about 60 a second while the stream runs).
    pub fn poll_state(&mut self) -> EngineState {
        self.drain_retired();
        while let Ok(s) = self.states.pop() {
            self.state = s;
        }
        self.state
    }

    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// True once the device has gone away; start a new engine to recover.
    pub fn device_lost(&self) -> bool {
        self.health.lost.load(Ordering::Relaxed)
    }

    /// Buffer under/overruns the device reported since start.
    pub fn xruns(&self) -> u64 {
        self.health.xruns.load(Ordering::Relaxed)
    }

    fn drain_retired(&mut self) {
        while self.retired.pop().is_ok() {}
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(t) = self.thread.take() {
            if t.join().is_err() {
                tracing::error!("audio output thread panicked");
            }
        }
    }
}

/// The audio thread's ends of the queues.
#[derive(Debug)]
pub(crate) struct Queues {
    pub controls: Consumer<Control>,
    pub states: Producer<EngineState>,
    pub retired: Producer<Arc<DeckSource>>,
}

fn open_stream(
    device: Option<String>,
    rate: Option<u32>,
    buffer_frames: Option<u32>,
    queues: Queues,
    health: Arc<Health>,
) -> Result<(cpal::Stream, StreamInfo)> {
    let dev = find_device(device.as_deref())?;
    let name = device_name(&dev).unwrap_or_else(|| "unknown".into());
    let default = dev.default_output_config()?;
    let supported = match rate {
        Some(r) if r != default.sample_rate() => dev
            .supported_output_configs()?
            .filter(|c| {
                c.channels() == default.channels() && c.sample_format() == default.sample_format()
            })
            .find_map(|c| c.try_with_sample_rate(r))
            .ok_or_else(|| Error::RateUnsupported(r, name.clone()))?,
        _ => default,
    };
    let mut config = supported.config();
    config.buffer_size = buffer_size(buffer_frames, supported.buffer_size(), &name)?;
    let info = StreamInfo {
        device: name.clone(),
        sample_rate: config.sample_rate,
        channels: config.channels,
        buffer_frames,
    };
    let processor = Processor::new(Graph::new(config.sample_rate), queues, config.sample_rate);

    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&dev, &config, processor, health)?,
        SampleFormat::I16 => build::<i16>(&dev, &config, processor, health)?,
        SampleFormat::I32 => build::<i32>(&dev, &config, processor, health)?,
        SampleFormat::U16 => build::<u16>(&dev, &config, processor, health)?,
        other => return Err(Error::SampleFormat(other.to_string(), name)),
    };
    stream.play()?;
    Ok((stream, info))
}

fn build<T>(
    dev: &cpal::Device,
    config: &cpal::StreamConfig,
    mut processor: Processor,
    health: Arc<Health>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let stream = dev.build_output_stream::<T, _, _>(
        *config,
        move |out: &mut [T], _| processor.process(out, channels),
        move |e: cpal::Error| match e.kind() {
            cpal::ErrorKind::Xrun => {
                health.xruns.fetch_add(1, Ordering::Relaxed);
            }
            cpal::ErrorKind::DeviceNotAvailable => {
                health.lost.store(true, Ordering::Relaxed);
                tracing::warn!("audio output device lost");
            }
            _ => tracing::warn!(error = %e, "audio output error"),
        },
        None,
    )?;
    Ok(stream)
}

/// The audio callback's state: graph, queue ends and scratch, all
/// allocated before the stream starts.
#[derive(Debug)]
pub(crate) struct Processor {
    graph: Graph,
    queues: Queues,
    scratch: Box<[f32]>,
    state_every: u32,
    since_state: u32,
}

impl Processor {
    pub(crate) fn new(graph: Graph, queues: Queues, sample_rate: u32) -> Self {
        Self {
            graph,
            queues,
            scratch: vec![0.0; BLOCK * 2].into_boxed_slice(),
            state_every: (sample_rate / STATE_HZ).max(1),
            since_state: 0,
        }
    }

    /// Fills one device buffer. `out` is interleaved with `channels` per frame:
    /// L/R on the first two, silence on the rest; mono gets (L + R) / 2.
    pub(crate) fn process<T: Sample + FromSample<f32>>(&mut self, out: &mut [T], channels: usize) {
        while let Ok(c) = self.queues.controls.pop() {
            if let Some(old) = self.graph.apply(c) {
                retire(&mut self.queues.retired, old);
            }
        }
        if channels == 0 {
            return;
        }
        let mut frames_total = 0;
        for chunk in out.chunks_mut(BLOCK * channels) {
            let frames = chunk.len() / channels;
            let scratch = &mut self.scratch[..frames * 2];
            let retired = &mut self.queues.retired;
            self.graph.render_with(scratch, |s| retire(retired, s));
            for (dst, src) in chunk
                .chunks_exact_mut(channels)
                .zip(scratch.as_chunks::<2>().0)
            {
                if channels == 1 {
                    dst[0] = T::from_sample((src[0] + src[1]) * 0.5);
                } else {
                    dst[0] = T::from_sample(src[0]);
                    dst[1] = T::from_sample(src[1]);
                    for d in &mut dst[2..] {
                        *d = T::EQUILIBRIUM;
                    }
                }
            }
            frames_total += frames as u32;
        }
        self.since_state += frames_total;
        if self.since_state >= self.state_every {
            self.since_state %= self.state_every;
            // A full queue means nobody is polling; skip this snapshot.
            let _ = self.queues.states.push(self.graph.state());
        }
    }
}

fn retire(queue: &mut Producer<Arc<DeckSource>>, src: Arc<DeckSource>) {
    if let Err(PushError::Full(src)) = queue.push(src) {
        // Only when the control side stopped draining; freeing here is the
        // lesser evil next to leaking a whole track.
        drop(src);
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::sync::atomic::AtomicUsize;

    use super::*;

    /// Counts allocations made on a thread while it is marked real-time.
    struct Guard;

    thread_local! {
        static REALTIME: Cell<bool> = const { Cell::new(false) };
    }
    static RT_ALLOCS: AtomicUsize = AtomicUsize::new(0);

    // SAFETY: forwards to the system allocator unchanged.
    unsafe impl GlobalAlloc for Guard {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if REALTIME.with(Cell::get) {
                RT_ALLOCS.fetch_add(1, Ordering::Relaxed);
            }
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if REALTIME.with(Cell::get) {
                RT_ALLOCS.fetch_add(1, Ordering::Relaxed);
            }
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static GLOBAL: Guard = Guard;

    const RATE: u32 = 48_000;

    struct Rig {
        p: Processor,
        controls: Producer<Control>,
        states: Consumer<EngineState>,
        retired: Consumer<Arc<DeckSource>>,
    }

    fn rig() -> Rig {
        let (controls, ctl_rx) = RingBuffer::new(QUEUE);
        let (state_tx, states) = RingBuffer::new(QUEUE);
        let (retire_tx, retired) = RingBuffer::new(QUEUE);
        let queues = Queues {
            controls: ctl_rx,
            states: state_tx,
            retired: retire_tx,
        };
        Rig {
            p: Processor::new(Graph::new(RATE), queues, RATE),
            controls,
            states,
            retired,
        }
    }

    fn source(frames: usize, value: i16) -> Arc<DeckSource> {
        Arc::new(DeckSource::new(vec![value; frames * 2]))
    }

    /// Runs `f` with allocation counting on; returns the count.
    fn realtime(f: impl FnOnce()) -> usize {
        let before = RT_ALLOCS.load(Ordering::Relaxed);
        REALTIME.with(|r| r.set(true));
        f();
        REALTIME.with(|r| r.set(false));
        RT_ALLOCS.load(Ordering::Relaxed) - before
    }

    #[test]
    fn callback_never_allocates_or_frees() {
        let mut r = rig();
        let first = source(RATE as usize, 1000);
        let second = source(RATE as usize, 2000);
        let mut out_f32 = vec![0.0f32; 512 * 2];
        let mut out_i16 = vec![0i16; 300 * 4];
        let mut out_mono = vec![0u16; 1000];

        r.controls
            .push(Control::Load {
                deck: 0,
                src: Arc::clone(&first),
            })
            .unwrap();
        r.controls.push(Control::Play { deck: 0 }).unwrap();
        let n = realtime(|| r.p.process(&mut out_f32, 2));
        assert_eq!(n, 0, "load + play");

        r.controls
            .push(Control::Seek {
                deck: 0,
                frame: 10_000,
            })
            .unwrap();
        r.controls.push(Control::Volume { value: 0.7 }).unwrap();
        let n = realtime(|| {
            for _ in 0..4 {
                r.p.process(&mut out_i16, 4);
                r.p.process(&mut out_mono, 1);
            }
        });
        assert_eq!(n, 0, "seek, volume, formats");

        // Loading over a playing deck retires `first` through the queue.
        r.controls
            .push(Control::Load {
                deck: 0,
                src: Arc::clone(&second),
            })
            .unwrap();
        r.controls.push(Control::Unload { deck: 0 }).unwrap();
        let n = realtime(|| {
            for _ in 0..4 {
                r.p.process(&mut out_f32, 2);
            }
        });
        assert_eq!(n, 0, "load over playing deck");
        drop(first);
        let mut retired = 0;
        while r.retired.pop().is_ok() {
            retired += 1;
        }
        // `second` when Unload replaced the pending load, then `first`.
        assert_eq!(retired, 2);
    }

    #[test]
    fn routes_stereo_to_the_first_two_channels() {
        let mut r = rig();
        r.controls
            .push(Control::Load {
                deck: 0,
                src: source(RATE as usize, 16_384),
            })
            .unwrap();
        r.controls.push(Control::Play { deck: 0 }).unwrap();
        let mut out = vec![1.0f32; 1024 * 4];
        r.p.process(&mut out, 4);
        let last = &out[out.len() - 4..];
        assert_eq!(last, [0.5, 0.5, 0.0, 0.0]);

        let mut mono = vec![0.0f32; 16];
        r.p.process(&mut mono, 1);
        assert_eq!(mono[15], 0.5);
    }

    #[test]
    fn buffer_size_is_checked_against_the_device_range() {
        let range = cpal::SupportedBufferSize::Range { min: 64, max: 4096 };
        assert_eq!(
            buffer_size(None, &range, "d").unwrap(),
            cpal::BufferSize::Default
        );
        assert_eq!(
            buffer_size(Some(256), &range, "d").unwrap(),
            cpal::BufferSize::Fixed(256)
        );
        assert!(matches!(
            buffer_size(Some(32), &range, "d"),
            Err(Error::BufferUnsupported(32, _))
        ));
        assert_eq!(
            buffer_size(Some(32), &cpal::SupportedBufferSize::Unknown, "d").unwrap(),
            cpal::BufferSize::Fixed(32)
        );
    }

    #[test]
    fn sends_state_about_sixty_times_a_second() {
        let mut r = rig();
        r.controls
            .push(Control::Load {
                deck: 0,
                src: source(10, 0),
            })
            .unwrap();
        let mut out = vec![0.0f32; 256 * 2];
        for _ in 0..(RATE as usize / 256) {
            r.p.process(&mut out, 2);
        }
        let mut n = 0;
        let mut last = None;
        while let Ok(s) = r.states.pop() {
            n += 1;
            last = Some(s);
        }
        assert!((50..=QUEUE).contains(&n), "{n} snapshots");
        assert!(last.unwrap().decks[0].loaded);
    }
}
