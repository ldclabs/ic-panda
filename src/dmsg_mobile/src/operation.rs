use crate::error::{MobileError, Result};
use std::{
    future::Future,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

#[derive(Clone, Default, uniffi::Record)]
pub struct TransferProgress {
    pub completed: u64,
    pub total: u64,
}

/// A cancellation signal for one UI operation; contains no authorization.
#[derive(uniffi::Object)]
pub struct OperationToken {
    cancelled: watch::Sender<bool>,
    progress: Mutex<TransferProgress>,
}

#[uniffi::export]
impl OperationToken {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: watch::channel(false).0,
            progress: Mutex::new(TransferProgress::default()),
        })
    }

    pub fn progress(&self) -> TransferProgress {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn cancel(&self) {
        self.cancelled.send_replace(true);
    }
}

impl OperationToken {
    pub(crate) fn report(&self, completed: u64, total: u64) {
        *self.progress.lock().unwrap_or_else(|e| e.into_inner()) =
            TransferProgress { completed, total };
    }

    pub async fn run<T>(&self, work: impl Future<Output = Result<T>>) -> Result<T> {
        let mut cancelled = self.cancelled.subscribe();
        if *cancelled.borrow() {
            return Err(MobileError::Failure {
                code: "CANCELLED".into(),
            });
        }
        tokio::select! {
            biased;
            _ = cancelled.changed() => Err(MobileError::Failure { code: "CANCELLED".into() }),
            result = work => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn cancellation_does_not_poll_unstarted_work() {
        let operation = OperationToken::new();
        operation.cancel();
        let ran = AtomicBool::new(false);
        let result = operation
            .run(async {
                ran.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await;
        assert!(matches!(result, Err(MobileError::Failure { code }) if code == "CANCELLED"));
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn cancellation_wakes_a_waiting_operation_without_cancelling_another() {
        let operation = OperationToken::new();
        let running = operation.clone();
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            running
                .run(async {
                    let _ = entered.send(());
                    std::future::pending::<Result<()>>().await
                })
                .await
        });
        waiting.await.unwrap();
        operation.cancel();
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(MobileError::Failure { code }) if code == "CANCELLED"));
        assert_eq!(OperationToken::new().run(async { Ok(7) }).await.unwrap(), 7);
    }
}
