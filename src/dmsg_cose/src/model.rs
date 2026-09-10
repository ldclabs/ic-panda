use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::*;
use dmsg_types::{cose::*, *};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    pub grant: ExecutionGrant,
    pub digest: Hash,
    pub result: ExecutionResult,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Home {
    pub home_user: Principal,
    pub terminal_sequence: u64,
    #[serde(skip)]
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
    pub fn prepare(
        &mut self,
        caller: Principal,
        grant: &ExecutionGrant,
        now: u64,
        cost: u128,
    ) -> Result<Option<ExecutionResult>> {
        ensure(
            caller == self.home_user && caller == grant.home_user,
            Error::Forbidden,
        )?;
        let fp = digest("dmsg/cose-execution/v2", grant);
        if let Some(e) = self
            .executions
            .values()
            .find(|e| e.grant.request_id == grant.request_id)
        {
            ensure(e.digest == fp, Error::IdempotencyConflict)?;
            return Ok(Some(e.result.clone()));
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
        ensure(cost <= grant.max_cycles, Error::QuotaExceeded)?;
        let mut next = self.clone();
        next.executions.retain(|seq, e| {
            *seq > next.terminal_sequence || e.grant.expires_at.saturating_add(DAY) > now
        });
        ensure(next.executions.len() < WINDOW, Error::QuotaExceeded)?;
        next.budget.reserve(now, cost, 100, 1_000_000_000_000)?;
        next.executions.insert(
            sequence,
            Execution {
                grant: grant.clone(),
                digest: fp,
                result: ExecutionResult {
                    request_id: grant.request_id,
                    outcome: ExecutionOutcome::Executing,
                    charged_cycles: cost,
                },
            },
        );
        *self = next;
        Ok(None)
    }
    pub fn finish(&mut self, sequence: u64, result: ExecutionResult) {
        self.executions
            .get_mut(&sequence)
            .expect("prepared execution")
            .result = result;
        while self
            .executions
            .get(&(self.terminal_sequence + 1))
            .is_some_and(|e| e.result.is_terminal())
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
        h.prepare(h.home_user, &g(2), 2, 1).unwrap();
        h.finish(2, done(2));
        assert_eq!(h.terminal_sequence, 0);
        h.prepare(h.home_user, &g(1), 2, 1).unwrap();
        h.finish(1, done(1));
        assert_eq!(h.terminal_sequence, 2);
        assert!(h.prepare(h.home_user, &g(1), 2, 1).unwrap().is_some());
        let mut bad = g(1);
        bad.max_cycles = 99;
        assert_eq!(
            h.prepare(h.home_user, &bad, 2, 1),
            Err(Error::IdempotencyConflict)
        );
        h.executions.clear();
        assert_eq!(
            h.prepare(h.home_user, &g(1), 2, 1),
            Err(Error::ResultExpired)
        );
    }
    #[test]
    fn wrong_home_does_not_consume_sequence() {
        let mut h = Home::new(g(1).home_user);
        assert_eq!(
            h.prepare(Principal::anonymous(), &g(1), 2, 1),
            Err(Error::Forbidden)
        );
        assert!(h.executions.is_empty());
    }
    #[test]
    fn cleaned_request_cannot_reuse_id_with_new_device_sequence() {
        let mut h = Home::new(g(1).home_user);
        h.prepare(h.home_user, &g(1), 2, 1).unwrap();
        h.finish(1, done(1));
        let mut second = g(2);
        second.approved_at = 2 * DAY;
        second.expires_at = 2 * DAY + MINUTE;
        h.prepare(h.home_user, &second, 2 * DAY, 1).unwrap();
        h.finish(2, done(2));
        assert!(!h.executions.contains_key(&1));
        let mut replay = g(3);
        replay.approved_at = 2 * DAY;
        replay.expires_at = 2 * DAY + MINUTE;
        replay.request_id = g(1).request_id;
        assert_eq!(
            h.prepare(h.home_user, &replay, 2 * DAY, 1),
            Err(Error::IdempotencyConflict)
        );
        assert_eq!(
            h.prepare(h.home_user, &g(1), 2 * DAY, 1),
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
            h.prepare(h.home_user, &grant, at, 1).unwrap();
            let mut result = done(seq);
            if seq == 1 {
                result.outcome = ExecutionOutcome::Failed(Error::UnsupportedProtocol);
            }
            h.finish(seq, result);
        }
        assert_eq!(h.terminal_sequence, 65);
    }
}
