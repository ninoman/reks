//! The candidate engines this build has, by name. Which exist depends on
//! the `keylock-*` cargo features (the C++ ones need a C++ toolchain).

use super::stretcher::{Feed, Stretcher};
use super::varispeed::Varispeed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Reference: pitch follows tempo, what decks do today.
    Varispeed,
    #[cfg(feature = "keylock-timestretch")]
    TimestretchKeylock,
    #[cfg(feature = "keylock-timestretch")]
    TimestretchWide,
    #[cfg(feature = "keylock-signalsmith")]
    Signalsmith,
    #[cfg(feature = "keylock-rubberband")]
    RubberbandR3,
    #[cfg(feature = "keylock-rubberband")]
    RubberbandR2,
}

/// Engines whose feature is off in this build, with the feature to enable.
pub const MISSING: &[(&str, &str)] = &[
    #[cfg(not(feature = "keylock-timestretch"))]
    (
        "timestretch-keylock, timestretch-wide",
        "keylock-timestretch",
    ),
    #[cfg(not(feature = "keylock-signalsmith"))]
    ("signalsmith", "keylock-signalsmith"),
    #[cfg(not(feature = "keylock-rubberband"))]
    ("rubberband-r3, rubberband-r2", "keylock-rubberband"),
];

impl Engine {
    pub fn all() -> Vec<Engine> {
        vec![
            Engine::Varispeed,
            #[cfg(feature = "keylock-timestretch")]
            Engine::TimestretchKeylock,
            #[cfg(feature = "keylock-timestretch")]
            Engine::TimestretchWide,
            #[cfg(feature = "keylock-signalsmith")]
            Engine::Signalsmith,
            #[cfg(feature = "keylock-rubberband")]
            Engine::RubberbandR3,
            #[cfg(feature = "keylock-rubberband")]
            Engine::RubberbandR2,
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            Engine::Varispeed => "varispeed",
            #[cfg(feature = "keylock-timestretch")]
            Engine::TimestretchKeylock => "timestretch-keylock",
            #[cfg(feature = "keylock-timestretch")]
            Engine::TimestretchWide => "timestretch-wide",
            #[cfg(feature = "keylock-signalsmith")]
            Engine::Signalsmith => "signalsmith",
            #[cfg(feature = "keylock-rubberband")]
            Engine::RubberbandR3 => "rubberband-r3",
            #[cfg(feature = "keylock-rubberband")]
            Engine::RubberbandR2 => "rubberband-r2",
        }
    }

    pub fn parse(name: &str) -> Option<Engine> {
        Engine::all().into_iter().find(|e| e.name() == name)
    }

    /// Builds the engine at tempo `ratio`. Some prime themselves from `feed`.
    pub fn build(
        self,
        rate: u32,
        ratio: f64,
        block: usize,
        feed: &mut Feed<'_>,
    ) -> anyhow::Result<Box<dyn Stretcher>> {
        let _ = (&rate, &feed);
        Ok(match self {
            Engine::Varispeed => Box::new(Varispeed::new(ratio, block)?),
            #[cfg(feature = "keylock-timestretch")]
            Engine::TimestretchKeylock => Box::new(super::timestretch::Timestretch::new(
                timestretch::engine::EngineProfile::Keylock,
                rate,
                ratio,
                block,
            )?),
            #[cfg(feature = "keylock-timestretch")]
            Engine::TimestretchWide => Box::new(super::timestretch::Timestretch::new(
                timestretch::engine::EngineProfile::WideKeylock,
                rate,
                ratio,
                block,
            )?),
            #[cfg(feature = "keylock-signalsmith")]
            Engine::Signalsmith => Box::new(super::signalsmith::Signalsmith::new(
                rate, ratio, block, feed,
            )),
            #[cfg(feature = "keylock-rubberband")]
            Engine::RubberbandR3 => Box::new(super::rubberband::RubberBand::new(true, rate, ratio)),
            #[cfg(feature = "keylock-rubberband")]
            Engine::RubberbandR2 => {
                Box::new(super::rubberband::RubberBand::new(false, rate, ratio))
            }
        })
    }
}
