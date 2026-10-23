//! A channel's FX slot (`docs/research/djm-eq-fx.md` §3): every effect's
//! block lives in the strip; the selected one gets the channel's knob and the
//! others get centre. Switching the effect is therefore a knob move: the old
//! one closes and the new one opens over the smoothing time, and echo and
//! reverb tails ring out. A block at rest is bypassed, so a centred channel
//! passes its input bit for bit.

use crate::control::FxKind;
use crate::dsp::channel_echo::ChannelEcho;
use crate::dsp::channel_filter::ChannelFilter;
use crate::dsp::channel_reverb::ChannelReverb;

#[derive(Debug, Clone)]
pub(crate) struct ChannelFx {
    kind: FxKind,
    knob: f32,
    filter: ChannelFilter,
    echo: ChannelEcho,
    reverb: ChannelReverb,
}

impl ChannelFx {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self {
            kind: FxKind::default(),
            knob: 0.5,
            filter: ChannelFilter::new(sample_rate),
            echo: ChannelEcho::new(sample_rate),
            reverb: ChannelReverb::new(sample_rate),
        }
    }

    pub(crate) fn set_knob(&mut self, value: f32) {
        self.knob = value;
        self.route();
    }

    pub(crate) fn set_kind(&mut self, kind: FxKind) {
        self.kind = kind;
        self.route();
    }

    pub(crate) fn set_parameter(&mut self, value: f32) {
        self.filter.set_parameter(value);
        self.echo.set_parameter(value);
        self.reverb.set_parameter(value);
    }

    /// Output frames per beat of the channel's deck (echo time).
    pub(crate) fn set_beat_frames(&mut self, frames: Option<f64>) {
        self.echo.set_beat_frames(frames);
    }

    fn route(&mut self) {
        let knob = |k: FxKind| if k == self.kind { self.knob } else { 0.5 };
        self.filter.set_knob(knob(FxKind::Filter));
        self.echo.set_knob(knob(FxKind::Echo));
        self.reverb.set_knob(knob(FxKind::Reverb));
    }

    /// Interleaved stereo in place; no allocation.
    pub(crate) fn process(&mut self, buf: &mut [f32]) {
        self.filter.process(buf);
        self.echo.process(buf);
        self.reverb.process(buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn run(fx: &mut ChannelFx, buf: &mut [f32]) {
        for c in buf.chunks_mut(64) {
            fx.process(c);
        }
    }

    #[test]
    fn centred_passes_bit_for_bit_whatever_the_effect() {
        for kind in [FxKind::Filter, FxKind::Echo, FxKind::Reverb] {
            let mut fx = ChannelFx::new(RATE);
            fx.set_kind(kind);
            fx.set_parameter(1.0);
            let input: Vec<f32> = (0..9_600)
                .map(|i| ((i * 7919) % 200) as f32 / 100.0 - 1.0)
                .collect();
            let mut buf = input.clone();
            run(&mut fx, &mut buf);
            assert_eq!(buf, input, "{kind:?}");
        }
    }

    #[test]
    fn switching_effect_moves_the_knob_and_lets_the_tail_ring() {
        let mut fx = ChannelFx::new(RATE);
        fx.set_kind(FxKind::Echo);
        fx.set_knob(1.0);
        // Settle the send, then a burst of 3 kHz-ish noise.
        let mut buf = vec![0.0f32; 4_800];
        run(&mut fx, &mut buf);
        let mut burst: Vec<f32> = (0..960)
            .map(|i| if i % 4 < 2 { 0.5 } else { -0.5 })
            .collect();
        run(&mut fx, &mut burst);
        // FILTER now gets the knob (full high-pass); the echo gets centre.
        fx.set_kind(FxKind::Filter);
        let mut tail = vec![0.0f32; RATE as usize * 2];
        run(&mut fx, &mut tail);
        let energy: f32 = tail.iter().map(|s| s * s).sum();
        assert!(energy > 1e-3, "echo tail should keep ringing: {energy}");
        assert!(tail.iter().all(|s| s.is_finite()));
    }
}
