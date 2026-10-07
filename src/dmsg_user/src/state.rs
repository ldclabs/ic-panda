use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::Budget;
use dmsg_types::{cose::*, handle::*, user::*, *};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleAuthorization {
    pub intent: HandleIntent,
    pub expires_at: u64,
}

/// The latest completed recovery, retained for exact completion retries and
/// for the root derivations its device may request until it rekeys.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecoveryReceipt {
    pub request_id: OpId,
    pub new_auth: Principal,
    pub device_id: Hash,
    /// Root generation the recovered device may still derive; None once it
    /// commits a new root.
    pub root_generation: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountState {
    pub created_at_ms: u64,
    pub account_id: AccountId,
    pub home_user: Principal,
    pub home_cose: Principal,
    pub auth_bindings: Vec<Principal>,
    pub account_version: u64,
    pub security_epoch: u64,
    pub devices: BTreeMap<Hash, Device>,
    pub recovery_delay_ms: u64,
    pub pending_recovery: Option<PendingRecovery>,
    pub completed_recovery: Option<RecoveryReceipt>,
    pub current_root: Option<ContentRootRef>,
    pub root_slot: Option<RootReservation>,
    pub next_root_generation: u64,
    pub vault_write_state: VaultWriteState,
    pub sensitive_policy: SensitivePolicy,
    pub budget: Budget,
    pub next_execution_sequence: u64,
    pub operations: Vec<OperationReceipt>,
    pub handle_authorizations: BTreeMap<OpId, HandleAuthorization>,
    // None pins a nonterminal execution; Some marks when its result may be
    // evicted. Payloads and results live only in the execution table.
    pub execution_expirations: BTreeMap<OpId, Option<u64>>,
    // Mirror of the principal record's `updated_at`, certified in the snapshot.
    pub principal_updated_at: Option<u64>,
}

impl AccountState {
    pub fn snapshot(&self, namespace: &str) -> SecuritySnapshot {
        SecuritySnapshot {
            issuer: account_issuer(namespace, &self.account_id),
            home_cose: self.home_cose,
            schema: 4,
            account_id: self.account_id,
            home_user: self.home_user,
            account_version: self.account_version,
            security_epoch: self.security_epoch,
            devices_root: digest("dmsg/devices/v1", &self.devices),
            recovery_delay_ms: self.recovery_delay_ms,
            pending_recovery_digest: self
                .pending_recovery
                .as_ref()
                .map(|r| digest("dmsg/pending-recovery/v1", r)),
            content_root_generation: self.current_root.as_ref().map_or(0, |r| r.generation),
            content_root_digest: self.current_root.as_ref().map(|r| r.bundle_digest),
            vault_write_state: self.vault_write_state.clone(),
            principal_updated_at: self.principal_updated_at,
        }
    }

    pub fn info(&self, namespace: &str) -> AccountInfo {
        AccountInfo {
            created_at_ms: self.created_at_ms,
            issuer: account_issuer(namespace, &self.account_id),
            account_id: self.account_id,
            home_user: self.home_user,
            home_cose: self.home_cose,
            auth_bindings: self.auth_bindings.clone(),
            account_version: self.account_version,
            security_epoch: self.security_epoch,
            devices: self.devices.clone(),
            recovery_delay_ms: self.recovery_delay_ms,
            pending_recovery: self.pending_recovery.clone(),
            recovered_device: self
                .completed_recovery
                .as_ref()
                .and_then(|r| r.root_generation.map(|g| (r.device_id, g))),
            current_root: self.current_root.clone(),
            root_slot: self.root_slot.clone(),
            vault_write_state: self.vault_write_state.clone(),
            sensitive_policy: self.sensitive_policy.clone(),
        }
    }

    /// IDs of the devices that are not revoked.
    pub fn active_devices(&self) -> Vec<Hash> {
        self.devices
            .iter()
            .filter(|(_, d)| d.revoked_at.is_none())
            .map(|(id, _)| *id)
            .collect()
    }
}

/// A device-signed statement the home verified and certified in one message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Attestation {
    pub device_id: Hash,
    pub security_epoch: u64,
    pub approved_at: u64,
    pub expires_at: u64,
    pub origin: String,
    pub to_be_signed_digest: Hash,
    pub public_key_fingerprint: Hash,
    pub signature_digest: Hash,
    pub artifact: SignedArtifact,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
// Records are decoded from stable memory per call; the variant size gap is immaterial.
#[allow(clippy::large_enum_variant)]
pub enum ExecutionRecord {
    Attestation(Attestation),
    Derivation {
        grant: ExecutionGrant,
        result: ExecutionResult,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedExecution {
    pub account_id: AccountId,
    pub request_id: OpId,
    pub command_digest: Hash,
    pub record: ExecutionRecord,
}

impl AuthorizedExecution {
    /// Retention deadline of a terminal record: a day after its approval expired.
    pub fn retention(&self) -> u64 {
        match &self.record {
            ExecutionRecord::Attestation(a) => a.expires_at,
            ExecutionRecord::Derivation { grant, .. } => grant.expires_at,
        }
        .saturating_add(DAY)
    }
}
