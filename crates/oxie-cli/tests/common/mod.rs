//! Runs the programs of the integration tests in parallel. `oxie run` builds every script in
//! `$OXIE_CACHE/target`, and cargo builds one crate at a time in a target directory, so each
//! worker builds in a cache directory of its own under `target/tmp/oxie-cache`.

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

fn workers() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get())
}

/// The cache directories no worker is using. The tests of a binary run at the same time and
/// share them, so no more than `workers()` programs build at once.
static FREE: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);
static RETURNED: Condvar = Condvar::new();

struct Cache(PathBuf);

impl Cache {
    fn take() -> Cache {
        let mut free = FREE.lock().unwrap();
        loop {
            let dirs = free.get_or_insert_with(|| {
                (0..workers()).rev().map(|i| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("oxie-cache").join(i.to_string())).collect()
            });
            if let Some(d) = dirs.pop() {
                return Cache(d);
            }
            free = RETURNED.wait(free).unwrap();
        }
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        FREE.lock().unwrap().get_or_insert_with(Vec::new).push(std::mem::take(&mut self.0));
        RETURNED.notify_one();
    }
}

/// Calls `run` on every item, on up to `available_parallelism()` threads, with the directory to
/// pass as `OXIE_CACHE`, and returns the results in the order of `items`. A panic in `run` is
/// raised again here, the first one in the order of `items`.
pub fn parallel<T: Sync, R: Send>(items: &[T], run: impl Fn(&T, &Path) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let mut results = Vec::new();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers().min(items.len()))
            .map(|_| {
                // named like the test, as the panic messages of the test's own thread are
                let thread = std::thread::Builder::new().name(std::thread::current().name().unwrap_or("test").to_string());
                thread
                    .spawn_scoped(s, || {
                        let cache = Cache::take();
                        let mut done = Vec::new();
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(item) = items.get(i) else { break };
                            done.push((i, catch_unwind(AssertUnwindSafe(|| run(item, &cache.0)))));
                        }
                        done
                    })
                    .unwrap()
            })
            .collect();
        for h in handles {
            results.extend(h.join().unwrap());
        }
    });
    results.sort_by_key(|(i, _)| *i);
    results.into_iter().map(|(_, r)| r.unwrap_or_else(|p| resume_unwind(p))).collect()
}
