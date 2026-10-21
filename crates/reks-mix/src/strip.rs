//! One mixer channel strip (Xone:92 music channel, R§9.1): trim → EQ →
//! filter assign → channel fader → crossfader assign, with the CUE taps
//! after the trim (pre-EQ) and before the fader (post-EQ). Works on one
//! block of interleaved stereo at a time and never allocates.

use crate::control::{ChannelParam, FilterUnit};
use crate::dsp::eq::{ChannelEq, EqBand};
use crate::dsp::filter::Filter;
use crate::dsp::level::ChannelLevel;
use crate::dsp::meter::PeakMeter;
use crate::dsp::smooth::Smoother;
use crate::dsp::xfader::{Assign, AssignSwitch, XfadeGains};

/// Frames per mixer block.
pub(crate) const BLOCK: usize = crate::varispeed::CHUNK;

/// One block of interleaved stereo.
pub(crate) type Block = [f32; BLOCK * 2];

/// A three-way switch value: X below a third, Y above two thirds, else off.
pub(crate) fn assign_of(value: f32) -> Assign {
    match value {
        v if v < 1.0 / 3.0 => Assign::X,
        v if v > 2.0 / 3.0 => Assign::Y,
        _ => Assign::Off,
    }
}

/// The cue bus at its two taps, one block each; strips add to them.
pub(crate) struct CueTaps<'a> {
    pub(crate) pre: &'a mut [f32],
    pub(crate) post: &'a mut [f32],
}

#[derive(Debug, Clone)]
pub(crate) struct Strip {
    level: ChannelLevel,
    eq: ChannelEq,
    filter_assign: AssignSwitch,
    /// This channel's copies of filter units X and Y, all set alike. The
    /// filters are linear, so filtering each channel's copy sounds the same
    /// as filtering the sum of the channels on a unit, and the channel fader
    /// can still come after the filter.
    filters: [Filter; 2],
    /// Which copies ran in the last block; an idle copy is reset before it
    /// runs again, so it does not replay stale state.
    warm: [bool; 2],
    xfade_assign: AssignSwitch,
    meter: PeakMeter,
    /// CUE button, smoothed so pressing it does not click in the phones.
    cue: Smoother,
    wet: [Block; 2],
    fader: [f32; BLOCK],
    cue_gain: [f32; BLOCK],
}

impl Strip {
    /// Flat, fader up, both assigns off: a deck plays through unchanged.
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self {
            level: ChannelLevel::new(1.0, sample_rate),
            eq: ChannelEq::new(sample_rate),
            filter_assign: AssignSwitch::new(Assign::Off, sample_rate),
            filters: [Filter::new(sample_rate), Filter::new(sample_rate)],
            warm: [false; 2],
            xfade_assign: AssignSwitch::new(Assign::Off, sample_rate),
            meter: PeakMeter::new(sample_rate),
            cue: Smoother::new(0.0, sample_rate),
            wet: [[0.0; BLOCK * 2]; 2],
            fader: [0.0; BLOCK],
            cue_gain: [0.0; BLOCK],
        }
    }

    pub(crate) fn set(&mut self, param: ChannelParam, value: f32) {
        match param {
            ChannelParam::Trim => self.level.set_trim(value),
            ChannelParam::EqHi => self.eq.set(EqBand::Hi, value),
            ChannelParam::EqMid1 => self.eq.set(EqBand::Mid1, value),
            ChannelParam::EqMid2 => self.eq.set(EqBand::Mid2, value),
            ChannelParam::EqLo => self.eq.set(EqBand::Lo, value),
            ChannelParam::Fader => self.level.set_fader(value),
            ChannelParam::FilterAssign => self.filter_assign.set(assign_of(value)),
            ChannelParam::XfadeAssign => self.xfade_assign.set(assign_of(value)),
            ChannelParam::Cue => self.cue.set(if value >= 0.5 { 1.0 } else { 0.0 }),
        }
    }

    /// This channel's copy of a filter unit, for settings.
    pub(crate) fn filter_mut(&mut self, unit: FilterUnit) -> &mut Filter {
        match unit {
            FilterUnit::X => &mut self.filters[0],
            FilterUnit::Y => &mut self.filters[1],
        }
    }

    pub(crate) fn meter_mut(&mut self) -> &mut PeakMeter {
        &mut self.meter
    }

    /// Runs `buf` through the strip in place up to the fader (the channel
    /// meter reads it there), then adds it, faded and assigned, to `mix`.
    /// `xgains` holds the crossfader gains for each frame of the block.
    /// A cued channel also adds itself to both cue taps.
    pub(crate) fn process(
        &mut self,
        buf: &mut [f32],
        xgains: &[XfadeGains],
        mix: &mut [f32],
        taps: &mut CueTaps<'_>,
    ) {
        let frames = buf.as_chunks_mut::<2>().0;
        for ((frame, fader), cue) in frames
            .iter_mut()
            .zip(&mut self.fader)
            .zip(&mut self.cue_gain)
        {
            let g = self.level.tick();
            frame[0] *= g.trim;
            frame[1] *= g.trim;
            *fader = g.fader;
            *cue = self.cue.tick();
        }
        let cued = self.cue.value() > 0.0 || self.cue.target() > 0.0 || self.cue_gain[0] > 0.0;
        if cued {
            add_weighted(taps.pre, buf, &self.cue_gain);
        }
        self.eq.process(buf);
        self.filter(buf);
        self.meter.process(buf);
        if cued {
            add_weighted(taps.post, buf, &self.cue_gain);
        }
        let frames = buf.as_chunks::<2>().0;
        let out = mix.as_chunks_mut::<2>().0;
        for (((frame, out), fader), &gains) in frames.iter().zip(out).zip(&self.fader).zip(xgains) {
            let g = fader * self.xfade_assign.tick(gains);
            out[0] += g * frame[0];
            out[1] += g * frame[1];
        }
    }
}

/// `dst += gain · src`, with one gain per frame.
fn add_weighted(dst: &mut [f32], src: &[f32], gain: &[f32]) {
    let dst = dst.as_chunks_mut::<2>().0;
    for ((d, s), g) in dst.iter_mut().zip(src.as_chunks::<2>().0).zip(gain) {
        d[0] += g * s[0];
        d[1] += g * s[1];
    }
}

impl Strip {
    /// FILTER assign: the dry signal and the two unit copies, mixed by the
    /// switch's smoothed weights. Only copies that are heard run.
    fn filter(&mut self, buf: &mut [f32]) {
        let n = buf.len();
        let mut any = false;
        for (i, side) in [Assign::X, Assign::Y].into_iter().enumerate() {
            let used = self.filter_assign.uses(side);
            if used {
                if !self.warm[i] {
                    self.filters[i].reset();
                }
                self.wet[i][..n].copy_from_slice(buf);
                self.filters[i].process(&mut self.wet[i][..n]);
            } else {
                self.wet[i][..n].fill(0.0);
            }
            self.warm[i] = used;
            any |= used;
        }
        if !any {
            // Settled on off: the signal passes untouched.
            return;
        }
        let [wx, wy] = &self.wet;
        let frames = buf.as_chunks_mut::<2>().0;
        let x = wx[..n].as_chunks::<2>().0;
        let y = wy[..n].as_chunks::<2>().0;
        for ((frame, x), y) in frames.iter_mut().zip(x).zip(y) {
            let w = self.filter_assign.tick_weights();
            for ch in 0..2 {
                frame[ch] = w.off * frame[ch] + w.x * x[ch] + w.y * y[ch];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_values_map_to_x_off_y() {
        assert_eq!(assign_of(0.0), Assign::X);
        assert_eq!(assign_of(0.5), Assign::Off);
        assert_eq!(assign_of(1.0), Assign::Y);
        assert_eq!(assign_of(f32::NAN), Assign::Off);
    }
}
