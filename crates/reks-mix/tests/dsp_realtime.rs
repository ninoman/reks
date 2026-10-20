//! The mixer DSP never allocates or frees while processing: a counting
//! global allocator (own test binary, so it does not clash with the engine's
//! guard) watches every `process`/`tick` call with all parameters moving.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use reks_mix::dsp::eq::{ChannelEq, EqBand};
use reks_mix::dsp::filter::{Filter, FilterModes};
use reks_mix::dsp::level::{ChannelLevel, MasterLevel};
use reks_mix::dsp::limiter::Limiter;
use reks_mix::dsp::meter::PeakMeter;
use reks_mix::dsp::xfader::{Assign, AssignSwitch, Crossfader};

struct Guard;

thread_local! {
    static WATCHING: Cell<bool> = const { Cell::new(false) };
}
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator unchanged.
unsafe impl GlobalAlloc for Guard {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if WATCHING.with(Cell::get) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if WATCHING.with(Cell::get) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Guard = Guard;

#[test]
fn dsp_processing_never_allocates_or_frees() {
    let rate = 48_000;
    let mut eq = ChannelEq::new(rate);
    let mut filter = Filter::new(rate);
    let mut level = ChannelLevel::new(1.0, rate);
    let mut master = MasterLevel::new(1.0, rate);
    let mut xfader = Crossfader::new(rate);
    let mut assign = AssignSwitch::new(Assign::X, rate);
    let mut limiter = Limiter::new(rate);
    let mut meter = PeakMeter::new(rate);
    let mut buf = vec![0.0f32; 256 * 2];

    let before = ALLOCS.load(Ordering::Relaxed);
    WATCHING.with(|w| w.set(true));
    for block in 0..200 {
        let v = (block % 50) as f32 / 49.0;
        for (i, s) in buf.iter_mut().enumerate() {
            *s = ((i + block) as f32 * 0.37).sin() * 2.0;
        }
        eq.set(EqBand::Hi, v);
        eq.set(EqBand::Mid1, 1.0 - v);
        eq.set(EqBand::Mid2, v);
        eq.set(EqBand::Lo, 1.0 - v);
        filter.set_on(block % 2 == 0);
        filter.set_modes(FilterModes {
            lp: block % 3 == 0,
            bp: block % 5 == 0,
            hp: block % 7 == 0,
        });
        filter.set_freq(v);
        filter.set_resonance(1.0 - v);
        level.set_trim(v);
        level.set_fader(1.0 - v);
        master.set(v);
        xfader.set_pos(v);
        xfader.set_curve(1.0 - v);
        assign.set(if block % 4 < 2 { Assign::X } else { Assign::Y });

        eq.process(&mut buf);
        filter.process(&mut buf);
        for frame in buf.as_chunks_mut::<2>().0 {
            let l = level.tick();
            let g = l.trim * l.fader * assign.tick(xfader.tick()) * master.tick();
            frame[0] *= g;
            frame[1] *= g;
        }
        limiter.process(&mut buf);
        meter.process(&buf);
    }
    WATCHING.with(|w| w.set(false));
    assert_eq!(ALLOCS.load(Ordering::Relaxed) - before, 0);
    assert!(buf.iter().all(|s| s.is_finite()));
}
