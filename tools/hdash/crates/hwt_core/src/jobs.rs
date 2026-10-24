//! The background job pool: a few worker threads running blocking jobs (git reads, `gh` queries,
//! scans, probes, restores) off the caller's thread.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::thread;

type Job = Box<dyn FnOnce() + Send + 'static>;

/// Queue `job` for the pool and return at once. The pool starts on first use with a quarter of
/// the available cores, at least 1 and at most 4 threads, so at most that many jobs run at once.
///
/// A job that panics is caught: the pool keeps its thread, and everything the job owned —
/// including the sending end of a channel it was to answer on — is dropped, so a caller waiting
/// on that channel sees it close. The panic message goes through the process's panic hook.
pub fn spawn_job(job: impl FnOnce() + Send + 'static) {
    let _ = pool().lock().map(|tx| tx.send(Box::new(job)));
}

fn pool() -> &'static Mutex<Sender<Job>> {
    static POOL: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        let rx = std::sync::Arc::new(Mutex::new(rx));
        for i in 0..pool_size() {
            let rx = rx.clone();
            let _ = thread::Builder::new()
                .name(format!("hdash-job-{i}"))
                .spawn(move || worker(&rx));
        }
        Mutex::new(tx)
    })
}

fn worker(rx: &Mutex<Receiver<Job>>) {
    loop {
        let job = match rx.lock() {
            Ok(rx) => rx.recv(),
            Err(_) => return,
        };
        let Ok(job) = job else { return };
        let _ = catch_unwind(AssertUnwindSafe(job));
    }
}

fn pool_size() -> usize {
    let cores = thread::available_parallelism().map_or(1, |n| n.get());
    (cores / 4).clamp(1, 4)
}

/// A one-shot answer on a channel that may be shared: [`Reply::send`] answers a value, and
/// dropping it unsent answers `fallback`. A job that owns one therefore answers even when it
/// panics, which a receiver on a shared channel could not otherwise notice.
pub struct Reply<T> {
    tx: Sender<T>,
    fallback: Option<T>,
}

impl<T> Reply<T> {
    /// A reply on `tx` that answers `fallback` if it is dropped unsent.
    pub fn new(tx: Sender<T>, fallback: T) -> Self {
        Self {
            tx,
            fallback: Some(fallback),
        }
    }

    /// Answer `value`; the fallback is then never sent.
    pub fn send(mut self, value: T) {
        self.fallback = None;
        let _ = self.tx.send(value);
    }
}

impl<T> Drop for Reply<T> {
    fn drop(&mut self) {
        if let Some(fallback) = self.fallback.take() {
            let _ = self.tx.send(fallback);
        }
    }
}
