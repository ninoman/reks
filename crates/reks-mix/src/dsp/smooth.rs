//! Parameter smoothing: every control moves to its new value over a short
//! linear ramp, so knob moves and switches never step the signal (no zipper
//! noise, no clicks).

/// Ramp length for every mixer parameter. The plan asks for at least 5 ms;
/// 10 ms also hides filter-mode and assign switches.
pub const SMOOTH_MS: f32 = 10.0;

/// A value that ramps linearly to its target over [`SMOOTH_MS`].
#[derive(Debug, Clone, Copy)]
pub struct Smoother {
    value: f32,
    target: f32,
    step: f32,
    remaining: u32,
    ramp_frames: u32,
}

impl Smoother {
    /// Starts settled at `value`.
    pub fn new(value: f32, sample_rate: u32) -> Self {
        let frames = (sample_rate as f32 * SMOOTH_MS / 1000.0).round().max(1.0) as u32;
        Self {
            value,
            target: value,
            step: 0.0,
            remaining: 0,
            ramp_frames: frames,
        }
    }

    /// Ramps to `target`. Non-finite targets are ignored.
    pub fn set(&mut self, target: f32) {
        if !target.is_finite() || target == self.target {
            return;
        }
        self.target = target;
        self.remaining = self.ramp_frames;
        self.step = (target - self.value) / self.ramp_frames as f32;
    }

    /// Jumps to `value` with no ramp (initial state, tests).
    pub fn snap(&mut self, value: f32) {
        if value.is_finite() {
            self.value = value;
            self.target = value;
            self.remaining = 0;
        }
    }

    /// Advances one frame and returns the new value.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.value = if self.remaining == 0 {
                self.target
            } else {
                self.value + self.step
            };
        }
        self.value
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn is_settled(&self) -> bool {
        self.remaining == 0
    }

    /// Ramp length in frames.
    pub fn ramp_frames(&self) -> u32 {
        self.ramp_frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramps_linearly_over_at_least_5_ms_and_lands_exactly() {
        let mut s = Smoother::new(0.0, 48_000);
        assert!(s.ramp_frames() >= 48_000 * 5 / 1000);
        s.set(1.0);
        let frames = s.ramp_frames();
        let mut last = 0.0;
        for i in 1..=frames {
            let v = s.tick();
            assert!(v >= last, "frame {i} went backwards");
            assert!(v - last <= 1.0 / frames as f32 + 1e-6, "frame {i} jumped");
            last = v;
        }
        assert_eq!(s.value(), 1.0);
        assert!(s.is_settled());
        assert_eq!(s.tick(), 1.0);
    }

    #[test]
    fn retargets_mid_ramp_from_the_current_value() {
        let mut s = Smoother::new(0.0, 48_000);
        s.set(1.0);
        for _ in 0..s.ramp_frames() / 2 {
            s.tick();
        }
        let mid = s.value();
        s.set(0.0);
        let first = s.tick();
        assert!(first < mid && mid - first < 1.0 / s.ramp_frames() as f32);
    }

    #[test]
    fn ignores_non_finite_targets() {
        let mut s = Smoother::new(0.5, 44_100);
        s.set(f32::NAN);
        s.snap(f32::INFINITY);
        assert_eq!(s.tick(), 0.5);
        assert!(s.is_settled());
    }
}
