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
    pub consumed: bool,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountState {
    pub account_id: AccountId,
    pub home_user: Principal,
    pub home_cose: Principal,
    pub auth_bindings: Vec<Principal>,
    pub account_version: u64,
    pub security_epoch: u64,
    pub status: AccountStatus,
    pub devices: BTreeMap<Hash, Device>,
    pub recovery: Option<RecoveryPolicy>,
    pub recovery_checked: bool,
    pub recovery_nonce: u64,
    pub pending_recovery: Option<PendingRecovery>,
    pub current_root: Option<ContentRootRef>,
    pub root_slot: Option<RootReservation>,
    pub next_root_generation: u64,
    pub vault_write_state: VaultWriteState,
    pub sensitive_policy: SensitivePolicy,
    pub budget: Budget,
    pub next_execution_sequence: u64,
    pub operations: Vec<OperationReceipt>,
    pub handle_authorizations: BTreeMap<OpId, HandleAuthorization>,
    #[serde(skip)]
    pub executions: BTreeMap<OpId, AuthorizedExecution>,
}
impl AccountState {
    pub fn snapshot(&self, namespace: &str) -> SecuritySnapshot {
        SecuritySnapshot {
            issuer: format!("{namespace}{}", self.account_id),
            home_cose: self.home_cose,
            schema: 2,
            account_id: self.account_id,
            home_user: self.home_user,
            account_status: self.status.clone(),
            account_version: self.account_version,
            security_epoch: self.security_epoch,
            devices_root: digest("dmsg/devices/v1", &self.devices),
            recovery_root_version: self.recovery.as_ref().map_or(0, |r| r.generation),
            recovery_hpke_pub: self.recovery.as_ref().map(|r| r.hpke_pub),
            recovery_signing_pub: self.recovery.as_ref().map(|r| r.signing_pub),
            recovery_nonce: self.recovery_nonce,
            recovery_delay_ms: self.recovery.as_ref().map(|r| r.delay_ms),
            pending_recovery_digest: self
                .pending_recovery
                .as_ref()
                .map(|r| digest("dmsg/pending-recovery/v1", r)),
            content_root_generation: self.current_root.as_ref().map_or(0, |r| r.generation),
            content_root_digest: self.current_root.as_ref().map(|r| r.bundle_digest),
            vault_write_state: self.vault_write_state.clone(),
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedExecution {
    pub grant: ExecutionGrant,
    pub command_digest: Hash,
    pub result: ExecutionResult,
}
impl AccountState {
    pub fn info(&self, namespace: &str) -> AccountInfo {
        AccountInfo {
            issuer: format!("{namespace}{}", self.account_id),
            account_id: self.account_id,
            home_user: self.home_user,
            home_cose: self.home_cose,
            auth_bindings: self.auth_bindings.clone(),
            account_version: self.account_version,
            security_epoch: self.security_epoch,
            status: self.status.clone(),
            devices: self.devices.clone(),
            recovery: self.recovery.clone(),
            recovery_checked: self.recovery_checked,
            recovery_nonce: self.recovery_nonce,
            pending_recovery: self.pending_recovery.clone(),
            current_root: self.current_root.clone(),
            root_slot: self.root_slot.clone(),
            vault_write_state: self.vault_write_state.clone(),
            sensitive_policy: self.sensitive_policy.clone(),
        }
    }
}
