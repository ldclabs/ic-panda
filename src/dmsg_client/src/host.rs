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
use tokio::sync::{Mutex, mpsc, oneshot, watch};

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

struct Hosted {
    client: Option<Client>,
    generation: u64,
}

impl Hosted {
    fn apply(&mut self, control: Control) {
        if self.generation == control.generation {
            return;
        }
        if let Some(client) = self.client.as_mut() {
            client.abandon_host_work();
            if control.lock_generation > self.generation {
                client.lock();
            }
            if control.close {
                client.shutdown();
            }
        }
        self.generation = control.generation;
        if control.close {
            self.client = None;
        }
    }
}

/// A running client's command queue. Its task runs on the host's runtime.
#[derive(Clone)]
pub struct Host {
    queue: mpsc::Sender<Command>,
    client: Arc<Mutex<Hosted>>,
    locked: watch::Sender<Control>,
    generation: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    stopped: watch::Receiver<bool>,
}

impl Host {
    /// Start on an explicit runtime with an already opened client.
    pub fn start(client: Client, runtime: &tokio::runtime::Handle) -> Self {
        let (queue, mut receiver) = mpsc::channel::<Command>(32);
        let (locked, mut changes) = watch::channel(Control::default());
        let generation = Arc::new(AtomicU64::new(0));
        let (stopped_sender, stopped) = watch::channel(false);
        let client = Arc::new(Mutex::new(Hosted {
            client: Some(client),
            generation: 0,
        }));
        let owned = client.clone();
        runtime.spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    changed = changes.changed() => {
                        if changed.is_err() { break; }
                        let control = *changes.borrow_and_update();
                        owned.lock().await.apply(control);
                        if control.close { break; }
                    }
                    command = receiver.recv() => {
                        let Some(command) = command else { break; };
                        let mut state = owned.lock().await;
                        let control = *changes.borrow_and_update();
                        state.apply(control);
                        if control.close { break; }
                        if command.generation != state.generation { continue; }
                        let Some(client) = state.client.as_mut() else { break; };
                        // Only legacy whole-client operations hold this guard
                        // while waiting. Migrated Hold steps take it synchronously.
                        let cancelled = {
                            let work = (command.job)(client);
                            tokio::select! {
                                biased;
                                _ = changes.changed() => true,
                                result = work => result.is_err(),
                            }
                        };
                        if cancelled {
                            if let Some(client) = state.client.as_mut() { client.abandon_host_work(); }
                            let control = *changes.borrow_and_update();
                            state.apply(control);
                            if control.close { break; }
                        }
                    }
                }
            }
            if let Some(mut client) = owned.lock().await.client.take() { client.shutdown(); }
            stopped_sender.send_replace(true);
        });
        Self {
            queue,
            client,
            locked,
            generation,
            paused: Arc::new(AtomicBool::new(false)),
            stopped,
        }
    }

    /// A short shared step. The control barrier is applied under the same lock
    /// before a new-generation step can observe the client.
    pub async fn with_at<T: Send>(
        &self,
        generation: u64,
        step: impl FnOnce(&mut Client) -> Result<T> + Send,
    ) -> Result<T> {
        let result = {
            let mut state = self.client.lock().await;
            let control = *self.locked.borrow();
            state.apply(control);
            dmsg_types::ensure(
                generation == control.generation
                    && !control.close
                    && !self.paused.load(Ordering::Acquire),
                Error::Locked,
            )?;
            step(state.client.as_mut().ok_or(Error::Locked)?)
        };
        dmsg_types::ensure(generation == self.generation(), Error::Locked)?;
        result
    }

    /// Cancel a multi-step workflow even while it is outside the client.
    pub async fn while_current<T>(
        &self,
        generation: u64,
        work: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        let mut changes = self.locked.subscribe();
        dmsg_types::ensure(
            generation == self.generation()
                && !self.paused.load(Ordering::Acquire)
                && !changes.borrow().close,
            Error::Locked,
        )?;
        tokio::select! {
            biased;
            _ = changes.changed() => Err(Error::Locked),
            result = work => {
                dmsg_types::ensure(generation == self.generation(), Error::Locked)?;
                result
            }
        }
    }

    /// Generation carried by every UI snapshot, never reused after lock.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Invalidate queued/in-flight operations immediately. The next command
    /// acts as a barrier after the worker has discarded keys.
    pub fn lock(&self) {
        self.control(true, false, false);
    }

    /// Fence work during a host-controlled identity transition without losing
    /// the login needed to complete it. Never clears a lifecycle pause or close.
    ///
    /// # Errors
    /// Returns `Locked` when the expected generation is no longer current.
    pub fn fence_if(&self, expected: u64) -> Result<u64> {
        let mut next = None;
        self.locked.send_if_modified(|control| {
            if control.generation != expected || control.close {
                return false;
            }
            control.generation += 1;
            self.generation.store(control.generation, Ordering::Release);
            next = Some(control.generation);
            true
        });
        next.ok_or(Error::Locked)
    }

    /// Stop all work while retaining keys only for the host's background grace.
    pub fn pause(&self) {
        self.control(false, false, true);
    }

    /// Permit foreground work again, but only in the lifecycle generation
    /// observed before an async call; no automatic unlock takes place. A late
    /// foreground task cannot undo a newer background pause.
    ///
    /// # Errors
    /// Returns `Locked` after another lifecycle event or closure.
    pub fn resume_if(&self, expected: u64) -> Result<()> {
        let mut resumed = false;
        self.locked.send_if_modified(|control| {
            if control.generation == expected && !control.close {
                self.paused.store(false, Ordering::Release);
                resumed = true;
            }
            false
        });
        if resumed { Ok(()) } else { Err(Error::Locked) }
    }

    /// Cancel work, clear keys and close the database on its owning executor.
    pub fn close(&self) {
        self.control(true, true, true);
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

    fn control(&self, lock: bool, close: bool, pause: bool) {
        self.locked.send_modify(|control| {
            if pause {
                self.paused.store(true, Ordering::Release);
            }
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
    async fn stale_foreground_cannot_resume_a_new_pause_and_lock_preserves_pause() {
        let dir = TempDir::new();
        let client = crate::testing::unlocked(&dir.0, false, None);
        let host = Host::start(client, &tokio::runtime::Handle::current());
        let foreground = host.generation();
        host.pause();
        assert_eq!(host.resume_if(foreground), Err(Error::Locked));
        host.lock();
        assert_eq!(
            host.run(|_| Box::pin(async { Ok(()) })).await,
            Err(Error::Locked)
        );
        host.resume_if(host.generation()).unwrap();
        assert!(
            host.run(|c| Box::pin(async { Ok(c.keys().is_err()) }))
                .await
                .unwrap()
        );
        host.close();
        host.closed().await.unwrap();
    }

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
        assert_eq!(host.fence_if(old), Err(Error::Locked));
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
    #[tokio::test]
    async fn shared_network_wait_does_not_block_local_reads_and_lock_cancels_it() {
        let dir = TempDir::new();
        let client = crate::testing::unlocked(&dir.0, false, None);
        let host = Host::start(client, &tokio::runtime::Handle::current());
        let generation = host.generation();
        let running = host.clone();
        let (entered, waiting) = oneshot::channel();
        let task = tokio::spawn(async move {
            running
                .while_current(generation, async {
                    running.with_at(generation, |c| c.status()).await?;
                    let _ = entered.send(());
                    std::future::pending::<Result<()>>().await
                })
                .await
        });
        waiting.await.unwrap();
        let state = tokio::time::timeout(
            Duration::from_millis(200),
            host.with_at(generation, |c| c.status()),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(state.state, "unlocked");
        host.lock();
        assert_eq!(task.await.unwrap(), Err(Error::Locked));
        assert!(
            host.with_at(host.generation(), |c| Ok(c.keys().is_err()))
                .await
                .unwrap()
        );
        host.close();
        host.closed().await.unwrap();
    }
}
