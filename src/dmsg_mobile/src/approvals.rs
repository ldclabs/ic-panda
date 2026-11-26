use super::*;
use dmsg_client::approval::{Ask, Decision};
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct ApprovalExpiry {
    wall: u64,
    monotonic: Instant,
}

impl ApprovalExpiry {
    fn at(expires: u64, wall: u64, monotonic: Instant) -> dmsg_types::Result<Self> {
        let remaining = expires
            .checked_sub(wall)
            .filter(|ms| *ms > 0)
            .ok_or(Error::Expired)?;
        Ok(Self {
            wall: expires,
            monotonic: monotonic + Duration::from_millis(remaining.min(90_000)),
        })
    }

    fn check_at(&self, wall: u64, monotonic: Instant) -> dmsg_types::Result<()> {
        ensure(
            wall < self.wall && monotonic < self.monotonic,
            Error::Expired,
        )
    }

    fn check(&self) -> dmsg_types::Result<()> {
        self.check_at(dmsg_client::now_ms(), Instant::now())
    }
}

/// Keep decision serialization until even cancellation cleanup has completed.
/// A delayed old cleanup must never remove the next attempt's live marker.
struct RunningApproval {
    host: ScopedHost,
    id: String,
    turn: Option<tokio::sync::OwnedMutexGuard<()>>,
}

impl Drop for RunningApproval {
    fn drop(&mut self) {
        let host = self.host.clone();
        let id = self.id.clone();
        let turn = self.turn.take();
        runtime().spawn(async move {
            let _turn = turn;
            let _ = host
                .inner
                .with_at(host.generation(), |c| {
                    c.abandon_approval_runtime(&id);
                    Ok(())
                })
                .await;
        });
    }
}

fn supported(view: &Value) -> bool {
    match view["type"].as_str() {
        Some(
            "device_pair"
            | "device_transfer"
            | "vault_backup"
            | "device_revoke"
            | "device_capabilities"
            | "attest"
            | "account"
            | "principal"
            | "approval_key"
            | "approval_key_retire"
            | "hardware_policy"
            | "secret_share"
            | "secret_package_import",
        ) => true,
        Some("wallet") => matches!(
            view["detail"]["action"].as_str(),
            Some("join" | "unbind" | "decide" | "cosign")
        ),
        Some("grant" | "delegation") => true,
        Some("connect") => matches!(
            view["detail"]["method"].as_str(),
            Some("authenticate" | "signDocument" | "signAction")
        ),
        _ => false,
    }
}

fn annotate(mut view: Value) -> Value {
    view["mobile_supported"] = supported(&view).into();
    view
}

#[uniffi::export]
impl MobileEngine {
    pub async fn approvals(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                let views = dmsg_client::approval::list(&c.cooperative_hold(), false).await?;
                Ok(views.into_iter().map(annotate).collect::<Vec<_>>())
            })
        })
        .await
    }

    pub async fn approval(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.approval(&id).map(annotate) }))
            .await
    }

    pub async fn decide(
        &self,
        id: String,
        digest: String,
        approve: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let result = operation
            .run(self.decide_reviewed(id, digest, approve))
            .await;
        if approve {
            let _ = self
                .held(|h| {
                    Box::pin(async {
                        h.with(|c| c.cache_invalidate(dmsg_client::cache::KINDS))
                            .await
                    })
                })
                .await;
        }
        result
    }
}

impl MobileEngine {
    async fn decide_reviewed(
        &self,
        id: String,
        digest: String,
        approve: bool,
    ) -> Result<ObjectSnapshot> {
        // Serialize decisions and their reconciliation, not Client access.
        let turn = self.approval_turn.clone().lock_owned().await;
        let _running = RunningApproval {
            host: self.host.clone(),
            id: id.clone(),
            turn: Some(turn),
        };
        let generation = self.host.generation();
        let (read_id, shown) = (id.clone(), digest.clone());
        if approve {
            let login_id = id.clone();
            self.held(move |h| {
                Box::pin(async move {
                    if h.with(|c| c.approval_needs_login(&login_id)).await? {
                        h.ensure_login().await?;
                    }
                    Ok(())
                })
            })
            .await?;
        }
        let (ask, expires_at, prepare_attestation, prepare_account, prepare_connect) = self
            .held(move |h| {
                Box::pin(async move {
                    let validate = |view: &Value| -> dmsg_types::Result<()> {
                        if approve {
                            ensure(supported(view), Error::UnsupportedProtocol)?;
                        }
                        ensure(
                            view["digest"].as_str() == Some(shown.as_str()),
                            Error::IntegrityFailed,
                        )?;
                        ensure(view["state"] == "awaiting_user", Error::Expired)
                    };
                    let refresh_keys = h
                        .with(|c| {
                            let view = c.approval(&read_id)?;
                            validate(&view)?;
                            Ok(approve
                                && !matches!(
                                    view["type"].as_str(),
                                    Some("secret_share" | "secret_package_import" | "vault_backup")
                                ))
                        })
                        .await?;
                    if refresh_keys {
                        // Network waits release Client, then the request is checked again.
                        dmsg_client::approval_key::status(h).await?;
                    }
                    h.with(|c| {
                        let view = c.approval(&read_id)?;
                        validate(&view)?;
                        let ask = if approve {
                            c.approval_presence(&read_id)?
                        } else {
                            None
                        };
                        let hardware = matches!(ask, Some((Ask::Hardware(..), _)));
                        let prepare_attestation = view["type"] == "attest" && hardware;
                        let prepare_account =
                            hardware && c.has_prepared_account_approval(&read_id)?;
                        let prepare_connect = hardware && view["type"] == "connect";
                        ensure(
                            !hardware || prepare_attestation || prepare_account || prepare_connect,
                            Error::UnsupportedProtocol,
                        )?;
                        let expires_at =
                            view["expires_at"].as_u64().ok_or(Error::IntegrityFailed)?;
                        Ok((
                            ask,
                            expires_at,
                            prepare_attestation,
                            prepare_account,
                            prepare_connect,
                        ))
                    })
                    .await
                })
            })
            .await?;
        if prepare_attestation {
            // The final P-256 commitment, protocol expiry and device sequence are
            // fixed before authentication. No Client borrow spans the prompt.
            let prepared = self
                .held(move |h| {
                    Box::pin(async move {
                        let pending = h
                            .with(|c| c.prepare_hardware_attestation(&id, &digest, "gui"))
                            .await?;
                        pending.prepare(h).await
                    })
                })
                .await?;
            let _signature = prepared.cancellation();
            let host = self.host.inner.clone();
            let proved = runtime()
                .spawn_blocking(move || {
                    ensure(host.generation() == generation, Error::Locked)?;
                    Ok::<_, Error>(prepared.sign())
                })
                .await
                .map_err(|_| Error::Unavailable("hardware: worker".into()))??;
            let hardware_error = proved.hardware_error().map(security::hardware_failure);
            let result = self
                .held(move |h| Box::pin(async move { proved.commit(h).await }))
                .await;
            // A locked/replaced session wins over a late hardware callback.
            if self.host.inner.generation() != generation {
                return Err(Error::Locked.into());
            }
            if let Some(error) = hardware_error {
                return Err(error);
            }
            return Ok(ObjectSnapshot {
                generation,
                json: annotate(result?).to_string(),
            });
        }
        if prepare_account {
            let followup_id = id.clone();
            let pending = self
                .held(move |h| {
                    Box::pin(async move {
                        h.with(|c| c.prepare_hardware_account_approval(&id, &digest, "gui"))
                            .await
                    })
                })
                .await?;
            let (pending, key) = self
                .held(move |h| {
                    Box::pin(async move {
                        let key = pending.prepare_key_possession(h).await?;
                        Ok((pending, key))
                    })
                })
                .await?;
            let pending = if let Some(key) = key {
                let _signature = key.cancellation();
                let host = self.host.inner.clone();
                let proof = runtime()
                    .spawn_blocking(move || {
                        ensure(host.generation() == generation, Error::Locked)?;
                        Ok::<_, Error>(key.sign())
                    })
                    .await
                    .map_err(|_| Error::Unavailable("hardware: worker".into()))??;
                let hardware_error = proof.hardware_error().map(security::hardware_failure);
                let result = self
                    .held(move |h| {
                        Box::pin(async move { pending.complete_key_possession(h, proof).await })
                    })
                    .await;
                if self.host.inner.generation() != generation {
                    return Err(Error::Locked.into());
                }
                if let Some(error) = hardware_error {
                    return Err(error);
                }
                result?
            } else {
                pending
            };
            let prepared = self
                .held(move |h| Box::pin(async move { pending.prepare(h).await }))
                .await?;
            let _signature = prepared.cancellation();
            let host = self.host.inner.clone();
            let proved = runtime()
                .spawn_blocking(move || {
                    ensure(host.generation() == generation, Error::Locked)?;
                    Ok::<_, Error>(prepared.sign())
                })
                .await
                .map_err(|_| Error::Unavailable("hardware: worker".into()))??;
            let hardware_error = proved.hardware_error().map(security::hardware_failure);
            let result = self
                .held(move |h| Box::pin(async move { proved.commit(h).await }))
                .await;
            if self.host.inner.generation() != generation {
                return Err(Error::Locked.into());
            }
            if let Some(error) = hardware_error {
                return Err(error);
            }
            result?;
            let view = self
                .host
                .run_at(generation, move |c| {
                    Box::pin(async move { c.finish_prepared_account_approval(&followup_id).await })
                })
                .await?;
            return Ok(ObjectSnapshot {
                generation,
                json: annotate(view).to_string(),
            });
        }
        if prepare_connect {
            let prepared = self
                .held(move |h| {
                    Box::pin(async move {
                        let pending = h.with(|c| c.prepare_hardware_connect(&id, &digest)).await?;
                        pending.prepare(h).await
                    })
                })
                .await?;
            let _signature = prepared.cancellation();
            let host = self.host.inner.clone();
            let proved = runtime()
                .spawn_blocking(move || {
                    ensure(host.generation() == generation, Error::Locked)?;
                    Ok::<_, Error>(prepared.sign())
                })
                .await
                .map_err(|_| Error::Unavailable("hardware: worker".into()))??;
            let hardware_error = proved.hardware_error().map(security::hardware_failure);
            let result = self
                .held(move |h| Box::pin(async move { proved.commit(h).await }))
                .await;
            if self.host.inner.generation() != generation {
                return Err(Error::Locked.into());
            }
            if let Some(error) = hardware_error {
                return Err(error);
            }
            return Ok(ObjectSnapshot {
                generation,
                json: annotate(result?).to_string(),
            });
        }
        let mut confirmed = false;
        let expiry = ApprovalExpiry::at(expires_at, dmsg_client::now_ms(), Instant::now())?;
        if let Some((ask, what)) = ask {
            let platform = self.platform.clone();
            let (key, shown) = (id.clone(), digest.clone());
            let host = self.host.inner.clone();
            let answer = runtime()
                .spawn_blocking(move || {
                    ensure(host.generation() == generation, Error::Locked)?;
                    expiry.check()?;
                    match ask {
                        Ask::Presence(_) => platform
                            .confirm(key, shown, what)
                            .map_err(platform::platform_error),
                        Ask::Hardware(..) => Err(Error::UnsupportedProtocol),
                    }
                })
                .await
                .map_err(|_| Error::Unavailable("hardware: worker".into()))?;
            match answer {
                Ok(()) => {
                    expiry.check()?;
                    confirmed = true;
                }
                Err(error) => {
                    let stored_error = error.clone();
                    self.host
                        .run_at(generation, move |c| {
                            Box::pin(async move { c.approval_unconfirmed(&id, &stored_error) })
                        })
                        .await?;
                    return Err(security::hardware_failure(error));
                }
            }
        }
        let decision = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    c.approval_decide(&id, &digest, approve, "gui", confirmed)
                        .await
                })
            })
            .await?;
        let nearby = self.nearby.clone();
        let view = self
            .held(move |h| {
                Box::pin(async move {
                    match decision {
                        Decision::Done(view) => Ok(view),
                        Decision::Attest(run) => run.run(h).await,
                        Decision::Wallet(run) => run.run(h).await,
                        Decision::SecretShare(run) => {
                            let result = run.run(h).await?;
                            if result["state"] == "ready_for_nearby"
                                && let Some(job_id) = result["id"].as_str()
                            {
                                nearby.notify_approval_ready(job_id.to_string());
                            }
                            Ok(result)
                        }
                        Decision::SecretPackageImport(run) => run.run(h).await,
                        _ => Err(Error::UnsupportedProtocol),
                    }
                })
            })
            .await?;
        let json = annotate(view).to_string();
        Ok(ObjectSnapshot { generation, json })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn cancelled_runner_keeps_the_turn_until_its_marker_cleanup_finishes() {
        let dir = dmsg_client::testing::TempDir::new();
        let client = dmsg_client::testing::unlocked(&dir.0, false, None);
        let host = Host::start(client, runtime().handle());
        let turn = Arc::new(tokio::sync::Mutex::new(()));
        let (entered, ready) = tokio::sync::oneshot::channel();
        let waiting = tokio::spawn({
            let host = host.clone();
            async move {
                host.run(|_| {
                    Box::pin(async move {
                        let _ = entered.send(());
                        std::future::pending::<dmsg_types::Result<()>>().await
                    })
                })
                .await
            }
        });
        ready.await.unwrap();
        drop(RunningApproval {
            host: ScopedHost::new(host.clone(), host.generation()),
            id: "same-request".into(),
            turn: Some(turn.clone().lock_owned().await),
        });
        tokio::task::yield_now().await;
        assert!(turn.try_lock().is_err());
        host.pause();
        assert_eq!(waiting.await.unwrap(), Err(Error::Locked));
        let next = tokio::time::timeout(Duration::from_secs(2), turn.lock())
            .await
            .unwrap();
        drop(next);
        host.close();
        host.closed().await.unwrap();
    }

    #[test]
    fn prompt_and_signing_share_the_original_deadline_despite_clock_changes() {
        let now = Instant::now();
        let short = ApprovalExpiry::at(1010, 1000, now).unwrap();
        assert!(short.check_at(1009, now + Duration::from_millis(9)).is_ok());
        assert_eq!(short.check_at(1010, now), Err(Error::Expired));
        assert_eq!(
            short.check_at(900, now + Duration::from_millis(10)),
            Err(Error::Expired)
        );
        let capped = ApprovalExpiry::at(u64::MAX, 1000, now).unwrap();
        assert_eq!(
            capped.check_at(1000, now + Duration::from_secs(90)),
            Err(Error::Expired)
        );
        assert!(ApprovalExpiry::at(1000, 1000, now).is_err());
    }

    #[test]
    fn unsupported_actions_and_connect_payments_cannot_gain_mobile_approval() {
        for kind in [
            "wallet",
            "checkout",
            "letter",
            "secret_use",
            "secret_run",
            "secret_mount",
            "secret_ssh",
            "future_action",
        ] {
            assert!(!supported(&json!({"type":kind})));
        }
        assert!(!supported(
            &json!({"type":"connect","detail":{"method":"checkout"}})
        ));
        assert!(!supported(&json!({"type":"connect"})));
        assert!(supported(
            &json!({"type":"connect","detail":{"method":"signDocument"}})
        ));
        assert!(supported(
            &json!({"type":"wallet","detail":{"action":"unbind"}})
        ));
        assert!(supported(&json!({"type":"secret_package_import"})));
        assert!(supported(&json!({"type":"hardware_policy"})));
        assert_eq!(
            annotate(json!({"type":"unknown"}))["mobile_supported"],
            false
        );
    }
}
