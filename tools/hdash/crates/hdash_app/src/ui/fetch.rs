//! One generic for the three async reads (git status, PR status, issue lists).
//!
//! Both pre-merge dashboards wrote this out by hand — `hwt-overview` twice, `hgh-issues`
//! once — as a `Sender` kept alongside a `Mutex<Receiver>` plus a `dispatched` set, a
//! `kick_*` startup system, a `kick_*_on_change` system, a `dispatch_visible_*` and a
//! `drain_*`. The three differed only in their key and payload types.
//!
//! Two properties are load-bearing and easy to lose:
//!
//! - **`tx` is retained**, not just `rx`. Work is dispatched incrementally as items
//!   become visible, and every dispatch feeds the same receiver.
//! - **`dispatched` is a guard, not a cache.** It keeps a filter change from re-issuing a
//!   `gh` call that is already in flight or already answered. `reset` clears it, which is
//!   what makes Refresh re-query.

use std::collections::HashSet;
use std::hash::Hash;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender};

use bevy::tasks::IoTaskPool;

/// A keyed, at-most-once fan-out of blocking work onto the [`IoTaskPool`], with results
/// streamed back over one channel.
pub struct Fetch<K: Eq + Hash, M: Send + 'static> {
    tx: Sender<M>,
    rx: Mutex<Receiver<M>>,
    dispatched: HashSet<K>,
}

impl<K: Eq + Hash, M: Send + 'static> Default for Fetch<K, M> {
    fn default() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<M>();
        Self {
            tx,
            rx: Mutex::new(rx),
            dispatched: HashSet::new(),
        }
    }
}

impl<K: Eq + Hash, M: Send + 'static> Fetch<K, M> {
    /// Run `job` off the main thread and deliver its result, unless `key` was already
    /// dispatched. Returns whether the job was started.
    pub fn dispatch(&mut self, key: K, job: impl FnOnce() -> M + Send + 'static) -> bool {
        if !self.dispatched.insert(key) {
            return false;
        }
        let tx = self.tx.clone();
        IoTaskPool::get()
            .spawn(async move {
                let _ = tx.send(job());
            })
            .detach();
        true
    }

    /// Deliver a result directly, without touching the pool — for an answer already known
    /// (a cached terminal PR status). Still subject to the at-most-once guard.
    pub fn deliver(&mut self, key: K, msg: M) -> bool {
        if !self.dispatched.insert(key) {
            return false;
        }
        let _ = self.tx.send(msg);
        true
    }

    /// Take up to `cap` results that have arrived. Empty when nothing landed, so a caller
    /// can skip touching its model resource on idle frames — which is what keeps the
    /// change-detection-driven rebuilds from running every frame.
    pub fn drain(&self, cap: usize) -> Vec<M> {
        let Ok(rx) = self.rx.lock() else {
            return Vec::new();
        };
        let mut msgs = Vec::new();
        while let Ok(m) = rx.try_recv() {
            msgs.push(m);
            if msgs.len() >= cap {
                break;
            }
        }
        msgs
    }

    /// Drop one key's guard, so that key can be dispatched again.
    ///
    /// For work that is **repeatable on demand** — a pull, which the user asks for by
    /// clicking — as opposed to the read-only fetches, whose answer stands until Refresh.
    /// Called when the result lands, so the guard covers exactly the in-flight window and a
    /// double click cannot start a second `git pull` in the same checkout.
    pub fn forget(&mut self, key: &K) {
        self.dispatched.remove(key);
    }

    /// Forget what has been dispatched, so the next dispatch pass re-issues everything.
    /// Refresh calls this; results still in flight from before land harmlessly.
    pub fn reset(&mut self) {
        self.dispatched.clear();
    }
}
