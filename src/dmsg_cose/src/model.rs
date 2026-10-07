use cbor2::Cbor;
use dmsg_protocol::*;
use dmsg_runtime::*;
use dmsg_types::{cose::*, *};
use std::collections::BTreeMap;

// Cover everything a user home may authorize for one account in a day: the
// formal share (total less a fifth) holds the formal ceiling, the rest roots.
const HOME_DAILY_EXECUTIONS: u32 = 125;
const HOME_DAILY_CYCLES: u128 = 1_100_000_000_000;
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

    /// Settle a reservation once its management call returns, or at once when
    /// it was not sent: keep only the charged cycles and, when the call ran
    /// nothing, return the execution as well.
    pub fn settle(
        &mut self,
        reserved_at: u64,
        reserved: u128,
        charged: u128,
        executed: bool,
        formal: bool,
    ) {
        self.total.settle(reserved_at, reserved, charged, executed);
        if formal {
            self.formal.settle(reserved_at, reserved, charged, executed);
        }
    }
}

/// Global budgets and the number of executions recorded as Unknown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Global {
    pub budgets: Budgets,
    pub unknown: u64,
}

/// A returned unknown outcome cannot be dispatched again, but must retain its
/// result and commercial hold. It does not block later closed sequences.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub enum ExecutionState {
    InFlight,
    Unknown,
    Terminal,
}

/// Private retained-execution metadata; this CBOR form is its stable representation.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct Execution {
    #[cbor(key = 1)]
    pub request_id: Hash,
    #[cbor(key = 2)]
    pub digest: Hash,
    #[cbor(key = 3)]
    pub expires_at: u64,
    #[cbor(key = 4)]
    pub state: ExecutionState,
    #[cbor(key = 5)]
    pub formal: bool,
}

impl Execution {
    fn retained(&self, sequence: u64, closed_sequence: u64, now: u64) -> bool {
        self.state != ExecutionState::Terminal
            || sequence > closed_sequence
            || self.expires_at.saturating_add(RESULT_RETENTION) > now
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Home {
    pub closed_sequence: u64,
    pub executions: BTreeMap<u64, Execution>,
    pub budgets: Budgets,
}

impl Home {
    /// Check replay and window bounds after the entry point authenticates the
    /// configured user home, before parsing or deriving keys.
    pub fn check(&self, grant: &ExecutionGrant, now: u64) -> Result<Option<u64>> {
        if let Some(sequence) = self.sequence(&grant.request_id) {
            ensure(
                self.executions[&sequence].digest == digest("dmsg/cose-execution/v3", grant),
                Error::IdempotencyConflict,
            )?;
            return Ok(Some(sequence));
        }
        let sequence = grant.execution_sequence;
        if sequence <= self.closed_sequence {
            return Err(Error::ResultExpired);
        }
        ensure(
            sequence - self.closed_sequence <= WINDOW as u64,
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
        let mut retained = 0;
        let mut formal = 0;
        for (seq, e) in &self.executions {
            if e.retained(*seq, self.closed_sequence, now) {
                retained += 1;
                formal += usize::from(e.formal);
            }
        }
        ensure(retained < WINDOW, Error::QuotaExceeded)?;
        if grant.kind.is_formal() {
            ensure(formal < FORMAL_EXECUTION_WINDOW, Error::QuotaExceeded)?;
        }
        Ok(None)
    }

    pub fn sequence(&self, request_id: &Hash) -> Option<u64> {
        self.executions
            .iter()
            .find_map(|(seq, e)| (e.request_id == *request_id).then_some(*seq))
    }

    /// Called after check, without an intervening await. Only a management call
    /// (`cost`) consumes budget; expired or rejected requests close regardless.
    /// Return only the result keys to delete; no result bodies are loaded.
    pub fn prepare(
        &mut self,
        grant: &ExecutionGrant,
        now: u64,
        cost: Option<u128>,
    ) -> Result<Vec<u64>> {
        let formal = grant.kind.is_formal();
        if let Some(cost) = cost {
            self.budgets
                .reserve(now, cost, HOME_DAILY_EXECUTIONS, HOME_DAILY_CYCLES, formal)?;
        }
        let removed = self.prune(now);
        self.executions.insert(
            grant.execution_sequence,
            Execution {
                request_id: grant.request_id,
                digest: digest("dmsg/cose-execution/v3", grant),
                expires_at: grant.expires_at,
                state: ExecutionState::InFlight,
                formal,
            },
        );
        Ok(removed)
    }

    pub fn prune(&mut self, now: u64) -> Vec<u64> {
        let mut removed = Vec::new();
        self.executions.retain(|seq, e| {
            let keep = e.retained(*seq, self.closed_sequence, now);
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
        execution.state = match &result.outcome {
            ExecutionOutcome::Unknown(_) => ExecutionState::Unknown,
            ExecutionOutcome::Completed(_)
            | ExecutionOutcome::Failed(_)
            | ExecutionOutcome::ResultExpired => ExecutionState::Terminal,
            _ => panic!("execution has not returned"),
        };
        self.close();
    }

    /// Record as Unknown every InFlight execution whose management call this
    /// module instance is not awaiting: an upgrade dropped its callback.
    /// Return their sequences.
    pub fn abandon(&mut self, awaiting: impl Fn(u64) -> bool) -> Vec<u64> {
        let mut abandoned = Vec::new();
        for (sequence, e) in &mut self.executions {
            if e.state == ExecutionState::InFlight && !awaiting(*sequence) {
                e.state = ExecutionState::Unknown;
                abandoned.push(*sequence);
            }
        }
        if !abandoned.is_empty() {
            self.close();
        }
        abandoned
    }

    fn close(&mut self) {
        while self.closed_sequence.checked_add(1).is_some_and(|next| {
            self.executions
                .get(&next)
                .is_some_and(|e| e.state != ExecutionState::InFlight)
        }) {
            self.closed_sequence += 1;
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
    let mut path = signing_prefix(config);
    path.extend(signing_suffix(account_id, key));
    path
}

pub fn signing_prefix(config: &CoseInit) -> Vec<Vec<u8>> {
    vec![b"dmsg/formal/v2".to_vec(), canonical(&config.environment)]
}

pub fn signing_suffix(account_id: &AccountId, key: &KeyRequest) -> Vec<Vec<u8>> {
    vec![
        account_id.to_vec(),
        canonical(&key.purpose),
        key.generation.to_be_bytes().to_vec(),
    ]
}

pub fn context(config: &CoseInit) -> Vec<u8> {
    content_root_context(&config.environment, config.derivation_version)
}

pub fn root_input(account_id: &AccountId, generation: u64) -> Vec<u8> {
    canonical(&(account_id, generation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;

    fn prepare(h: &mut Home, grant: &ExecutionGrant, now: u64, cost: u128) -> Result<Option<u64>> {
        if let Some(sequence) = h.check(grant, now)? {
            return Ok(Some(sequence));
        }
        h.prepare(grant, now, Some(cost))?;
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
                transport_key: [1; 48].into(),
            },
            max_cycles: 100,
        }
    }

    fn done(seq: u64) -> ExecutionResult {
        ExecutionResult {
            request_id: g(seq).request_id,
            outcome: ExecutionOutcome::ResultExpired,
            cycles_cost_upper_bound: 1,
            cycles_charged: 0,
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
        let mut h = Home::default();
        // The user may dispatch roots after authorizing signatures, but their
        // messages can reach COSE first. Count formal records, not all records.
        for sequence in 57..=64 {
            prepare(&mut h, &g(sequence), 2, 1).unwrap();
            h.finish(sequence, &done(sequence));
        }
        for sequence in 1..=56 {
            prepare(&mut h, &signing(sequence), 2, 1).unwrap();
            h.finish(sequence, &done(sequence));
        }
        assert_eq!(h.closed_sequence, 64);
        assert_eq!(h.check(&g(65), 2), Err(Error::QuotaExceeded));

        let mut h = Home::default();
        for sequence in 1..=56 {
            prepare(&mut h, &signing(sequence), 2, 1).unwrap();
            h.finish(sequence, &done(sequence));
        }
        assert_eq!(h.check(&signing(57), 2), Err(Error::QuotaExceeded));
        assert_eq!(h.check(&signing(1), 2), Ok(Some(1)));
        prepare(&mut h, &g(57), 2, 1).unwrap();
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
    fn settlement_applies_to_both_budget_classes() {
        let mut budgets = Budgets::default();
        budgets.reserve(2 * DAY, 10, 10, 100, true).unwrap();
        let before = budgets.clone();
        for formal in [false, true] {
            budgets.reserve(2 * DAY, 20, 10, 100, formal).unwrap();
            budgets.settle(2 * DAY, 20, 0, false, formal);
            assert_eq!(budgets, before);
        }
        // A returned call keeps its execution and only the charged cycles.
        budgets.reserve(2 * DAY, 68, 10, 100, true).unwrap();
        budgets.settle(2 * DAY, 68, 26, true, true);
        assert_eq!((budgets.total.executions, budgets.total.cycles), (2, 36));
        assert_eq!((budgets.formal.executions, budgets.formal.cycles), (2, 36));
        // Settling frees what the upper bound had reserved: two more fit.
        for _ in 0..2 {
            budgets.reserve(2 * DAY, 20, 10, 100, true).unwrap();
        }
        assert_eq!(
            budgets.reserve(2 * DAY, 20, 10, 100, true),
            Err(Error::QuotaExceeded)
        );
    }

    #[test]
    fn abandoned_calls_become_unknown_and_release_the_window() {
        let mut h = Home::default();
        for sequence in 1..=3 {
            prepare(&mut h, &g(sequence), 2, 1).unwrap();
        }
        h.finish(2, &done(2));
        assert_eq!(h.closed_sequence, 0);
        // Sequence 3 is still awaited by this instance; sequence 1 is not.
        assert_eq!(h.abandon(|sequence| sequence == 3), vec![1]);
        assert_eq!(h.executions[&1].state, ExecutionState::Unknown);
        assert_eq!(h.executions[&3].state, ExecutionState::InFlight);
        assert_eq!(h.closed_sequence, 2);
        assert!(h.abandon(|sequence| sequence == 3).is_empty());
        h.finish(3, &done(3));
        assert_eq!(h.closed_sequence, 3);
    }

    #[test]
    fn home_budget_covers_every_user_authorized_execution() {
        let formal = FORMAL_DAILY_CYCLES / u128::from(FORMAL_DAILY_EXECUTIONS);
        let root = ROOT_DAILY_CYCLES / u128::from(ROOT_DAILY_EXECUTIONS);
        for formal_first in [true, false] {
            let mut budgets = Budgets::default();
            let mut reserve = |count: u32, cycles: u128, formal: bool| {
                for _ in 0..count {
                    budgets
                        .reserve(1, cycles, HOME_DAILY_EXECUTIONS, HOME_DAILY_CYCLES, formal)
                        .unwrap();
                }
            };
            if formal_first {
                reserve(FORMAL_DAILY_EXECUTIONS, formal, true);
                reserve(ROOT_DAILY_EXECUTIONS, root, false);
            } else {
                reserve(ROOT_DAILY_EXECUTIONS, root, false);
                reserve(FORMAL_DAILY_EXECUTIONS, formal, true);
            }
        }
    }

    #[test]
    fn rejected_and_expired_requests_close_without_budget() {
        let mut h = Home::default();
        h.budgets.total.executions = HOME_DAILY_EXECUTIONS;
        let budgets = h.budgets.clone();
        h.check(&g(1), 2).unwrap();
        assert!(h.prepare(&g(1), 2, None).unwrap().is_empty());
        assert_eq!(h.budgets, budgets);
        let mut failed = done(1);
        failed.outcome = ExecutionOutcome::Failed(Error::Expired);
        h.finish(1, &failed);
        assert_eq!(h.closed_sequence, 1);
        h.check(&g(2), 2).unwrap();
        assert_eq!(h.prepare(&g(2), 2, Some(1)), Err(Error::QuotaExceeded));
    }

    #[test]
    fn pruning_keeps_holes_and_budgets_but_reclaims_idle_terminal_results() {
        let mut h = Home::default();
        for sequence in 1..=3 {
            prepare(&mut h, &g(sequence), 2, 1).unwrap();
        }
        h.finish(1, &done(1));
        h.finish(3, &done(3));
        let budgets = h.budgets.clone();
        assert!(h.prune(DAY).is_empty());
        assert_eq!(h.prune(DAY + MINUTE), vec![1]);
        assert_eq!(h.budgets, budgets);
        assert_eq!(h.closed_sequence, 1);
        assert_eq!(h.check(&g(1), 2 * DAY), Err(Error::ResultExpired));
        h.finish(2, &done(2));
        assert_eq!(h.prune(2 * DAY), vec![2, 3]);
        assert_eq!(h.closed_sequence, 3);
    }

    #[test]
    fn out_of_order_and_replay_never_skip_a_hole() {
        let mut h = Home::default();
        prepare(&mut h, &g(2), 2, 1).unwrap();
        h.finish(2, &done(2));
        assert_eq!(h.closed_sequence, 0);
        prepare(&mut h, &g(1), 2, 1).unwrap();
        h.finish(1, &done(1));
        assert_eq!(h.closed_sequence, 2);
        assert!(prepare(&mut h, &g(1), 2, 1).unwrap().is_some());
        let mut bad = g(1);
        bad.max_cycles = 99;
        assert_eq!(prepare(&mut h, &bad, 2, 1), Err(Error::IdempotencyConflict));
        h.executions.clear();
        assert_eq!(prepare(&mut h, &g(1), 2, 1), Err(Error::ResultExpired));
    }

    #[test]
    fn cleaned_request_cannot_reuse_id_with_new_device_sequence() {
        let mut h = Home::default();
        prepare(&mut h, &g(1), 2, 1).unwrap();
        h.finish(1, &done(1));
        let mut second = g(2);
        second.approved_at = 2 * DAY;
        second.expires_at = 2 * DAY + MINUTE;
        prepare(&mut h, &second, 2 * DAY, 1).unwrap();
        h.finish(2, &done(2));
        assert!(!h.executions.contains_key(&1));
        let mut replay = g(3);
        replay.approved_at = 2 * DAY;
        replay.expires_at = 2 * DAY + MINUTE;
        replay.request_id = g(1).request_id;
        assert_eq!(
            prepare(&mut h, &replay, 2 * DAY, 1),
            Err(Error::IdempotencyConflict)
        );
        assert_eq!(
            prepare(&mut h, &g(1), 2 * DAY, 1),
            Err(Error::ResultExpired)
        );
    }

    #[test]
    fn a_rejected_execution_does_not_block_the_next_window() {
        let mut h = Home::default();
        for seq in 1..=65 {
            let mut grant = g(seq);
            let at = if seq <= 64 { 2 } else { 2 * DAY };
            grant.approved_at = at;
            grant.expires_at = at + MINUTE;
            prepare(&mut h, &grant, at, 1).unwrap();
            let mut result = done(seq);
            if seq == 1 {
                result.outcome = ExecutionOutcome::Failed(Error::UnsupportedProtocol);
            }
            h.finish(seq, &result);
        }
        assert_eq!(h.closed_sequence, 65);
    }

    #[test]
    fn only_in_flight_executions_pin_the_sequence_window() {
        let mut h = Home::default();
        for sequence in 1..=3 {
            prepare(&mut h, &g(sequence), 2, 1).unwrap();
        }
        let mut unknown = done(1);
        unknown.outcome = ExecutionOutcome::Unknown(Error::ExecutionUnknown);
        h.finish(1, &unknown);
        h.finish(3, &done(3));
        let mut next = g(4);
        next.approved_at = 2 * DAY;
        next.expires_at = next.approved_at + MINUTE;
        h.check(&next, next.approved_at).unwrap();
        assert!(h
            .prepare(&next, next.approved_at, Some(1))
            .unwrap()
            .is_empty());
        assert_eq!(h.closed_sequence, 1);
        assert_eq!(h.executions.len(), 4);
        h.finish(1, &done(1));
        assert_eq!(h.closed_sequence, 1);
        h.finish(2, &done(2));
        assert_eq!(h.closed_sequence, 3);
    }

    #[test]
    fn returned_unknown_keeps_its_result_without_pinning_later_windows() {
        use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};

        let mut h = Home::default();
        for sequence in 1..=WINDOW as u64 {
            prepare(&mut h, &g(sequence), 2, 1).unwrap();
            let mut result = done(sequence);
            if sequence == 1 {
                result.outcome = ExecutionOutcome::Unknown(Error::ExecutionUnknown);
            }
            h.finish(sequence, &result);
        }
        assert_eq!(h.closed_sequence, 64);
        // The exact state must survive an upgrade before cleanup.
        let mut h = compact_from_bytes::<Home>(&compact_bytes(&h));
        assert_eq!(h.prune(30 * DAY), (2..=64).collect::<Vec<_>>());
        assert_eq!(h.executions.len(), 1);
        assert_eq!(h.executions[&1].state, ExecutionState::Unknown);
        assert_eq!(h.check(&g(1), 30 * DAY), Ok(Some(1)));
        assert_eq!(h.check(&g(2), 30 * DAY), Err(Error::ResultExpired));
        let mut changed = g(1);
        changed.max_cycles += 1;
        assert_eq!(h.check(&changed, 30 * DAY), Err(Error::IdempotencyConflict));
        let mut next = g(65);
        next.approved_at = 30 * DAY;
        next.expires_at = next.approved_at + MINUTE;
        prepare(&mut h, &next, next.approved_at, 1).unwrap();
        h.finish(65, &done(65));
        assert_eq!(h.closed_sequence, 65);
    }

    #[test]
    fn failed_budget_reservation_does_not_prune_or_insert() {
        let mut h = Home::default();
        prepare(&mut h, &g(1), 2, 1).unwrap();
        h.finish(1, &done(1));
        h.budgets.total.day = 2;
        h.budgets.total.executions = HOME_DAILY_EXECUTIONS;
        let before = h.clone();
        let mut next = g(2);
        next.approved_at = 2 * DAY;
        next.expires_at = next.approved_at + MINUTE;
        assert_eq!(
            prepare(&mut h, &next, next.approved_at, 1),
            Err(Error::QuotaExceeded)
        );
        assert_eq!(h, before);
    }

    #[test]
    fn last_sequence_finishes_without_overflow() {
        let mut h = Home {
            closed_sequence: u64::MAX - 1,
            ..Default::default()
        };
        let grant = g(u64::MAX);
        prepare(&mut h, &grant, 2, 1).unwrap();
        h.finish(u64::MAX, &done(u64::MAX));
        assert_eq!(h.closed_sequence, u64::MAX);
    }
}
