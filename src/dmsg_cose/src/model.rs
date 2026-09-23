use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::*;
use dmsg_types::{cose::*, *};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const HOME_DAILY_EXECUTIONS: u32 = 100;
const HOME_DAILY_CYCLES: u128 = 1_000_000_000_000;
const RESULT_RETENTION: u64 = DAY;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Budgets {
    pub total: Budget,
    pub formal: Budget,
}

impl Budgets {
    pub fn reserve(
        &mut self,
        now: u64,
        cycles: u128,
        count_limit: u32,
        cycle_limit: u128,
        formal: bool,
    ) -> Result<()> {
        let mut next = self.clone();
        next.total.reserve(now, cycles, count_limit, cycle_limit)?;
        if formal {
            next.formal.reserve(
                now,
                cycles,
                count_limit - count_limit.div_ceil(5),
                cycle_limit - cycle_limit.div_ceil(5),
            )?;
        }
        *self = next;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    pub request_id: Hash,
    pub digest: Hash,
    pub expires_at: u64,
    pub terminal: bool,
    pub formal: bool,
}

impl Execution {
    fn retained(&self, sequence: u64, terminal_sequence: u64, now: u64) -> bool {
        sequence > terminal_sequence || self.expires_at.saturating_add(RESULT_RETENTION) > now
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Home {
    pub home_user: Principal,
    pub terminal_sequence: u64,
    pub executions: BTreeMap<u64, Execution>,
    pub budget: Budget,
    pub formal_budget: Budget,
}

impl Home {
    pub fn new(home_user: Principal) -> Self {
        Self {
            home_user,
            terminal_sequence: 0,
            executions: BTreeMap::new(),
            budget: Budget::default(),
            formal_budget: Budget::default(),
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
                self.executions[&sequence].digest == digest("dmsg/cose-execution/v3", grant),
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
        let mut retained = 0;
        let mut formal = 0;
        for (seq, e) in &self.executions {
            if e.retained(*seq, self.terminal_sequence, now) {
                retained += 1;
                formal += usize::from(e.formal);
            }
        }
        ensure(retained < WINDOW, Error::QuotaExceeded)?;
        if matches!(grant.kind, ExecutionKind::Sign { .. }) {
            ensure(formal < FORMAL_EXECUTION_WINDOW, Error::QuotaExceeded)?;
        }
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
        let formal = matches!(grant.kind, ExecutionKind::Sign { .. });
        let mut budgets = Budgets {
            total: self.budget.clone(),
            formal: self.formal_budget.clone(),
        };
        budgets.reserve(now, cost, HOME_DAILY_EXECUTIONS, HOME_DAILY_CYCLES, formal)?;
        self.budget = budgets.total;
        self.formal_budget = budgets.formal;
        let removed = self.prune(now);
        self.executions.insert(
            grant.execution_sequence,
            Execution {
                request_id: grant.request_id,
                digest: digest("dmsg/cose-execution/v3", grant),
                expires_at: grant.expires_at,
                terminal: false,
                formal,
            },
        );
        Ok(removed)
    }

    pub fn prune(&mut self, now: u64) -> Vec<u64> {
        let mut removed = Vec::new();
        self.executions.retain(|seq, e| {
            let keep = e.retained(*seq, self.terminal_sequence, now);
            if !keep {
                removed.push(*seq);
            }
            keep
        });
        removed
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
            commerce: None,
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
            cycles_cost_upper_bound: 1,
        }
    }

    fn signing(seq: u64) -> ExecutionGrant {
        let mut grant = g(seq);
        grant.kind = ExecutionKind::Sign {
            key: KeySelector::Signing(SigningKey {
                purpose: SigningPurpose::Statement,
                algorithm: SigningAlgorithm::Ed25519,
            })
            .into(),
            to_be_signed: vec![1].into(),
            public_key_fingerprint: Hash::new([1; 32]),
            origin: "https://example.com".into(),
        };
        grant
    }

    #[test]
    fn signing_history_leaves_root_slots_even_when_roots_arrive_first() {
        let mut h = Home::new(g(1).home_user);
        // The user may dispatch roots after authorizing signatures, but their
        // messages can reach COSE first. Count formal records, not all records.
        for sequence in 57..=64 {
            prepare(&mut h, g(1).home_user, &g(sequence), 2, 1).unwrap();
            h.finish(sequence, &done(sequence));
        }
        for sequence in 1..=56 {
            prepare(&mut h, g(1).home_user, &signing(sequence), 2, 1).unwrap();
            h.finish(sequence, &done(sequence));
        }
        assert_eq!(h.terminal_sequence, 64);
        assert_eq!(h.check(h.home_user, &g(65), 2), Err(Error::QuotaExceeded));

        let mut h = Home::new(g(1).home_user);
        for sequence in 1..=56 {
            prepare(&mut h, g(1).home_user, &signing(sequence), 2, 1).unwrap();
            h.finish(sequence, &done(sequence));
        }
        assert_eq!(
            h.check(h.home_user, &signing(57), 2),
            Err(Error::QuotaExceeded)
        );
        assert_eq!(h.check(h.home_user, &signing(1), 2), Ok(Some(1)));
        prepare(&mut h, g(1).home_user, &g(57), 2, 1).unwrap();
    }

    #[test]
    fn small_budgets_reserve_safety_capacity_atomically() {
        for limit in [1u32, 4, 5, 6] {
            let mut budgets = Budgets::default();
            let formal_limit = limit - limit.div_ceil(5);
            for _ in 0..formal_limit {
                budgets.reserve(1, 1, limit, 100, true).unwrap();
            }
            let before = budgets.clone();
            assert_eq!(
                budgets.reserve(1, 1, limit, 100, true),
                Err(Error::QuotaExceeded)
            );
            assert_eq!(budgets, before);
            for _ in formal_limit..limit {
                budgets.reserve(1, 1, limit, 100, false).unwrap();
            }
            assert_eq!(
                budgets.reserve(1, 1, limit, 100, false),
                Err(Error::QuotaExceeded)
            );
            budgets.reserve(DAY, 1, limit, 100, false).unwrap();
            assert_eq!(budgets.total.executions, 1);
        }
    }

    #[test]
    fn pruning_keeps_holes_and_budgets_but_reclaims_idle_terminal_results() {
        let mut h = Home::new(g(1).home_user);
        for sequence in 1..=3 {
            prepare(&mut h, g(1).home_user, &g(sequence), 2, 1).unwrap();
        }
        h.finish(1, &done(1));
        h.finish(3, &done(3));
        let budget = h.budget.clone();
        assert!(h.prune(DAY).is_empty());
        assert_eq!(h.prune(DAY + MINUTE), vec![1]);
        assert_eq!(h.budget, budget);
        assert_eq!(h.terminal_sequence, 1);
        assert_eq!(
            h.check(h.home_user, &g(1), 2 * DAY),
            Err(Error::ResultExpired)
        );
        h.finish(2, &done(2));
        assert_eq!(h.prune(2 * DAY), vec![2, 3]);
        assert_eq!(h.terminal_sequence, 3);
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
