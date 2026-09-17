//! A chat turn that outlives the request that started it (VIR-323).
//!
//! The agent loop used to run inside the streaming HTTP response: when the
//! client went away — a tab switched, the app backgrounded, a phone locked —
//! the response was dropped, the loop with it, and the assistant row was
//! never written. The chat you came back to showed your message and no
//! reply, or the error card.
//!
//! Now a turn is driven by its own task and every event it produces is kept
//! here, in order, until the turn ends. The request that started it and any
//! later `GET /api/chat/{id}/stream` are both just watchers: a watcher gets
//! everything so far, then follows. The one thing a watcher's absence does is
//! start a clock — a turn nobody has watched for [`UNATTENDED_CAP`] is
//! cancelled, so a closed laptop cannot spend the wallet on a reply no one
//! will read.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use futures::Stream;
use tokio::sync::Notify;

type SseEvent = axum::response::sse::Event;

/// How long a turn keeps running with nobody watching before it is cancelled.
/// Long enough to survive a phone locking for a bit and coming back; short
/// enough that an abandoned deep-research turn does not run to its 50 steps.
pub const UNATTENDED_CAP: Duration = Duration::from_secs(300);

/// Every turn currently running, by chat id. One per chat: a new turn on the
/// same chat replaces the entry (the old task keeps its own `Arc` and
/// finishes on its own, removing nothing that is not its own).
#[derive(Clone, Default)]
pub struct LiveTurns {
    inner: Arc<RwLock<HashMap<String, Arc<LiveTurn>>>>,
}

/// One running turn: its event log, its end flag, and who is watching.
pub struct LiveTurn {
    /// Every SSE data line so far, in order. `[DONE]` is one of them.
    log: RwLock<Vec<String>>,
    /// Set once by the driver after the last push (and after the row is
    /// written), so a watcher that sees `done` has seen the whole log.
    done: AtomicBool,
    /// Woken on every push and on `done`.
    notify: Notify,
    /// Live watcher streams. Zero means nobody is looking.
    watchers: AtomicUsize,
    /// When the watcher count last hit zero; `None` while someone watches.
    unattended_since: Mutex<Option<Instant>>,
}

impl LiveTurns {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a turn for `chat_id`. Replaces any earlier entry for the chat.
    pub fn start(&self, chat_id: &str) -> Arc<LiveTurn> {
        let turn = Arc::new(LiveTurn {
            log: RwLock::new(Vec::new()),
            done: AtomicBool::new(false),
            notify: Notify::new(),
            watchers: AtomicUsize::new(0),
            unattended_since: Mutex::new(None),
        });
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        guard.insert(chat_id.to_string(), turn.clone());
        turn
    }

    /// The running turn for `chat_id`, if there is one that has not ended.
    pub fn get(&self, chat_id: &str) -> Option<Arc<LiveTurn>> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        guard.get(chat_id).filter(|t| !t.is_done()).cloned()
    }

    /// The driver is done with `turn`: mark it, wake every watcher, and drop
    /// the entry — but only if the entry is still this turn, so a newer turn
    /// on the same chat is left alone.
    pub fn finish(&self, chat_id: &str, turn: &Arc<LiveTurn>) {
        turn.done.store(true, Ordering::SeqCst);
        turn.notify.notify_waiters();
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if guard.get(chat_id).is_some_and(|t| Arc::ptr_eq(t, turn)) {
            guard.remove(chat_id);
        }
    }
}

impl LiveTurn {
    /// Append one SSE data line and wake the watchers.
    pub fn push(&self, data: String) {
        self.log
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .push(data);
        self.notify.notify_waiters();
    }

    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }

    pub fn watchers(&self) -> usize {
        self.watchers.load(Ordering::SeqCst)
    }

    /// Called by the driver as events flow. `true` once nobody has watched
    /// for longer than `cap`; the clock resets whenever a watcher is present.
    pub fn unattended_past(&self, cap: Duration) -> bool {
        let mut since = self.unattended_since.lock().unwrap_or_else(|e| e.into_inner());
        if self.watchers() > 0 {
            *since = None;
            return false;
        }
        match *since {
            None => {
                *since = Some(Instant::now());
                false
            }
            Some(t) => t.elapsed() > cap,
        }
    }
}

/// Everything the turn has said so far, then the rest as it happens, as SSE
/// events. Counts as a watcher for as long as the stream is alive.
pub fn watch(turn: Arc<LiveTurn>) -> impl Stream<Item = Result<SseEvent, Infallible>> + Send {
    turn.watchers.fetch_add(1, Ordering::SeqCst);
    let guard = WatcherGuard(turn.clone());
    async_stream::stream! {
        let _guard = guard;
        let mut next = 0usize;
        loop {
            // Arm the wakeup BEFORE reading, so a push between the read and
            // the await is not a lost wakeup (the Notify idiom).
            let notified = turn.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            let (batch, done) = {
                let log = turn.log.read().unwrap_or_else(|e| e.into_inner());
                (log[next.min(log.len())..].to_vec(), turn.is_done())
            };
            next += batch.len();
            for data in batch {
                yield Ok(SseEvent::default().data(data));
            }
            if done {
                break;
            }
            notified.await;
        }
    }
}

struct WatcherGuard(Arc<LiveTurn>);

impl Drop for WatcherGuard {
    fn drop(&mut self) {
        self.0.watchers.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn a_late_watcher_gets_the_whole_log_then_follows() {
        let turns = LiveTurns::new();
        let turn = turns.start("c1");
        turn.push("a".into());
        turn.push("b".into());

        let mut w = Box::pin(watch(turn.clone()));
        assert_eq!(turn.watchers(), 1);
        // The replay is immediate.
        let first = w.next().await.unwrap().unwrap();
        let second = w.next().await.unwrap().unwrap();
        drop((first, second));

        // Then it follows: a push after subscribing is delivered, and finish
        // ends the stream.
        let t2 = turn.clone();
        let turns2 = turns.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            t2.push("c".into());
            turns2.finish("c1", &t2);
        });
        let third = w.next().await;
        assert!(third.is_some(), "the push after subscribing arrives");
        assert!(w.next().await.is_none(), "finish ends the watch");
        drop(w);
        assert_eq!(turn.watchers(), 0);
        assert!(turns.get("c1").is_none(), "a finished turn is not offered");
    }

    #[test]
    fn the_unattended_clock_only_runs_with_no_watchers() {
        let turns = LiveTurns::new();
        let turn = turns.start("c2");
        // No watcher: the first call starts the clock, not past it yet.
        assert!(!turn.unattended_past(Duration::from_millis(50)));
        std::thread::sleep(Duration::from_millis(80));
        assert!(turn.unattended_past(Duration::from_millis(50)));
        // A watcher arriving resets it.
        turn.watchers.fetch_add(1, Ordering::SeqCst);
        assert!(!turn.unattended_past(Duration::from_millis(50)));
        turn.watchers.fetch_sub(1, Ordering::SeqCst);
        assert!(!turn.unattended_past(Duration::from_millis(50)), "clock restarts from now");
    }

    #[test]
    fn a_newer_turn_is_not_removed_by_the_older_ones_finish() {
        let turns = LiveTurns::new();
        let old = turns.start("c3");
        let new = turns.start("c3");
        turns.finish("c3", &old);
        assert!(turns.get("c3").is_some_and(|t| Arc::ptr_eq(&t, &new)));
    }
}
