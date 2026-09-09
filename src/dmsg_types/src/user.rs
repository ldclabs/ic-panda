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
impl DeviceInput {
    pub fn validate(&self) -> Result<()> {
        nonzero(&self.device_id)?;
        nonzero(&self.hpke_pub)?;
        ed25519_dalek::VerifyingKey::from_bytes(&self.signing_pub)
            .map_err(|_| Error::IntegrityFailed)?;
        ensure(
            !self.capabilities.is_empty() && self.capabilities.len() <= 5,
            invalid("capabilities"),
        )?;
        let unique: std::collections::BTreeSet<_> = self.capabilities.iter().collect();
        ensure(
            unique.len() == self.capabilities.len(),
            invalid("duplicate capability"),
        )
    }
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
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Budget {
    pub day: u64,
    pub executions: u32,
    pub cycles: u128,
}
impl Budget {
    pub fn reserve(
        &mut self,
        now: u64,
        cycles: u128,
        count_limit: u32,
        cycle_limit: u128,
    ) -> Result<()> {
        let mut next = self.clone();
        if now / DAY > next.day {
            next = Self {
                day: now / DAY,
                ..Self::default()
            };
        }
        next.executions = next.executions.checked_add(1).ok_or(Error::QuotaExceeded)?;
        next.cycles = next
            .cycles
            .checked_add(cycles)
            .ok_or(Error::QuotaExceeded)?;
        ensure(
            next.executions <= count_limit && next.cycles <= cycle_limit,
            Error::QuotaExceeded,
        )?;
        *self = next;
        Ok(())
    }
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OperationReceipt {
    pub id: OpId,
    pub digest: Hash,
    pub account_version: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleAuthorization {
    pub intent: HandleIntent,
    pub expires_at: u64,
    pub consumed: bool,
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
pub fn recovery_confirmation_message(
    home: Principal,
    subject: SubjectId,
    nonce: u64,
    request: &RecoveryRequest,
    confirmation: &RecoveryConfirmation,
) -> Hash {
    digest(
        "dmsg/recovery-reconfirm/v2",
        &(home, subject, nonce, request, confirmation),
    )
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    pub subject_id: SubjectId,
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
    pub executions: BTreeMap<OpId, AuthorizedExecution>,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SecuritySnapshot {
    pub schema: u16,
    pub subject_id: SubjectId,
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
impl Subject {
    pub fn snapshot(&self) -> SecuritySnapshot {
        SecuritySnapshot {
            schema: 1,
            subject_id: self.subject_id,
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
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UserInit {
    pub home_cose: Principal,
    pub handle_canister: Principal,
    pub payment_canister: Principal,
    pub max_subjects: u64,
    pub daily_new_subjects: u32,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CreateSubject {
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
    pub subject: SubjectId,
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
