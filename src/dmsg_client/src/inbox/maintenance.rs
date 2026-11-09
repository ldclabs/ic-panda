//! Renew an existing free inbox after account security changes. Content sync
//! uploads the new key before publishing it; contact rules remain unchanged.

use super::*;
use crate::account::Security;

const CHECKED: &str = "inbox-key-security";

#[derive(Clone)]
pub(crate) struct Refresh {
    old: Json,
    input: PolicyInput,
    security: (u64, u64),
    version: u64,
    public: [u8; 32],
}

fn stamp(security: &Security) -> (u64, u64) {
    (
        security.snapshot.security_epoch,
        security.snapshot.content_root_generation,
    )
}

/// Only an existing signed policy may supply rules for automatic renewal.
fn input(old: &Json, security: &Security) -> Result<PolicyInput> {
    let cose = old["signed"]["cose_sign1"]
        .as_str()
        .ok_or(Error::IntegrityFailed)?;
    let payload: PolicyPayload =
        crate::cloud::verify_command(cose, &security.snapshot.issuer, &security.devices, POLICY)?;
    let fields = serde_json::to_value(&payload).expect("json");
    ensure(
        fields
            .as_object()
            .expect("policy")
            .iter()
            .all(|(k, v)| old[k] == *v)
            && old["version"].as_u64() == payload.expected_version.checked_add(1)
            && old["hash"] == hex::encode(*sha256(&unb64(cose)?)),
        Error::IntegrityFailed,
    )?;
    // A paid offer needs its own authorized renewal. Never silently extend
    // its lifetime or change its ledger/payee as part of content sync.
    ensure(
        payload.mode != "paid" && payload.offer.is_none(),
        Error::Forbidden,
    )?;
    Ok(PolicyInput {
        mode: payload.mode,
        allow: payload.allow,
        block: Some(payload.block),
        invitations: Some(payload.invitations),
        capacity: Some(payload.capacity),
        price: None,
        payee: None,
    })
}

/// Called after pulling content and before uploading, in the host's sync lane.
pub(crate) async fn prepare<H: Hold>(h: &H) -> Result<Option<Refresh>> {
    let h = operations::Unlocked::new(h).await?;
    let Some(security) = h
        .with(|c| {
            let security = c.fresh()?;
            if security.snapshot.vault_write_state != dmsg_types::user::VaultWriteState::Ready
                || c.roots()?.generation != security.snapshot.content_root_generation
                || c.workspace.store.objects_of_kind(INBOX_KEY)?.is_empty()
                || c.workspace.store.meta::<(u64, u64)>(CHECKED)? == Some(stamp(&security))
            {
                return Ok(None);
            }
            Ok(Some(security))
        })
        .await?
    else {
        return Ok(None);
    };
    let observed = stamp(&security);
    let old = policy(&h).await?;
    let Some(old) = old.filter(|p| {
        p["security_epoch"].as_u64() != Some(observed.0)
            || p["root_generation"].as_u64() != Some(observed.1)
    }) else {
        h.with(|c| c.workspace.store.set_meta(CHECKED, &observed))
            .await?;
        return Ok(None);
    };
    if old["mode"] == "paid" || !old["offer"].is_null() {
        return Ok(None);
    }
    let input = input(&old, &security)?;
    let version = old["inbox_key_version"]
        .as_u64()
        .and_then(|v| v.checked_add(1))
        .ok_or(Error::IntegrityFailed)?;
    let public = h.with(|c| c.inbox_key(version)).await?;
    Ok(Some(Refresh {
        old,
        input,
        security: observed,
        version,
        public,
    }))
}

/// Publishing uses the exact policy observed before upload, including its CAS
/// hash. A concurrent settings edit wins; the next sync rereads its rules.
pub(crate) async fn finish<H: Hold>(h: &H, refresh: Refresh) -> Result<()> {
    let h = operations::Unlocked::new(h).await?;
    let ready = h
        .with(|c| {
            if stamp(&c.fresh()?) != refresh.security {
                return Ok(false);
            }
            let (row, _) = c.current(&key_object(&h.account.to_string(), refresh.version))?;
            Ok(
                row.state == crate::store::SYNCED
                    && c.inbox_key(refresh.version)? == refresh.public,
            )
        })
        .await?;
    if !ready {
        return Ok(());
    }
    match operations::configure_observed(
        &h,
        &refresh.input,
        Principal::anonymous(),
        None,
        Some(refresh.old),
        "policy-refresh",
    )
    .await
    {
        Ok(_) => {
            h.with(|c| c.workspace.store.set_meta(CHECKED, &refresh.security))
                .await
        }
        Err(Error::VersionConflict) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "maintenance_tests.rs"]
mod tests;
