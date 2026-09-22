//! Monotone legacy business freeze. Recovery reads remain a separate authority.
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, CandidType, Deserialize, Serialize, PartialEq, Eq)]
pub enum LegacyMode {
    #[default]
    Active,
    Draining,
    ReadOnly,
}

#[derive(Clone, Debug, CandidType, Deserialize, Serialize)]
pub struct PendingWrite {
    pub id: u64,
    pub method: String,
    pub caller: Principal,
    pub input: ByteBuf,
    pub started_at: u64,
    pub boot: u64,
    pub live: bool,
    pub uncertain: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct FreezeState {
    pub mode: LegacyMode,
    pub epoch: u64,
    pub cutover_id: Option<ByteArray<32>>,
    pub draining_at: Option<u64>,
    pub readonly_at: Option<u64>,
    pub boot: u64,
    pub next_ticket: u64,
    pub pending: BTreeMap<u64, PendingWrite>,
    pub resolutions: BTreeMap<u64, ByteArray<32>>,
    pub instrumented: bool,
    pub baseline_needed: bool,
    pub baseline_evidence: Option<ByteArray<32>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Serialize, PartialEq, Eq)]
pub struct FreezeStatus {
    pub mode: LegacyMode,
    pub epoch: u64,
    pub cutover_id: Option<ByteArray<32>>,
    pub draining_at: Option<u64>,
    pub readonly_at: Option<u64>,
    pub pending: u64,
    pub unresolved_after_upgrade: u64,
    pub baseline_needed: bool,
    pub baseline_evidence: Option<ByteArray<32>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Serialize)]
pub enum SnapshotScope {
    Names,
    Authorities,
    Profile,
    Channels,
    Channel(u32),
    ChannelAuthority(u32),
    Messages(u32),
    /// Explicit caller approval of a domain-separated migration intent.
    Attestation {
        digest: ByteArray<32>,
        expires_at: u64,
    },
}

#[derive(Clone, Debug, CandidType, Deserialize, Serialize)]
pub struct ChannelAuthority {
    pub id: u32,
    pub managers: std::collections::BTreeSet<Principal>,
    pub members: std::collections::BTreeSet<Principal>,
    pub message_start: u32,
    pub latest_message_id: u32,
    pub dek_digest: ByteArray<32>,
    pub storage: Option<(Principal, u32)>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Serialize)]
pub struct SnapshotPage {
    pub schema: u8,
    pub source: Principal,
    pub subject: Principal,
    pub freeze: FreezeStatus,
    pub scope: SnapshotScope,
    pub source_context: ByteBuf,
    pub count: u64,
    pub initial_digest: ByteArray<32>,
    pub inventory_digest: ByteArray<32>,
    pub entries: Vec<(String, ByteBuf)>,
    pub next: Option<String>,
    pub complete: bool,
}

/// Same Candid record as ProfileInfo, with deterministic map iteration.
#[derive(CandidType, Serialize)]
pub struct FrozenProfile {
    pub id: Principal,
    pub canister: Principal,
    pub bio: String,
    pub active_at: u64,
    pub created_at: u64,
    pub image_file: Option<(Principal, u32)>,
    pub links: Vec<crate::profile::Link>,
    pub tokens: Vec<Principal>,
    pub following: Option<std::collections::BTreeSet<Principal>>,
    pub channels: Option<BTreeMap<(Principal, u64), crate::profile::ChannelSetting>>,
    pub ecdh_pub: Option<ByteArray<32>>,
}

impl From<crate::profile::ProfileInfo> for FrozenProfile {
    fn from(p: crate::profile::ProfileInfo) -> Self {
        Self {
            id: p.id,
            canister: p.canister,
            bio: p.bio,
            active_at: p.active_at,
            created_at: p.created_at,
            image_file: p.image_file,
            links: p.links,
            tokens: p.tokens,
            following: p.following,
            channels: p.channels.map(|m| m.into_iter().collect()),
            ecdh_pub: p.ecdh_pub,
        }
    }
}

impl FreezeState {
    pub fn status(&self) -> FreezeStatus {
        FreezeStatus {
            mode: self.mode.clone(),
            epoch: self.epoch,
            cutover_id: self.cutover_id,
            draining_at: self.draining_at,
            readonly_at: self.readonly_at,
            pending: self.pending.len() as u64,
            unresolved_after_upgrade: self
                .pending
                .values()
                .filter(|p| !p.live || p.boot < self.boot)
                .count() as u64,
            baseline_needed: self.baseline_needed,
            baseline_evidence: self.baseline_evidence,
        }
    }

    pub fn writable(&self) -> Result<(), String> {
        if self.mode != LegacyMode::Active {
            return Err("LegacyWriteDisabled".into());
        }
        Ok(())
    }

    pub fn begin(
        &mut self,
        method: &str,
        caller: Principal,
        input: Vec<u8>,
        now: u64,
    ) -> Result<u64, String> {
        self.writable()?;
        if self.pending.len() >= 256 || input.len() > 65536 {
            return Err("LegacyPendingLimit".into());
        }
        let id = self
            .next_ticket
            .checked_add(1)
            .ok_or("LegacyTicketExhausted")?;
        self.next_ticket = id;
        self.instrumented = true;
        self.pending.insert(
            id,
            PendingWrite {
                id,
                method: method.into(),
                caller,
                input: input.into(),
                started_at: now,
                boot: self.boot,
                live: true,
                uncertain: false,
            },
        );
        Ok(id)
    }

    pub fn finish(&mut self, ticket: u64) -> Result<(), String> {
        let pending = self.pending.get_mut(&ticket).ok_or("LegacyUnknownTicket")?;
        if self.mode == LegacyMode::ReadOnly || pending.boot != self.boot {
            return Err("LegacyCallbackAfterFreeze".into());
        }
        if pending.uncertain {
            pending.live = false;
        } else {
            self.pending.remove(&ticket);
        }
        Ok(())
    }

    pub fn uncertain(&mut self, ticket: u64) -> Result<(), String> {
        self.pending
            .get_mut(&ticket)
            .ok_or("LegacyUnknownTicket")?
            .uncertain = true;
        Ok(())
    }

    pub fn after_callback(&self, ticket: u64, now: u64) -> Result<(), String> {
        let pending = self.pending.get(&ticket).ok_or("LegacyUnknownTicket")?;
        if self.mode == LegacyMode::ReadOnly
            || pending.boot != self.boot
            || !pending.live
            || now < pending.started_at
        {
            return Err("LegacyCallbackAfterFreeze".into());
        }
        if self.draining_at.is_some_and(|at| pending.started_at > at) {
            return Err("LegacyCutoverConflict".into());
        }
        Ok(())
    }

    pub fn drain(&mut self, cutover: ByteArray<32>, now: u64) -> Result<FreezeStatus, String> {
        if cutover.as_ref() == &[0; 32] {
            return Err("InvalidCutoverId".into());
        }
        if self.mode != LegacyMode::Active {
            if self.cutover_id == Some(cutover) {
                return Ok(self.status());
            }
            return Err("LegacyCutoverConflict".into());
        }
        self.mode = LegacyMode::Draining;
        self.epoch = self.epoch.checked_add(1).ok_or("LegacyEpochExhausted")?;
        self.cutover_id = Some(cutover);
        self.draining_at = Some(now);
        Ok(self.status())
    }

    pub fn seal(&mut self, cutover: ByteArray<32>, now: u64) -> Result<FreezeStatus, String> {
        if self.cutover_id != Some(cutover) {
            return Err("LegacyCutoverConflict".into());
        }
        if self.mode == LegacyMode::ReadOnly {
            return Ok(self.status());
        }
        if self.mode != LegacyMode::Draining || !self.pending.is_empty() {
            return Err("LegacyWritesPending".into());
        }
        if self.baseline_needed {
            return Err("LegacyBaselineReviewRequired".into());
        }
        self.mode = LegacyMode::ReadOnly;
        self.readonly_at = Some(now);
        Ok(self.status())
    }

    pub fn upgraded(&mut self) {
        self.baseline_needed |= !self.instrumented;
        self.instrumented = true;
        self.boot = self.boot.checked_add(1).expect("legacy boot exhausted");
    }

    pub fn acknowledge_baseline(&mut self, evidence: ByteArray<32>) -> Result<(), String> {
        if evidence.as_ref() == &[0; 32] {
            return Err("InvalidBaselineEvidence".into());
        }
        if let Some(previous) = self.baseline_evidence {
            return if previous == evidence {
                Ok(())
            } else {
                Err("LegacyBaselineConflict".into())
            };
        }
        self.baseline_evidence = Some(evidence);
        self.baseline_needed = false;
        Ok(())
    }

    /// Only a controller may call this after reviewing an interrupted operation.
    /// The digest records that review; it is not an automatic ledger proof.
    pub fn resolve_interrupted(
        &mut self,
        ticket: u64,
        evidence: ByteArray<32>,
    ) -> Result<(), String> {
        if let Some(old) = self.resolutions.get(&ticket) {
            return if old == &evidence {
                Ok(())
            } else {
                Err("LegacyResolutionConflict".into())
            };
        }
        let pending = self.pending.get(&ticket).ok_or("LegacyUnknownTicket")?;
        if (pending.live && pending.boot >= self.boot)
            || evidence.as_ref() == &[0; 32]
            || self.resolutions.len() >= 256
        {
            return Err("LegacyOperationStillLive".into());
        }
        self.pending.remove(&ticket);
        self.resolutions.insert(ticket, evidence);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_live_calls_before_sealing_and_never_reopens() {
        let mut state = FreezeState::default();
        let op = state
            .begin("create_channel", Principal::anonymous(), vec![1], 10)
            .unwrap();
        let id = ByteArray::new([7; 32]);
        state.drain(id, 11).unwrap();
        assert!(state.writable().is_err());
        assert!(state.seal(id, 12).is_err());
        assert!(state
            .resolve_interrupted(op, ByteArray::new([9; 32]))
            .is_err());
        state.finish(op).unwrap();
        let frozen = state.seal(id, 13).unwrap();
        assert_eq!(frozen, state.seal(id, 14).unwrap());
        assert!(state.finish(op).is_err());
        assert!(state.drain(ByteArray::new([8; 32]), 15).is_err());
    }

    #[test]
    fn interrupted_calls_require_explicit_resolution_and_old_callbacks_fail() {
        let mut state = FreezeState::default();
        let ticket = state
            .begin("register_username", Principal::anonymous(), vec![1], 1)
            .unwrap();
        state.upgraded();
        let id = ByteArray::new([7; 32]);
        state.drain(id, 2).unwrap();
        assert_eq!(state.status().unresolved_after_upgrade, 1);
        assert!(state.finish(ticket).is_err());
        assert!(state.seal(id, 3).is_err());
        state
            .resolve_interrupted(ticket, ByteArray::new([9; 32]))
            .unwrap();
        state.seal(id, 4).unwrap();
        assert!(state.finish(ticket).is_err());
    }

    #[test]
    fn unknown_outbound_result_stays_pending_after_parent_returns() {
        let mut state = FreezeState::default();
        let ticket = state
            .begin("register_username", Principal::anonymous(), vec![1], 10)
            .unwrap();
        state.uncertain(ticket).unwrap();
        state.finish(ticket).unwrap();
        let cutover = ByteArray::new([7; 32]);
        state.drain(cutover, 11).unwrap();
        assert!(state.seal(cutover, 12).is_err());
        assert!(state.after_callback(ticket, 12).is_err());
        state
            .resolve_interrupted(ticket, ByteArray::new([9; 32]))
            .unwrap();
        state.seal(cutover, 13).unwrap();
    }
}
