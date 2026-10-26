//! An immutable UI handle, bound before Swift can suspend or change accounts.
use dmsg_client::{
    Client,
    host::{Host, Work},
};
use dmsg_types::{Error, Result, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Default)]
struct TransitionGate {
    active: AtomicU64,
    next: AtomicU64,
}

#[derive(Clone)]
pub struct ScopedHost {
    pub inner: Host,
    stamp: u64,
    gate: Arc<TransitionGate>,
}

impl ScopedHost {
    pub fn new(inner: Host, stamp: u64) -> Self {
        Self {
            inner,
            stamp,
            gate: Arc::new(TransitionGate::default()),
        }
    }

    pub fn generation(&self) -> u64 {
        self.stamp
    }

    /// Ordinary snapshots and handles are unavailable during identity changes.
    pub fn ensure_available(&self) -> Result<()> {
        ensure(self.gate.active.load(Ordering::Acquire) == 0, Error::Locked)
    }

    /// Rebind the controller to a completed session, retaining the same gate.
    pub fn scoped(&self, stamp: u64) -> Result<Self> {
        self.ensure_available()?;
        ensure(stamp == self.inner.generation(), Error::Locked)?;
        Ok(Self {
            inner: self.inner.clone(),
            stamp,
            gate: self.gate.clone(),
        })
    }

    /// Exclude ordinary work before fencing old handles. The fence preserves
    /// the login needed by account creation, pairing and binding acceptance.
    pub fn begin_transition(&self) -> Result<Transition> {
        let token = self
            .gate
            .next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| Error::Locked)?
            + 1;
        self.gate
            .active
            .compare_exchange(0, token, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Locked)?;
        let mut transition = Transition {
            inner: self.inner.clone(),
            gate: self.gate.clone(),
            token,
            generation: self.stamp,
            fenced: false,
            completed: false,
        };
        transition.generation = self.inner.fence_if(self.stamp)?;
        transition.fenced = true;
        Ok(transition)
    }

    pub async fn run<T: Send + 'static>(
        &self,
        work: impl for<'a> FnOnce(&'a mut Client) -> Work<'a, T> + Send + 'static,
    ) -> Result<T> {
        self.run_at(self.stamp, work).await
    }

    pub async fn run_at<T: Send + 'static>(
        &self,
        stamp: u64,
        work: impl for<'a> FnOnce(&'a mut Client) -> Work<'a, T> + Send + 'static,
    ) -> Result<T> {
        ensure(stamp == self.stamp, Error::Locked)?;
        self.ensure_available()?;
        let gate = self.gate.clone();
        let result = self
            .inner
            .run_at(self.stamp, move |client| {
                Box::pin(async move {
                    ensure(gate.active.load(Ordering::Acquire) == 0, Error::Locked)?;
                    work(client).await
                })
            })
            .await;
        self.ensure_available()?;
        result
    }
}

/// One identity transition. No client borrow or mutex survives an await.
/// Dropping before `finish` fences its outstanding work, then releases only its
/// own gate token; lifecycle controls can still invalidate it immediately.
pub struct Transition {
    inner: Host,
    gate: Arc<TransitionGate>,
    token: u64,
    generation: u64,
    fenced: bool,
    completed: bool,
}

impl Transition {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub async fn run<T: Send + 'static>(
        &self,
        work: impl for<'a> FnOnce(&'a mut Client) -> Work<'a, T> + Send + 'static,
    ) -> Result<T> {
        ensure(
            self.gate.active.load(Ordering::Acquire) == self.token,
            Error::Locked,
        )?;
        self.inner.run_at(self.generation, work).await
    }

    /// Fence even the private intermediate generation before releasing the
    /// gate. A snapshot that raced the first gate check cannot mint a handle
    /// after the entire transition completed between its two checks.
    pub fn finish(mut self) -> Result<()> {
        ensure(
            self.inner.generation() == self.generation
                && self.gate.active.load(Ordering::Acquire) == self.token,
            Error::Locked,
        )?;
        self.inner.fence_if(self.generation)?;
        self.completed = true;
        self.release();
        Ok(())
    }

    fn release(&self) {
        let _ =
            self.gate
                .active
                .compare_exchange(self.token, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}

impl Drop for Transition {
    fn drop(&mut self) {
        if self.fenced && !self.completed && self.gate.active.load(Ordering::Acquire) == self.token
        {
            // Do not invalidate a newer lifecycle generation or another
            // transition. Fencing also prevents already queued late results.
            let _ = self.inner.fence_if(self.generation);
        }
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn a_delayed_handle_cannot_start_work_in_a_replacement_generation() {
        let dir = dmsg_client::testing::TempDir::new();
        let client = dmsg_client::testing::unlocked(&dir.0, false, None);
        let host = Host::start(client, &tokio::runtime::Handle::current());
        let old = ScopedHost::new(host.clone(), host.generation());
        assert!(old.run(|c| Box::pin(async { c.status() })).await.is_ok());
        host.pause();
        host.resume_if(host.generation()).unwrap();
        // Even an operation first polled now still belongs to the old UI.
        assert!(matches!(
            old.run(|_| Box::pin(async { Ok(()) })).await,
            Err(Error::Locked)
        ));
        let new = ScopedHost::new(host.clone(), host.generation());
        assert_eq!(
            new.run(|c| Box::pin(async { Ok(c.session_state().state) }))
                .await
                .unwrap(),
            "unlocked"
        );
        host.close();
        host.closed().await.unwrap();
    }

    #[tokio::test]
    async fn transition_blocks_handles_snapshots_and_a_future_first_polled_after_completion() {
        use std::sync::atomic::AtomicBool;
        let dir = dmsg_client::testing::TempDir::new();
        let client = dmsg_client::testing::unlocked(&dir.0, false, None);
        let host = Host::start(client, &tokio::runtime::Handle::current());
        let owner = ScopedHost::new(host.clone(), host.generation());
        let old = owner.scoped(host.generation()).unwrap();
        let ran = Arc::new(AtomicBool::new(false));
        let marker = ran.clone();
        let delayed = old.run(move |_| {
            Box::pin(async move {
                marker.store(true, Ordering::SeqCst);
                Ok(())
            })
        });

        let transition = owner.begin_transition().unwrap();
        let intermediate = transition.generation();
        assert!(matches!(owner.ensure_available(), Err(Error::Locked)));
        assert!(matches!(
            owner.scoped(transition.generation()),
            Err(Error::Locked)
        ));
        assert!(matches!(
            old.run(|_| Box::pin(async { Ok(()) })).await,
            Err(Error::Locked)
        ));
        // An unrelated rejected attempt must not release the first transition.
        assert!(matches!(owner.begin_transition(), Err(Error::Locked)));
        assert!(matches!(owner.ensure_available(), Err(Error::Locked)));
        // The private transition can read/write at its one fixed generation;
        // the gate does not lock the Client or discard its existing keys.
        assert_eq!(
            transition
                .run(|c| Box::pin(async { Ok(c.session_state().state) }))
                .await
                .unwrap(),
            "unlocked"
        );
        transition.finish().unwrap();

        // Even a session snapshot read at the intermediate stamp and delivered
        // only after the gate reopened cannot create an authorized UI handle.
        assert!(matches!(owner.scoped(intermediate), Err(Error::Locked)));
        assert!(matches!(delayed.await, Err(Error::Locked)));
        assert!(!ran.load(Ordering::SeqCst));
        let completed_generation = host.generation();
        assert!(matches!(old.begin_transition(), Err(Error::Locked)));
        assert_eq!(host.generation(), completed_generation);
        assert!(owner.ensure_available().is_ok());
        let fresh = owner.scoped(host.generation()).unwrap();
        assert_eq!(
            fresh
                .run(|c| Box::pin(async { Ok(c.session_state().state) }))
                .await
                .unwrap(),
            "unlocked"
        );
        host.close();
        host.closed().await.unwrap();
    }

    #[tokio::test]
    async fn dropping_an_unfinished_transition_releases_its_gate_and_fences_queued_work() {
        let dir = dmsg_client::testing::TempDir::new();
        let client = dmsg_client::testing::unlocked(&dir.0, false, None);
        let host = Host::start(client, &tokio::runtime::Handle::current());
        let owner = ScopedHost::new(host.clone(), host.generation());
        let transition = owner.begin_transition().unwrap();
        let obsolete = transition.generation();
        drop(transition);
        assert!(owner.ensure_available().is_ok());
        assert_ne!(host.generation(), obsolete);
        assert!(matches!(
            host.run_at(obsolete, |_| Box::pin(async { Ok(()) })).await,
            Err(Error::Locked)
        ));
        let fresh = owner.scoped(host.generation()).unwrap();
        let next = fresh.begin_transition().unwrap();
        assert!(matches!(owner.ensure_available(), Err(Error::Locked)));
        next.finish().unwrap();
        assert!(owner.ensure_available().is_ok());
        host.close();
        host.closed().await.unwrap();
    }

    #[tokio::test]
    async fn lifecycle_pause_invalidates_transition_without_being_resumed_by_cleanup() {
        let dir = dmsg_client::testing::TempDir::new();
        let client = dmsg_client::testing::unlocked(&dir.0, false, None);
        let host = Host::start(client, &tokio::runtime::Handle::current());
        let owner = ScopedHost::new(host.clone(), host.generation());
        let transition = owner.begin_transition().unwrap();
        host.pause();
        let paused_generation = host.generation();
        assert!(matches!(
            transition.run(|_| Box::pin(async { Ok(()) })).await,
            Err(Error::Locked)
        ));
        assert!(matches!(transition.finish(), Err(Error::Locked)));
        assert_eq!(host.generation(), paused_generation);
        assert!(owner.ensure_available().is_ok());
        let fresh = owner.scoped(paused_generation).unwrap();
        assert!(matches!(
            fresh.run(|_| Box::pin(async { Ok(()) })).await,
            Err(Error::Locked)
        ));
        host.resume_if(paused_generation).unwrap();
        assert!(fresh.run(|_| Box::pin(async { Ok(()) })).await.is_ok());
        host.close();
        host.closed().await.unwrap();
    }
}
