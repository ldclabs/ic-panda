//! An immutable UI handle, bound before Swift can suspend or change accounts.
use dmsg_client::{
    Client,
    host::{Host, Work},
};
use dmsg_types::{Error, Result, ensure};

#[derive(Clone)]
pub struct ScopedHost {
    pub inner: Host,
    stamp: u64,
}

impl ScopedHost {
    pub fn new(inner: Host, stamp: u64) -> Self {
        Self { inner, stamp }
    }

    pub fn generation(&self) -> u64 {
        self.stamp
    }

    pub async fn run<T: Send + 'static>(
        &self,
        work: impl for<'a> FnOnce(&'a mut Client) -> Work<'a, T> + Send + 'static,
    ) -> Result<T> {
        self.inner.run_at(self.stamp, work).await
    }

    pub async fn run_at<T: Send + 'static>(
        &self,
        stamp: u64,
        work: impl for<'a> FnOnce(&'a mut Client) -> Work<'a, T> + Send + 'static,
    ) -> Result<T> {
        ensure(stamp == self.stamp, Error::Locked)?;
        self.inner.run_at(self.stamp, work).await
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
        host.resume();
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
}
