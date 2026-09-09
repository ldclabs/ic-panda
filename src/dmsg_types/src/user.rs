use crate::{cose::*, handle::HandleIntent, *};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;

pub const MAX_DEVICES: usize = 16;
pub const MAX_AUTH_BINDINGS: usize = 8;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ControllerRole {
    Administrator,
    Member,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    ContentSign,
    VaultUnlock,
    RootManage,
    FormalApprove,
    PaymentOffer,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeviceInput {
    pub device_id: Hash,
    pub signing_pub: Hash,
    pub hpke_pub: Hash,
    pub role: ControllerRole,
    pub capabilities: Vec<Capability>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub input: DeviceInput,
    pub added_at: u64,
    pub added_by: Option<Hash>,
    pub revoked_at: Option<u64>,
    pub next_sequence: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecoveryPolicy {
    pub generation: u64,
    pub signing_pub: Hash,
    pub hpke_pub: Hash,
    pub delay_ms: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum VaultWriteState {
    Uninitialized,
    Ready,
    RekeyRequired,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    RecoveryDisputed,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ContentRootRef {
    pub generation: u64,
    pub suite: String,
    pub home_cose: Principal,
    pub derivation_version: u16,
    pub key_generation: u64,
    pub bundle_digest: Hash,
    pub recovery_generation: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RootReservation {
    pub op_id: OpId,
    pub expected_generation: u64,
    pub generation: u64,
    pub security_epoch: u64,
    pub expires_at: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SensitivePolicy {
    pub frozen: bool,
    pub allowed_purposes: Vec<KeyPurpose>,
    pub daily_executions: u32,
    pub daily_cycles: u128,
}
impl Default for SensitivePolicy {
    fn default() -> Self {
        Self {
            frozen: false,
            allowed_purposes: vec![KeyPurpose::FileAttestation, KeyPurpose::Statement],
            daily_executions: 20,
            daily_cycles: 1_000_000_000_000,
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OperationReceipt {
    pub id: OpId,
    pub digest: Hash,
    pub account_version: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRequest {
    pub op_id: OpId,
    pub new_auth: Principal,
    pub device: DeviceInput,
    pub generation: u64,
    pub expires_at: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PendingRecovery {
    pub request: RecoveryRequest,
    pub execute_after: u64,
    pub dispute: Option<Hash>,
    pub reconfirmed: bool,
    #[serde(default)]
    pub confirmation: Option<RecoveryConfirmation>,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecoveryConfirmation {
    pub request_id: OpId,
    pub dispute: Hash,
    pub expires_at: u64,
}
impl PendingRecovery {
    pub fn expires_at(&self) -> u64 {
        self.confirmation
            .as_ref()
            .map_or(self.request.expires_at, |c| c.expires_at)
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SecuritySnapshot {
    pub issuer: String,
    pub home_cose: Principal,
    pub schema: u16,
    pub account_id: AccountId,
    pub home_user: Principal,
    pub account_status: AccountStatus,
    pub account_version: u64,
    pub security_epoch: u64,
    pub devices_root: Hash,
    pub recovery_root_version: u64,
    pub recovery_hpke_pub: Option<Hash>,
    pub recovery_signing_pub: Option<Hash>,
    pub recovery_nonce: u64,
    pub recovery_delay_ms: Option<u64>,
    pub pending_recovery_digest: Option<Hash>,
    pub content_root_generation: u64,
    pub content_root_digest: Option<Hash>,
    pub vault_write_state: VaultWriteState,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UserInit {
    pub environment: Environment,
    pub issuer_namespace: String,
    pub home_cose: Principal,
    pub handle_canister: Principal,
    pub payment_canister: Principal,
    pub max_accounts: u64,
    pub daily_new_accounts: u32,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CreateAccount {
    pub device: DeviceInput,
    pub op_id: OpId,
    pub expires_at: u64,
    pub proof: ByteBuf,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AccountCommand {
    AddDevice {
        device: DeviceInput,
        proof: ByteBuf,
    },
    RevokeDevice {
        device_id: Hash,
    },
    BindAuth {
        principal: Principal,
        nonce: Hash,
    },
    RemoveAuth {
        principal: Principal,
    },
    SetRecovery {
        policy: RecoveryPolicy,
        proof: ByteBuf,
    },
    ConfirmRecovery {
        proof: ByteBuf,
    },
    SetPolicy {
        policy: SensitivePolicy,
    },
    ReserveRoot {
        expected_generation: u64,
        op_id: OpId,
    },
    CommitRoot {
        expected_generation: u64,
        op_id: OpId,
        root: ContentRootRef,
    },
    AuthorizeHandle {
        intent: HandleIntent,
    },
    DisputeRecovery {
        op_id: OpId,
        dispute: Hash,
    },
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountMutation {
    pub account_id: AccountId,
    pub expected_version: u64,
    pub command: AccountCommand,
    pub approval: Approval,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeviceEvidence {
    pub snapshot: SecuritySnapshot,
    pub device: Device,
    pub observed_at: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountInfo {
    pub issuer: String,
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
    pub vault_write_state: VaultWriteState,
    pub sensitive_policy: SensitivePolicy,
}
