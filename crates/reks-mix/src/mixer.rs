//! The mixer (D21, `docs/mixer-dsp.md`): four channel strips, the two
//! filter units' settings, the crossfader, and the master bus (level →
//! meter → safety limiter). Runs one block at a time on the audio thread;
//! everything is allocated in [`Mixer::new`].

use crate::control::{Control, FilterParam, FilterUnit, MasterParam};
use crate::dsp::filter::{FilterModes, FilterPanel};
use crate::dsp::level::MasterLevel;
use crate::dsp::limiter::Limiter;
use crate::dsp::meter::PeakMeter;
use crate::dsp::xfader::{Crossfader, XfadeGains};
use crate::graph::{volume_gain, Ramp, DECKS};
use crate::strip::{Block, Strip, BLOCK};

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
    panel: FilterPanel,
    modes: [FilterModes; 2],
    xfader: Crossfader,
    xgains: [XfadeGains; BLOCK],
    master: MasterLevel,
    /// Preview player volume ([`Control::Volume`]), on top of the master level.
    volume: Ramp,
    volume_value: f32,
    master_meter: PeakMeter,
    limiter: Limiter,
    mix: Block,
}

impl Mixer {
    /// Every channel flat with its fader up and both assigns off, master up:
    /// a single deck (the preview player) plays through unchanged.
    pub(crate) fn new(sample_rate: u32) -> Self {
        let lp = FilterModes {
            lp: true,
            bp: false,
            hp: false,
        };
        Self {
            strips: std::array::from_fn(|_| Strip::new(sample_rate)),
            panel: FilterPanel {
                freq_x: 1.0,
                freq_y: 1.0,
                to_xfader_x: false,
                to_xfader_y: false,
                link: false,
            },
            modes: [lp; 2],
            xfader: Crossfader::new(sample_rate),
            xgains: [XfadeGains { x: 1.0, y: 1.0 }; BLOCK],
            master: MasterLevel::new(1.0, sample_rate),
            volume: Ramp::new(volume_gain(1.0), sample_rate),
            volume_value: 1.0,
            master_meter: PeakMeter::new(sample_rate),
            limiter: Limiter::new(sample_rate),
            mix: [0.0; BLOCK * 2],
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
                MasterParam::CueLevel
                | MasterParam::CueMix
                | MasterParam::SplitCue
                | MasterParam::CuePostEq => {}
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
            FilterParam::Freq => match unit {
                FilterUnit::X => self.panel.freq_x = value,
                FilterUnit::Y => self.panel.freq_y = value,
            },
            FilterParam::VcfToXfader => match unit {
                FilterUnit::X => self.panel.to_xfader_x = switch(value),
                FilterUnit::Y => self.panel.to_xfader_y = switch(value),
            },
            // A panel switch: either unit's control sets it.
            FilterParam::Link => self.panel.link = switch(value),
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
    /// samples and is processed in place; `out` gets the limited master.
    pub(crate) fn process(&mut self, inputs: &mut [Block; DECKS], out: &mut [f32]) {
        let n = out.len();
        let frames = n / 2;
        // VCF TO XFADER and LINK move the cutoff once a block; the filters
        // smooth it.
        let (fx, fy) = self.panel.effective_freq(self.xfader.pos());
        for s in &mut self.strips {
            s.filter_mut(FilterUnit::X).set_freq(fx);
            s.filter_mut(FilterUnit::Y).set_freq(fy);
        }
        for g in &mut self.xgains[..frames] {
            *g = self.xfader.tick();
        }
        let mix = &mut self.mix[..n];
        mix.fill(0.0);
        for (s, input) in self.strips.iter_mut().zip(inputs.iter_mut()) {
            s.process(&mut input[..n], &self.xgains[..frames], mix);
        }
        for frame in mix.as_chunks_mut::<2>().0 {
            let g = self.master.tick() * self.volume.next();
            frame[0] *= g;
            frame[1] *= g;
        }
        self.master_meter.process(mix);
        self.limiter.process(mix);
        out.copy_from_slice(mix);
    }
}
