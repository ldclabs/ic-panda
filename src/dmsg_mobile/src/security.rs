//! Named account-security operations. Native key handles, authorization tokens
//! and signable commitments never cross the public MobileEngine API.

use crate::{
    MobileEngine, ObjectSnapshot,
    error::{MobileError, Result},
    runtime,
};
use dmsg_client::{approval_key, client::LoginUnlock};
use dmsg_types::{Error, Hash, ensure, ensure_valid};

/// Map failures of a known hardware step without relabelling network or
/// protocol-integrity errors from the surrounding operation.
pub(crate) fn hardware_failure(error: Error) -> MobileError {
    match error {
        Error::Forbidden => MobileError::Failure {
            code: "CANCELLED".into(),
        },
        Error::IntegrityFailed => MobileError::Failure {
            code: "HARDWARE_KEY_INVALIDATED".into(),
        },
        Error::Unavailable(_) => MobileError::Failure {
            code: "HARDWARE_UNAVAILABLE".into(),
        },
        other => other.into(),
    }
}

impl MobileEngine {
    /// Complete a login's optional hardware step. `generation` is captured by
    /// the caller before preparation; it is never read again to accept work.
    pub(crate) async fn finish_login_unlock(
        &self,
        generation: u64,
        prepared: Option<LoginUnlock>,
    ) -> Result<()> {
        if let Some(prepared) = prepared {
            let (prepared, key) = runtime()
                .spawn_blocking(move || {
                    let key = prepared.open();
                    (prepared, key)
                })
                .await
                .map_err(|_| MobileError::Failure {
                    code: "HARDWARE_UNAVAILABLE".into(),
                })?;
            let failure = key.as_ref().err().cloned().map(hardware_failure);
            let completed = self
                .host
                .inner
                .run_at(generation, move |c| {
                    Box::pin(
                        async move { c.complete_login_unlock(prepared, key).await.map(|_| ()) },
                    )
                })
                .await;
            // Session invalidation wins over a late hardware error.
            if matches!(completed, Err(Error::Locked)) {
                return Err(Error::Locked.into());
            }
            if let Some(error) = failure {
                return Err(error);
            }
            completed?;
        }
        Ok(())
    }
}

#[uniffi::export]
impl MobileEngine {
    pub async fn hardware_security(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { approval_key::owner_security_status(h).await }))
            .await
    }

    pub async fn register_approval_key(&self, biometry_only: bool) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move { approval_key::register_request(h, "gui", biometry_only).await })
        })
        .await
    }

    pub async fn approval_key_request(&self, biometry_only: bool) -> Result<ObjectSnapshot> {
        let generation = self.host.generation();
        let prepared = self
            .held(move |h| {
                Box::pin(async move { approval_key::prepare_key_request(h, biometry_only).await })
            })
            .await?;
        let proved = runtime()
            .spawn_blocking(move || prepared.prove())
            .await
            .map_err(|_| MobileError::Failure {
                code: "HARDWARE_UNAVAILABLE".into(),
            })?;
        ensure(generation == self.host.inner.generation(), Error::Locked)?;
        let proved = proved.map_err(hardware_failure)?;
        let json = self
            .held(move |h| {
                Box::pin(async move {
                    let token = approval_key::complete_key_request(h, proved).await?;
                    Ok(serde_json::json!({"token": token}).to_string())
                })
            })
            .await?;
        Ok(ObjectSnapshot { generation, json })
    }

    pub async fn approve_approval_key(
        &self,
        token: String,
        device: String,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(
            token.len() <= 16 * 1024 && device.len() == 64,
            "approval key request",
        )?;
        self.held_object(move |h| {
            Box::pin(async move { approval_key::approve_request(h, "gui", &token, &device).await })
        })
        .await
    }

    pub async fn retire_approval_key(&self, device: Option<String>) -> Result<ObjectSnapshot> {
        let device = device
            .map(|value| {
                let bytes: [u8; 32] = hex::decode(value)
                    .ok()
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| dmsg_types::invalid("device"))?;
                Ok::<_, Error>(Hash::new(bytes))
            })
            .transpose()?;
        self.held_object(move |h| {
            Box::pin(async move { approval_key::retire_request(h, "gui", device).await })
        })
        .await
    }

    pub async fn set_hardware_policy(&self, purposes: Vec<String>) -> Result<ObjectSnapshot> {
        ensure_valid(purposes.len() <= 5, "hardware purposes")?;
        let purposes = purposes
            .iter()
            .map(|p| approval_key::purpose_named(p))
            .collect::<dmsg_types::Result<Vec<_>>>()?;
        self.held_object(move |h| {
            Box::pin(async move { approval_key::policy_request(h, "gui", purposes).await })
        })
        .await
    }

    pub async fn set_device_bound(
        &self,
        enabled: bool,
        biometry_only: bool,
    ) -> Result<ObjectSnapshot> {
        self.held(|h| {
            Box::pin(async {
                use dmsg_client::hold::Hold;
                h.with(|c| c.display_session().map(|_| ())).await?;
                h.ensure_login().await
            })
        })
        .await?;
        let generation = self.host.generation();
        let json = if enabled {
            let prepared = self
                .host
                .run_at(generation, move |c| {
                    Box::pin(async move { c.prepare_device_binding(biometry_only) })
                })
                .await?;
            let opened = runtime()
                .spawn_blocking(move || prepared.open())
                .await
                .map_err(|_| MobileError::Failure {
                    code: "HARDWARE_UNAVAILABLE".into(),
                })?;
            ensure(generation == self.host.inner.generation(), Error::Locked)?;
            let opened = opened.map_err(hardware_failure)?;
            self.host
                .run_at(generation, move |c| {
                    Box::pin(async move {
                        c.complete_device_binding(opened)
                            .await
                            .map(|v| v.to_string())
                    })
                })
                .await?
        } else {
            self.host
                .run_at(generation, |c| {
                    Box::pin(async {
                        let status = c.unbind_device().await?;
                        if status.state == "logged_out" {
                            Ok(serde_json::json!({"device_bound": false}).to_string())
                        } else {
                            c.device_binding_status().map(|v| v.to_string())
                        }
                    })
                })
                .await?
        };
        Ok(ObjectSnapshot { generation, json })
    }
}
