use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::*;
use dmsg_types::{cose::*, *};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    pub request_id: Hash,
    pub digest: Hash,
    pub expires_at: u64,
    pub terminal: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Home {
    pub home_user: Principal,
    pub terminal_sequence: u64,
    pub executions: BTreeMap<u64, Execution>,
    pub budget: Budget,
}

impl Home {
    pub fn new(home_user: Principal) -> Self {
        Self {
            home_user,
            terminal_sequence: 0,
            executions: BTreeMap::new(),
            budget: Budget::default(),
        }
    }

    /// Check replay and cheap authorization/window bounds before key derivation.
    pub fn check(
        &self,
        caller: Principal,
        grant: &ExecutionGrant,
        now: u64,
    ) -> Result<Option<u64>> {
        ensure(
            caller == self.home_user && caller == grant.home_user,
            Error::Forbidden,
        )?;
        if let Some(sequence) = self.sequence(&grant.request_id) {
            ensure(
                self.executions[&sequence].digest == digest("dmsg/cose-execution/v2", grant),
                Error::IdempotencyConflict,
            )?;
            return Ok(Some(sequence));
        }
        let sequence = grant.execution_sequence;
        if sequence <= self.terminal_sequence {
            return Err(Error::ResultExpired);
        }
        ensure(
            sequence - self.terminal_sequence <= WINDOW as u64,
            Error::QuotaExceeded,
        )?;
        if self.executions.contains_key(&sequence) {
            return Err(Error::IdempotencyConflict);
        }
        ensure(
            grant.request_id
                == execution_request_id(
                    &grant.account_id,
                    grant.security_epoch,
                    grant.device_id,
                    grant.device_sequence,
                ),
            Error::IdempotencyConflict,
        )?;
        ensure(
            grant.approved_at <= now
                && grant.expires_at > grant.approved_at
                && grant.expires_at - grant.approved_at <= 5 * MINUTE,
            Error::Expired,
        )?;
        nonzero(grant.request_id.as_slice())?;
        ensure(
            self.executions
                .iter()
                .filter(|(seq, e)| {
                    **seq > self.terminal_sequence || e.expires_at.saturating_add(DAY) > now
                })
                .count()
                < WINDOW,
            Error::QuotaExceeded,
        )?;
        Ok(None)
    }

    pub fn sequence(&self, request_id: &Hash) -> Option<u64> {
        self.executions
            .iter()
            .find_map(|(seq, e)| (e.request_id == *request_id).then_some(*seq))
    }

    /// Called after check, without an intervening await. Return only the result
    /// keys to delete; no historical result bodies need to be loaded.
    pub fn prepare(&mut self, grant: &ExecutionGrant, now: u64, cost: u128) -> Result<Vec<u64>> {
        ensure(cost <= grant.max_cycles, Error::QuotaExceeded)?;
        self.budget.reserve(now, cost, 100, 1_000_000_000_000)?;
        let mut removed = Vec::new();
        self.executions.retain(|seq, e| {
            let keep = *seq > self.terminal_sequence || e.expires_at.saturating_add(DAY) > now;
            if !keep {
                removed.push(*seq);
            }
            keep
        });
        self.executions.insert(
            grant.execution_sequence,
            Execution {
                request_id: grant.request_id,
                digest: digest("dmsg/cose-execution/v2", grant),
                expires_at: grant.expires_at,
                terminal: false,
            },
        );
        Ok(removed)
    }

    pub fn finish(&mut self, sequence: u64, result: &ExecutionResult) {
        let execution = self
            .executions
            .get_mut(&sequence)
            .expect("prepared execution");
        assert_eq!(execution.request_id, result.request_id);
        execution.terminal = result.is_terminal();
        while self
            .terminal_sequence
            .checked_add(1)
            .is_some_and(|next| self.executions.get(&next).is_some_and(|e| e.terminal))
        {
            self.terminal_sequence += 1;
        }
    }
}

pub fn key_id(config: &CoseInit, account_id: &AccountId, key: &KeyRequest) -> Hash {
    digest(
        "dmsg/key-id/v3",
        &(
            &config.environment,
            config.executing_canister,
            config.derivation_version,
            account_id,
            key,
        ),
    )
}

pub fn path(config: &CoseInit, account_id: &AccountId, key: &KeyRequest) -> Vec<Vec<u8>> {
    vec![
        b"dmsg/formal/v2".to_vec(),
        canonical(&config.environment),
        account_id.to_vec(),
        canonical(&key.purpose),
        key.generation.to_be_bytes().to_vec(),
    ]
}

pub fn context(config: &CoseInit) -> Vec<u8> {
    canonical(&(
        "dmsg/content-root/v2",
        &config.environment,
        config.derivation_version,
    ))
}

pub fn root_input(account_id: &AccountId, generation: u64) -> Vec<u8> {
    canonical(&(account_id, generation))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prepare(
        h: &mut Home,
        caller: Principal,
        grant: &ExecutionGrant,
        now: u64,
        cost: u128,
    ) -> Result<Option<u64>> {
        if let Some(sequence) = h.check(caller, grant, now)? {
            return Ok(Some(sequence));
        }
        h.prepare(grant, now, cost)?;
        Ok(None)
    }
    fn g(seq: u64) -> ExecutionGrant {
        ExecutionGrant {
            account_id: AccountId([1; 12]),
            home_user: Principal::from_slice(&[1]),
            home_cose: Principal::from_slice(&[2]),
            request_id: execution_request_id(&AccountId([1; 12]), 0, Hash::new([2; 32]), seq),
            execution_sequence: seq,
            security_epoch: 0,
            device_id: Hash::new([2; 32]),
            device_sequence: seq,
            approved_at: 1,
            expires_at: MINUTE,
            kind: ExecutionKind::Derive {
                generation: 1,
                root_op_id: Some(Hash::new([3; 32])),
                transport_key: vec![1; 48].into(),
            },
            max_cycles: 100,
        }
    }
    fn done(seq: u64) -> ExecutionResult {
        ExecutionResult {
            request_id: g(seq).request_id,
            outcome: ExecutionOutcome::ResultExpired,
            charged_cycles: 1,
        }
    }
    #[test]
    fn out_of_order_and_replay_never_skip_a_hole() {
        let mut h = Home::new(g(1).home_user);
        prepare(&mut h, g(1).home_user, &g(2), 2, 1).unwrap();
        h.finish(2, &done(2));
        assert_eq!(h.terminal_sequence, 0);
        prepare(&mut h, g(1).home_user, &g(1), 2, 1).unwrap();
        h.finish(1, &done(1));
        assert_eq!(h.terminal_sequence, 2);
        assert!(prepare(&mut h, g(1).home_user, &g(1), 2, 1)
            .unwrap()
            .is_some());
        let mut bad = g(1);
        bad.max_cycles = 99;
        assert_eq!(
            prepare(&mut h, g(1).home_user, &bad, 2, 1),
            Err(Error::IdempotencyConflict)
        );
        h.executions.clear();
        assert_eq!(
            prepare(&mut h, g(1).home_user, &g(1), 2, 1),
            Err(Error::ResultExpired)
        );
    }
    #[test]
    fn wrong_home_does_not_consume_sequence() {
        let mut h = Home::new(g(1).home_user);
        assert_eq!(
            prepare(&mut h, Principal::anonymous(), &g(1), 2, 1),
            Err(Error::Forbidden)
        );
        assert!(h.executions.is_empty());
    }
    #[test]
    fn cleaned_request_cannot_reuse_id_with_new_device_sequence() {
        let mut h = Home::new(g(1).home_user);
        prepare(&mut h, g(1).home_user, &g(1), 2, 1).unwrap();
        h.finish(1, &done(1));
        let mut second = g(2);
        second.approved_at = 2 * DAY;
        second.expires_at = 2 * DAY + MINUTE;
        prepare(&mut h, g(1).home_user, &second, 2 * DAY, 1).unwrap();
        h.finish(2, &done(2));
        assert!(!h.executions.contains_key(&1));
        let mut replay = g(3);
        replay.approved_at = 2 * DAY;
        replay.expires_at = 2 * DAY + MINUTE;
        replay.request_id = g(1).request_id;
        assert_eq!(
            prepare(&mut h, g(1).home_user, &replay, 2 * DAY, 1),
            Err(Error::IdempotencyConflict)
        );
        assert_eq!(
            prepare(&mut h, g(1).home_user, &g(1), 2 * DAY, 1),
            Err(Error::ResultExpired)
        );
    }
    #[test]
    fn a_rejected_execution_does_not_block_the_next_window() {
        let mut h = Home::new(g(1).home_user);
        for seq in 1..=65 {
            let mut grant = g(seq);
            let at = if seq <= 64 { 2 } else { 2 * DAY };
            grant.approved_at = at;
            grant.expires_at = at + MINUTE;
            prepare(&mut h, g(1).home_user, &grant, at, 1).unwrap();
            let mut result = done(seq);
            if seq == 1 {
                result.outcome = ExecutionOutcome::Failed(Error::UnsupportedProtocol);
            }
            h.finish(seq, &result);
        }
        assert_eq!(h.terminal_sequence, 65);
    }

    #[test]
    fn unknown_and_in_flight_executions_pin_the_window() {
        let mut h = Home::new(g(1).home_user);
        for sequence in 1..=3 {
            prepare(&mut h, g(1).home_user, &g(sequence), 2, 1).unwrap();
        }
        let mut unknown = done(1);
        unknown.outcome = ExecutionOutcome::Unknown(Error::ExecutionUnknown);
        h.finish(1, &unknown);
        h.finish(3, &done(3));
        let mut next = g(4);
        next.approved_at = 2 * DAY;
        next.expires_at = next.approved_at + MINUTE;
        h.check(h.home_user, &next, next.approved_at).unwrap();
        assert!(h.prepare(&next, next.approved_at, 1).unwrap().is_empty());
        assert_eq!(h.terminal_sequence, 0);
        assert_eq!(h.executions.len(), 4);
        h.finish(1, &done(1));
        assert_eq!(h.terminal_sequence, 1);
        h.finish(2, &done(2));
        assert_eq!(h.terminal_sequence, 3);
    }

    #[test]
    fn failed_budget_reservation_does_not_prune_or_insert() {
        let mut h = Home::new(g(1).home_user);
        prepare(&mut h, g(1).home_user, &g(1), 2, 1).unwrap();
        h.finish(1, &done(1));
        h.budget.day = 2;
        h.budget.executions = 100;
        let before = h.clone();
        let mut next = g(2);
        next.approved_at = 2 * DAY;
        next.expires_at = next.approved_at + MINUTE;
        assert_eq!(
            prepare(&mut h, g(1).home_user, &next, next.approved_at, 1),
            Err(Error::QuotaExceeded)
        );
        assert_eq!(h, before);
    }

    #[test]
    fn last_sequence_finishes_without_overflow() {
        let mut h = Home::new(g(1).home_user);
        h.terminal_sequence = u64::MAX - 1;
        let grant = g(u64::MAX);
        prepare(&mut h, g(1).home_user, &grant, 2, 1).unwrap();
        h.finish(u64::MAX, &done(u64::MAX));
        assert_eq!(h.terminal_sequence, u64::MAX);
    }
}
