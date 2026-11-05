//! The hint channel while the app is open and unlocked: what moved, never its
//! content. Nothing listens in the background; the next foreground catches up.
use super::*;
use dmsg_client::channel::Reading;
use dmsg_client::diagnostic::Diagnostic;
use dmsg_client::hint_channel::{self, Connection};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::Notify;

/// Told of the connection and its hints, from a background thread.
#[uniffi::export(callback_interface)]
pub trait HintObserver: Send + Sync {
    fn connected(&self, connected: bool);
    /// `news`: a `content` hint this device has not read yet.
    fn hint(&self, kind: String, resource: Option<String>, news: bool);
}

/// One connection, until it drops.
async fn listen<H: Hold>(
    h: &H,
    origin: &str,
    redial: &Notify,
    signal: &sync::SyncSignal,
    observer: &dyn HintObserver,
) -> std::result::Result<(), Diagnostic> {
    let failure = |error: Error| Diagnostic::error(&error);
    let token = dmsg_client::connect::hint_token(h).await.map_err(failure)?;
    let account = h
        .with(|c| Ok(c.session_state().account))
        .await
        .map_err(failure)?
        .ok_or_else(|| failure(Error::AuthRequired))?;
    let url = hint_channel::url(origin, &account);
    let mut connection = Connection::open(&url, &token.token).await?;
    signal.hinted.store(true, Ordering::Relaxed);
    observer.connected(true);
    loop {
        tokio::select! {
            // A socket from before a network change is likely dead.
            _ = redial.notified() => return Ok(()),
            hint = connection.next() => {
                let Some(hint) = hint? else { return Ok(()) };
                if hint.account != account {
                    continue;
                }
                let news = hint.kind == "content"
                    && h.with(|c| Ok(dmsg_client::sync::hint_is_news(c, hint.cursor)))
                        .await
                        .map_err(failure)?;
                observer.hint(hint.kind, hint.resource, news);
            }
        }
    }
}

#[uniffi::export]
impl MobileEngine {
    /// Keep this session's hint channel connected until the operation ends,
    /// the session changes or the app leaves the foreground.
    pub async fn listen_hints(
        &self,
        observer: Box<dyn HintObserver>,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        let origin = self.config.relay_origin();
        let redial = self.hints_redial.clone();
        let signal = self.sync_signal.clone();
        operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    let mut backoff = 1;
                    loop {
                        let result = listen(h, &origin, &redial, &signal, observer.as_ref()).await;
                        signal.hinted.store(false, Ordering::Relaxed);
                        observer.connected(false);
                        backoff = match result {
                            Ok(()) => 1,
                            Err(error) => {
                                error.emit();
                                (backoff * 2).min(60)
                            }
                        };
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(backoff)) => {},
                            _ = redial.notified() => {},
                        }
                    }
                })
            }))
            .await
    }

    /// What a channel hint asks for: the channel's controls and how far its
    /// messages reach, for the list; only an open conversation reads bodies.
    pub async fn read_channel_controls(
        &self,
        channel: String,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        let turn = self.read_turns.lane(&format!("channel:{channel}"));
        let control = self.read_turns.lane(&format!("channel-control:{channel}"));
        operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    {
                        let _turn = turn.lock().await;
                        dmsg_client::channel::read(h, &mut Reading::controls(&channel)).await?;
                    }
                    maintain_channel(h, &control, &channel).await;
                    Ok(())
                })
            }))
            .await
    }

    /// A channel with news, read ahead for the list on screen: at most its
    /// latest page. The app paces it.
    pub async fn read_channel_tail(
        &self,
        channel: String,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        let turn = self.read_turns.lane(&format!("channel:{channel}"));
        operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    dmsg_client::channel::sync_tail(h, &channel).await?;
                    Ok(())
                })
            }))
            .await
    }

    /// The read position follows to this account's other devices.
    pub async fn report_channel_cursor(
        &self,
        channel: String,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    dmsg_client::channel::report_cursor(h, &channel, None).await?;
                    Ok(())
                })
            }))
            .await
    }
}

/// While the app is open it does what the daemon's channel loop does: report
/// how far this device got and rotate an epoch that waits for it. Both are
/// tried again with the next read.
pub(crate) async fn maintain_channel<H: Hold>(
    h: &H,
    control: &tokio::sync::Mutex<()>,
    channel: &str,
) {
    let _ = dmsg_client::channel::report_cursor(h, channel, None).await;
    if h.with(|c| c.channel_rotation_due(channel)).await == Ok(true)
        && let Ok(_turn) = control.try_lock()
    {
        let _ = dmsg_client::channel::rotate(h, channel).await;
    }
}
