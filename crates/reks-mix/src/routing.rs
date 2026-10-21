//! Master and headphone cue onto device output channels ([`CueRouting`]).

use cpal::{FromSample, Sample};

use crate::control::CueRouting;
use crate::error::{Error, Result};

/// Whether a device with `channels` outputs can take `routing`.
pub(crate) fn check(routing: CueRouting, channels: u16) -> Result<()> {
    let fits = match routing {
        CueRouting::Outputs { master, cue } => master.iter().chain(&cue).all(|&c| c < channels),
        CueRouting::SplitMono => channels >= 2,
        CueRouting::Off => true,
    };
    if fits {
        Ok(())
    } else {
        Err(Error::CueRouting { routing, channels })
    }
}

/// The routing to offer first for a device with `channels` outputs: master
/// on 1/2 and cue on 3/4 when there are four, else no cue (split mono would
/// take the master off the speakers, so it is the user's choice).
pub fn default_cue_routing(channels: u16) -> CueRouting {
    if channels >= 4 {
        CueRouting::Outputs {
            master: [0, 1],
            cue: [2, 3],
        }
    } else {
        CueRouting::Off
    }
}

/// Writes one device frame. `phones` is the cue bus output (split when the
/// routing is [`CueRouting::SplitMono`]); channels nothing is routed to are
/// silent. A mono device gets the master, summed.
pub(crate) fn route<T: Sample + FromSample<f32>>(
    routing: CueRouting,
    master: [f32; 2],
    phones: [f32; 2],
    dst: &mut [T],
) {
    if let [only] = dst {
        *only = T::from_sample((master[0] + master[1]) * 0.5);
        return;
    }
    dst.fill(T::EQUILIBRIUM);
    let mut put = |ch: u16, v: f32| {
        if let Some(d) = dst.get_mut(usize::from(ch)) {
            *d = T::from_sample(v);
        }
    };
    match routing {
        CueRouting::Off => {
            put(0, master[0]);
            put(1, master[1]);
        }
        CueRouting::Outputs { master: m, cue } => {
            put(m[0], master[0]);
            put(m[1], master[1]);
            put(cue[0], phones[0]);
            put(cue[1], phones[1]);
        }
        CueRouting::SplitMono => {
            put(0, phones[0]);
            put(1, phones[1]);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_routing_fits_the_device() {
        for ch in 0..=8 {
            let r = super::default_cue_routing(ch);
            assert!(super::check(r, ch).is_ok(), "{ch} channels: {r:?}");
        }
        assert_eq!(super::default_cue_routing(2), super::CueRouting::Off);
        assert!(matches!(
            super::default_cue_routing(4),
            super::CueRouting::Outputs { cue: [2, 3], .. }
        ));
    }

    use super::*;

    const M: [f32; 2] = [0.25, -0.25];
    const P: [f32; 2] = [0.5, 0.125];

    fn routed(routing: CueRouting, channels: usize) -> Vec<f32> {
        let mut out = vec![9.0; channels];
        route(routing, M, P, &mut out);
        out
    }

    #[test]
    fn each_mode_fills_its_channels_and_silences_the_rest() {
        assert_eq!(routed(CueRouting::Off, 4), [0.25, -0.25, 0.0, 0.0]);
        let four = CueRouting::Outputs {
            master: [0, 1],
            cue: [2, 3],
        };
        assert_eq!(routed(four, 4), [0.25, -0.25, 0.5, 0.125]);
        let swapped = CueRouting::Outputs {
            master: [3, 2],
            cue: [4, 5],
        };
        assert_eq!(routed(swapped, 6), [0.0, 0.0, -0.25, 0.25, 0.5, 0.125]);
        assert_eq!(routed(CueRouting::SplitMono, 2), [0.5, 0.125]);
        assert_eq!(routed(CueRouting::SplitMono, 1), [0.0]);
        assert_eq!(routed(CueRouting::Off, 1), [0.0]);
    }

    #[test]
    fn routing_must_fit_the_device() {
        let four = CueRouting::Outputs {
            master: [0, 1],
            cue: [2, 3],
        };
        assert!(check(four, 4).is_ok());
        assert!(matches!(
            check(four, 2),
            Err(Error::CueRouting { channels: 2, .. })
        ));
        assert!(check(CueRouting::SplitMono, 2).is_ok());
        assert!(check(CueRouting::SplitMono, 1).is_err());
        assert!(check(CueRouting::Off, 1).is_ok());
    }
}
