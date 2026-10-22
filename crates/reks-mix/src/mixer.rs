//! The mixer (D21, `docs/mixer-dsp.md`): four channel strips, the two
//! filter units' settings, the crossfader, the master bus (level → meter →
//! safety limiter) and the headphone cue ([`crate::cue`]). Runs one block
//! at a time on the audio thread; everything is allocated in [`Mixer::new`].

use crate::control::{Control, CueRouting, FilterParam, FilterUnit, MasterParam};
use crate::cue::CueBus;
use crate::dsp::filter::FilterModes;
use crate::dsp::level::MasterLevel;
use crate::dsp::limiter::Limiter;
use crate::dsp::meter::PeakMeter;
use crate::dsp::xfader::{Crossfader, XfadeGains};
use crate::graph::{volume_gain, Ramp, DECKS};
use crate::strip::{default_filter_assign, Block, CueTaps, Strip, BLOCK};

/// A meter reading for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeterState {
    /// Peak level with a falling display, linear (1.0 = 0 dBFS).
    pub level: f32,
    /// A sample reached 0 dBFS since the last snapshot.
    pub clip: bool,
}

impl MeterState {
    fn take(m: &mut PeakMeter) -> Self {
        Self {
            level: m.level(),
            clip: m.take_clip(),
        }
    }
}

/// Switch positions arrive as knob values; on from half travel.
fn switch(value: f32) -> bool {
    value >= 0.5
}

#[derive(Debug, Clone)]
pub(crate) struct Mixer {
    strips: [Strip; DECKS],
    modes: [FilterModes; 2],
    xfader: Crossfader,
    xgains: [XfadeGains; BLOCK],
    master: MasterLevel,
    /// Preview player volume ([`Control::Volume`]), on top of the master level.
    volume: Ramp,
    volume_value: f32,
    master_meter: PeakMeter,
    limiter: Limiter,
    cue: CueBus,
    mix: Block,
    cue_pre: Block,
    cue_post: Block,
}

impl Mixer {
    /// Every channel flat with its fader up, crossfader assign off and FILTER
    /// assign at its default (1–2 on X, 3–4 on Y), both units off, master
    /// up: a single deck (the preview player) plays through unchanged.
    pub(crate) fn new(sample_rate: u32) -> Self {
        let lp = FilterModes {
            lp: true,
            bp: false,
            hp: false,
        };
        Self {
            strips: std::array::from_fn(|ch| Strip::new(sample_rate, default_filter_assign(ch))),
            modes: [lp; 2],
            xfader: Crossfader::new(sample_rate),
            xgains: [XfadeGains { x: 1.0, y: 1.0 }; BLOCK],
            master: MasterLevel::new(1.0, sample_rate),
            volume: Ramp::new(volume_gain(1.0), sample_rate),
            volume_value: 1.0,
            master_meter: PeakMeter::new(sample_rate),
            limiter: Limiter::new(sample_rate),
            cue: CueBus::new(sample_rate),
            mix: [0.0; BLOCK * 2],
            cue_pre: [0.0; BLOCK * 2],
            cue_post: [0.0; BLOCK * 2],
        }
    }

    /// Takes a mixer control; false for anything else.
    pub(crate) fn apply(&mut self, c: &Control) -> bool {
        match *c {
            Control::Channel { ch, param, value } => {
                if let Some(s) = self.strips.get_mut(usize::from(ch)) {
                    s.set(param, value);
                }
            }
            Control::Filter { unit, param, value } => self.set_filter(unit, param, value),
            Control::Crossfader { pos, curve } => {
                self.xfader.set_pos(pos);
                self.xfader.set_curve(curve);
            }
            Control::Master { param, value } => match param {
                MasterParam::Level => self.master.set(value),
                _ => self.cue.set(param, value),
            },
            Control::Volume { value } => {
                self.volume_value = if value.is_finite() {
                    value.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                self.volume.target = volume_gain(value);
            }
            _ => return false,
        }
        true
    }

    fn set_filter(&mut self, unit: FilterUnit, param: FilterParam, value: f32) {
        let u = match unit {
            FilterUnit::X => 0,
            FilterUnit::Y => 1,
        };
        match param {
            FilterParam::Freq => {
                for s in &mut self.strips {
                    s.filter_mut(unit).set_freq(value);
                }
            }
            FilterParam::Resonance => {
                for s in &mut self.strips {
                    s.filter_mut(unit).set_resonance(value);
                }
            }
            FilterParam::On => {
                for s in &mut self.strips {
                    s.filter_mut(unit).set_on(switch(value));
                }
            }
            FilterParam::HighPass | FilterParam::BandPass | FilterParam::LowPass => {
                let m = &mut self.modes[u];
                match param {
                    FilterParam::HighPass => m.hp = switch(value),
                    FilterParam::BandPass => m.bp = switch(value),
                    _ => m.lp = switch(value),
                }
                let m = *m;
                for s in &mut self.strips {
                    s.filter_mut(unit).set_modes(m);
                }
            }
        }
    }

    /// A device with one stereo output for master and cue gets split cue.
    pub(crate) fn set_cue_routing(&mut self, routing: CueRouting) {
        self.cue.force_split(routing == CueRouting::SplitMono);
    }

    pub(crate) fn volume(&self) -> f32 {
        self.volume_value
    }

    /// Frames of delay the limiter adds to the output.
    pub(crate) fn latency(&self) -> usize {
        self.limiter.latency()
    }

    /// Channel and master meters; reading clears the clip latches.
    pub(crate) fn meters(&mut self) -> ([MeterState; DECKS], MeterState) {
        let channels = std::array::from_fn(|i| MeterState::take(self.strips[i].meter_mut()));
        (channels, MeterState::take(&mut self.master_meter))
    }

    /// Mixes one block. `inputs[i]` holds deck `i`'s first `out.len()`
    /// samples and is processed in place; `out` gets the limited master and
    /// `phones`, when given, the limited headphone cue. Both limiters have
    /// the same look-ahead, so the two stay aligned.
    pub(crate) fn process(
        &mut self,
        inputs: &mut [Block; DECKS],
        out: &mut [f32],
        phones: Option<&mut [f32]>,
    ) {
        let n = out.len();
        let frames = n / 2;
        for g in &mut self.xgains[..frames] {
            *g = self.xfader.tick();
        }
        let mix = &mut self.mix[..n];
        mix.fill(0.0);
        let mut taps = CueTaps {
            pre: &mut self.cue_pre[..n],
            post: &mut self.cue_post[..n],
        };
        taps.pre.fill(0.0);
        taps.post.fill(0.0);
        for (s, input) in self.strips.iter_mut().zip(inputs.iter_mut()) {
            s.process(&mut input[..n], &self.xgains[..frames], mix, &mut taps);
        }
        for frame in mix.as_chunks_mut::<2>().0 {
            let g = self.master.tick() * self.volume.next();
            frame[0] *= g;
            frame[1] *= g;
        }
        self.master_meter.process(mix);
        if let Some(phones) = phones {
            self.cue
                .process(&self.cue_pre[..n], &self.cue_post[..n], mix, phones);
        }
        self.limiter.process(mix);
        out.copy_from_slice(mix);
    }
}
