//! A cancellable owner for hosts whose process can suspend at any time.
//!
//! The worker owns `Client`, never a mutex guard across a network wait.
//! Lock invalidates queued work and drops the current future before clearing
//! keys. Existing account/root/approval journals remain the authority after
//! cancellation. Ordinary operations use `Hold` where available; legacy
//! `&mut Client` paths are serialized here until converted to short steps.

use crate::Client;
use dmsg_types::{Error, Result};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{mpsc, oneshot, watch};

/// A bounded operation on the exclusively owned client.
pub type Work<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;
type Job = Box<dyn for<'a> FnOnce(&'a mut Client) -> Work<'a, ()> + Send>;
struct Command {
    generation: u64,
    job: Job,
}
#[derive(Clone, Copy, Default)]
struct Control {
    generation: u64,
    lock_generation: u64,
    close: bool,
}

/// A running client's command queue. Its task runs on the host's runtime.
#[derive(Clone)]
pub struct Host {
    queue: mpsc::Sender<Command>,
    locked: watch::Sender<Control>,
    generation: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    stopped: watch::Receiver<bool>,
}

impl Host {
    /// Start on an explicit runtime with an already opened client.
    pub fn start(mut client: Client, runtime: &tokio::runtime::Handle) -> Self {
        let (queue, mut receiver) = mpsc::channel::<Command>(32);
        let (locked, mut changes) = watch::channel(Control::default());
        let generation = Arc::new(AtomicU64::new(0));
        let (stopped_sender, stopped) = watch::channel(false);
        runtime.spawn(async move {
            let mut current = 0;
            loop {
                tokio::select! {
                    biased;
                    changed = changes.changed() => {
                        if changed.is_err() { break; }
                        let control = *changes.borrow_and_update();
                        client.abandon_host_work();
                        if control.lock_generation > current { client.lock(); }
                        current = control.generation;
                        if control.close { break; }
                    }
                    command = receiver.recv() => {
                        let Some(command) = command else { break; };
                        // A control may arrive between select polling its two
                        // branches. Apply it before accepting a new-generation job.
                        let control = *changes.borrow_and_update();
                        if control.generation != current {
                            client.abandon_host_work();
                            if control.lock_generation > current { client.lock(); }
                            current = control.generation;
                            if control.close { break; }
                        }
                        if command.generation != current { continue; }
                        // Drop the operation (and its borrow) before locking.
                        let cancelled = {
                            let work = (command.job)(&mut client);
                            tokio::select! {
                                biased;
                                _ = changes.changed() => true,
                                result = work => result.is_err(),
                            }
                        };
                        if cancelled {
                            client.abandon_host_work();
                            let control = *changes.borrow_and_update();
                            if control.lock_generation > current { client.lock(); }
                            current = control.generation;
                            if control.close { break; }
                        }
                    }
                }
            }
            client.shutdown();
            drop(client);
            stopped_sender.send_replace(true);
        });
        Self {
            queue,
            locked,
            generation,
            paused: Arc::new(AtomicBool::new(false)),
            stopped,
        }
    }

    /// Generation carried by every UI snapshot, never reused after lock.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Invalidate queued/in-flight operations immediately. The next command
    /// acts as a barrier after the worker has discarded keys.
    pub fn lock(&self) {
        self.control(true, false);
    }

    /// A user-initiated logout/switch belongs to the UI session that requested it.
    /// # Errors
    /// Returns `Locked` without affecting a replacement session.
    pub fn lock_if(&self, expected: u64) -> Result<()> {
        let mut accepted = false;
        self.locked.send_if_modified(|control| {
            if control.generation != expected {
                return false;
            }
            control.generation += 1;
            control.lock_generation = control.generation;
            self.generation.store(control.generation, Ordering::Release);
            accepted = true;
            true
        });
        if accepted { Ok(()) } else { Err(Error::Locked) }
    }

    /// Stop all work while retaining keys only for the host's background grace.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
        self.control(false, false);
    }

    /// Permit foreground work again; no automatic unlock takes place.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
    }

    /// Cancel work, clear keys and close the database on its owning executor.
    pub fn close(&self) {
        self.paused.store(true, Ordering::Release);
        self.control(true, true);
    }

    /// Wait until the worker has dropped its keys and SQLite connection.
    ///
    /// # Errors
    /// Returns `Unavailable` if the worker failed before completing shutdown.
    pub async fn closed(&self) -> Result<()> {
        let mut stopped = self.stopped.clone();
        stopped
            .wait_for(|stopped| *stopped)
            .await
            .map(|_| ())
            .map_err(|_| Error::Unavailable("client host stopped".into()))
    }

    fn control(&self, lock: bool, close: bool) {
        self.locked.send_modify(|control| {
            control.generation += 1;
            // A following pause must never erase an unprocessed lock/close.
            if lock {
                control.lock_generation = control.generation;
            }
            control.close |= close;
            self.generation.store(control.generation, Ordering::Release);
        });
    }

    /// Run a typed operation, rejecting an answer invalidated by lock.
    ///
    /// # Errors
    /// Returns `Locked` on cancellation, or the operation's own error.
    pub async fn run<T: Send + 'static>(
        &self,
        work: impl for<'a> FnOnce(&'a mut Client) -> Work<'a, T> + Send + 'static,
    ) -> Result<T> {
        let generation = self.generation();
        self.run_at(generation, work).await
    }

    /// Continue a two-stage operation only in the session it started in.
    ///
    /// # Errors
    /// Returns `Locked` when an intervening lock invalidated the operation.
    pub async fn run_at<T: Send + 'static>(
        &self,
        generation: u64,
        work: impl for<'a> FnOnce(&'a mut Client) -> Work<'a, T> + Send + 'static,
    ) -> Result<T> {
        if generation != self.generation() || self.paused.load(Ordering::Acquire) {
            return Err(Error::Locked);
        }
        let (mut reply, result) = oneshot::channel();
        self.queue
            .send(Command {
                generation,
                job: Box::new(move |client| {
                    Box::pin(async move {
                        let value = tokio::select! {
                                    biased;
                        _ = reply.closed() => return Err(Error::Locked),
                                    value = work(client) => value,
                                };
                        let _ = reply.send(value);
                        Ok(())
                    })
                }),
            })
            .await
            .map_err(|_| Error::Locked)?;
        let result = result.await.map_err(|_| Error::Locked)?;
        if generation != self.generation() {
            return Err(Error::Locked);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempDir, config};
    use std::time::Duration;

    #[tokio::test]
    async fn lock_cancels_a_network_wait_and_rejects_queued_old_work() {
        let dir = TempDir::new();
        let client = Client::open(config(false), &dir.0, None).unwrap();
        let host = Host::start(client, &tokio::runtime::Handle::current());
        let (entered, waiting) = oneshot::channel();
        let pending = host.clone();
        let work = tokio::spawn(async move {
            pending
                .run(move |client| {
                    Box::pin(async move {
                        client.running.insert("cancelled-request".into(), 0);
                        let _ = entered.send(());
                        std::future::pending::<Result<()>>().await
                    })
                })
                .await
        });
        waiting.await.unwrap();
        let old = host.generation();
        host.lock();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), work)
                .await
                .unwrap()
                .unwrap(),
            Err(Error::Locked)
        );
        assert_eq!(
            host.run_at(old, |_| Box::pin(async { Ok(()) })).await,
            Err(Error::Locked)
        );
        let state = host
            .run(|c| Box::pin(async { Ok(c.session_state()) }))
            .await
            .unwrap();
        assert_ne!(state.state, "unlocked");
        assert!(
            host.run(|c| Box::pin(async { Ok(c.running.is_empty()) }))
                .await
                .unwrap()
        );
        assert_eq!(host.lock_if(old), Err(Error::Locked));
        host.close();
        host.closed().await.unwrap();
    }

    #[tokio::test]
    async fn concurrent_lock_signals_never_regress_generation() {
        let dir = TempDir::new();
        let client = Client::open(config(false), &dir.0, None).unwrap();
        let host = Host::start(client, &tokio::runtime::Handle::current());
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let host = &host;
                scope.spawn(move || {
                    for _ in 0..50 {
                        host.lock();
                    }
                });
            }
        });
        assert_eq!(host.generation(), 400);
        assert!(host.run(|c| Box::pin(async { c.status() })).await.is_ok());
    }

    #[tokio::test]
    async fn dropping_one_waiter_abandons_its_live_marker_without_locking_the_session() {
        let dir = TempDir::new();
        let client = crate::testing::unlocked(&dir.0, false, None);
        let host = Host::start(client, &tokio::runtime::Handle::current());
        let pending = host.clone();
        let (entered, waiting) = oneshot::channel();
        let work = tokio::spawn(async move {
            pending
                .run(|c| {
                    Box::pin(async move {
                        c.running.insert("aborted".into(), 0);
                        let _ = entered.send(());
                        std::future::pending::<Result<()>>().await
                    })
                })
                .await
        });
        waiting.await.unwrap();
        work.abort();
        let _ = work.await;
        let state = tokio::time::timeout(
            Duration::from_secs(1),
            host.run(|c| {
                Box::pin(async {
                    Ok((c.running.is_empty(), c.session_state().state == "unlocked"))
                })
            }),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(state, (true, true));
        host.close();
        host.closed().await.unwrap();
    }
}
