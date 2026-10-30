//! A small bounded worker pool for per-file reads (hash, probe, tags,
//! artwork), which dominate scan and import time (perf audit P3-1).

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

/// Worker threads: enough to overlap I/O on an SSD, few enough not to thrash
/// a spinning disk.
const MIN_THREADS: usize = 4;
const MAX_THREADS: usize = 8;

/// Runs `work` on every item on a few threads and hands the results to
/// `consume` on the calling thread, in input order. Workers stop taking new
/// items once `stop()` is true or `consume` fails; items already taken
/// finish, so `consume` always sees a contiguous prefix of `items`.
pub(crate) fn for_each_ordered<T, R, E>(
    items: &[T],
    stop: &(dyn Fn() -> bool + Sync),
    work: impl Fn(&T) -> R + Sync,
    mut consume: impl FnMut(usize, R) -> Result<(), E>,
) -> Result<(), E>
where
    T: Sync,
    R: Send,
{
    let threads = std::thread::available_parallelism()
        .map_or(MIN_THREADS, NonZeroUsize::get)
        .clamp(MIN_THREADS, MAX_THREADS)
        .min(items.len());
    let next = AtomicUsize::new(0);
    let halt = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();
        for _ in 0..threads {
            let tx = tx.clone();
            let (next, halt, work) = (&next, &halt, &work);
            scope.spawn(move || {
                while !halt.load(Ordering::Relaxed) && !stop() {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    if tx.send((i, work(item))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        // Results arrive out of order; hold them until their turn.
        let mut waiting = BTreeMap::new();
        let mut turn = 0;
        for (i, r) in rx {
            waiting.insert(i, r);
            while let Some(r) = waiting.remove(&turn) {
                if let Err(e) = consume(turn, r) {
                    halt.store(true, Ordering::Relaxed);
                    return Err(e);
                }
                turn += 1;
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_come_back_in_input_order() {
        let items: Vec<u64> = (0..500).collect();
        let mut seen = Vec::new();
        let r: Result<(), ()> = for_each_ordered(
            &items,
            &|| false,
            // Uneven work so threads finish out of order.
            |&n| {
                std::thread::sleep(std::time::Duration::from_micros((n * 7919) % 300));
                n * 2
            },
            |i, r| {
                seen.push((i, r));
                Ok(())
            },
        );
        assert!(r.is_ok());
        let want: Vec<(usize, u64)> = (0..500).map(|n| (n as usize, n * 2)).collect();
        assert_eq!(seen, want);
    }

    #[test]
    fn stop_and_errors_end_early_with_a_prefix() {
        let items: Vec<usize> = (0..10_000).collect();
        let stopped = AtomicBool::new(false);
        let mut seen = Vec::new();
        let r: Result<(), ()> = for_each_ordered(
            &items,
            &|| stopped.load(Ordering::Relaxed),
            |&n| {
                std::thread::sleep(std::time::Duration::from_micros(20));
                n
            },
            |i, n| {
                if i == 100 {
                    stopped.store(true, Ordering::Relaxed);
                }
                seen.push(n);
                Ok(())
            },
        );
        assert!(r.is_ok());
        assert!(seen.len() < items.len());
        assert_eq!(seen, (0..seen.len()).collect::<Vec<_>>());

        let r = for_each_ordered(
            &items,
            &|| false,
            |&n| n,
            |i, _| {
                if i == 5 {
                    Err(i)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(r, Err(5));
    }

    #[test]
    fn empty_input() {
        let r: Result<(), ()> = for_each_ordered(&[] as &[u8], &|| false, |_| (), |_, _| Ok(()));
        assert!(r.is_ok());
    }
}
