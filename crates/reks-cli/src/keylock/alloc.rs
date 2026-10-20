//! Counts heap allocations and frees made by the current thread while
//! watching, to check an engine's process call for real-time safety.
//! Only Rust's allocator is visible: C++ libraries allocate through their
//! own `new`/`malloc`, so their counts are not covered.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

struct Counting;

thread_local! {
    static WATCHING: Cell<bool> = const { Cell::new(false) };
}
static EVENTS: AtomicU64 = AtomicU64::new(0);

// SAFETY: forwards to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note();
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

fn note() {
    // `try_with`: the thread-local may be gone while a thread shuts down.
    if WATCHING.try_with(Cell::get).unwrap_or(false) {
        EVENTS.fetch_add(1, Ordering::Relaxed);
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Runs `f` and returns its result with the allocation events it caused.
pub fn count<T>(f: impl FnOnce() -> T) -> (T, u64) {
    let before = EVENTS.load(Ordering::Relaxed);
    WATCHING.with(|w| w.set(true));
    let out = f();
    WATCHING.with(|w| w.set(false));
    (out, EVENTS.load(Ordering::Relaxed) - before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_only_inside() {
        let (v, n) = count(|| vec![1u8; 64]);
        assert!(n >= 1);
        let (_, n) = count(|| v.len());
        assert_eq!(n, 0);
    }
}
