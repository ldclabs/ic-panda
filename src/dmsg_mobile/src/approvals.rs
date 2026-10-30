use super::*;
use dmsg_client::approval::{Ask, Decision};
use serde_json::Value;

fn supported(view: &Value) -> bool {
    match view["type"].as_str() {
        Some(
            "device_pair"
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
        operation
            .run(self.decide_reviewed(id, digest, approve))
            .await
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
        let _turn = self.approval_turn.lock().await;
        struct Running {
            host: ScopedHost,
            id: String,
        }
        impl Drop for Running {
            fn drop(&mut self) {
                let host = self.host.clone();
                let id = self.id.clone();
                runtime().spawn(async move {
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
        let _running = Running {
            host: self.host.clone(),
            id: id.clone(),
        };
        let generation = self.host.generation();
        let (read_id, shown) = (id.clone(), digest.clone());
        let ask = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    let view = c.approval(&read_id)?;
                    if approve {
                        ensure(supported(&view), Error::UnsupportedProtocol)?;
                    }
                    ensure(
                        view["digest"].as_str() == Some(shown.as_str()),
                        Error::IntegrityFailed,
                    )?;
                    ensure(view["state"] == "awaiting_user", Error::Expired)?;
                    if approve
                        && !matches!(
                            view["type"].as_str(),
                            Some("secret_share" | "secret_package_import")
                        )
                    {
                        // Required purposes may have changed on another device. Read
                        // the current account before choosing the single native prompt.
                        dmsg_client::approval_key::status(&c.cooperative_hold()).await?;
                        let current = c.approval(&read_id)?;
                        ensure(
                            current["digest"].as_str() == Some(shown.as_str()),
                            Error::IntegrityFailed,
                        )?;
                        ensure(current["state"] == "awaiting_user", Error::Expired)?;
                    }
                    if approve {
                        c.approval_presence(&read_id)
                    } else {
                        Ok(None)
                    }
                })
            })
            .await?;
        let mut confirmed = false;
        let mut authorized = None;
        if let Some((ask, what)) = ask {
            let platform = self.platform.clone();
            let (key, shown) = (id.clone(), digest.clone());
            let answer = runtime()
                .spawn_blocking(move || match ask {
                    Ask::Presence(_) => platform
                        .confirm(key, shown, what)
                        .map(|_| None)
                        .map_err(platform::platform_error),
                    Ask::Hardware(keys, biometry_only) => {
                        keys.authorize(&what, biometry_only).map(Some)
                    }
                })
                .await
                .map_err(|_| Error::Unavailable("hardware: worker".into()))?;
            match answer {
                Ok(answer) => {
                    authorized = answer;
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
                    c.approval_decide_with(&id, &digest, approve, "gui", confirmed, authorized)
                        .await
                })
            })
            .await?;
        let view = self
            .held(move |h| {
                Box::pin(async move {
                    match decision {
                        Decision::Done(view) => Ok(view),
                        Decision::Attest(run) => run.run(h).await,
                        Decision::Wallet(run) => run.run(h).await,
                        Decision::SecretShare(run) => run.run(h).await,
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
