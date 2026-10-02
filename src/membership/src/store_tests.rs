use super::*;
use crate::fixture;
use dmsg_protocol::integration::product_decision_hash;
use dmsg_types::{integration::*, membership::Eligibility};

fn claim_for(index: u64, actor: Principal) -> Claim {
    let mut request = fixture::claim();
    request.terms.offer.operation_id = digest("membership/test-operation", &index);
    request.terms.neuron_id = digest("membership/test-neuron", &index);
    request.terms.actor = actor;
    Claim::new(panda_claim_id(&request.terms), request, PANDA_COOLING_MS)
}

#[test]
fn reader_index_pages_only_authorized_claims_and_survives_rebuild() {
    let at = fixture::base::NOW;
    let reader = fixture::base::principal(40);
    for i in 0..100 {
        let mut c = claim_for(
            i,
            fixture::base::principal(if i % 25 == 0 { 40 } else { 41 }),
        );
        save(&mut c, at);
    }
    rebuild();
    let first = operations(reader, false, None, 2).unwrap();
    assert_eq!(first.claims.len(), 2);
    assert!(first.claims.iter().all(|c| c.terms.actor == reader));
    let second = operations(reader, false, first.next, 2).unwrap();
    assert_eq!(second.claims.len(), 2);
    assert!(second.next.is_none());
    assert!(first.claims.last().unwrap().claim_id < second.claims[0].claim_id);
    assert!(operations(fixture::base::principal(42), false, None, 32)
        .unwrap()
        .claims
        .is_empty());
    assert_eq!(operations(reader, true, None, 32).unwrap().claims.len(), 32);
}

#[test]
fn terminal_cleanup_is_bounded_and_preserves_live_commitments_and_replay_identity() {
    let at = fixture::base::NOW;
    let actor = fixture::base::principal(40);
    let mut closed = vec![];
    for i in 0..33 {
        let mut c = claim_for(i, actor);
        c.cancel().unwrap();
        save(&mut c, at);
        closed.push(c);
    }
    let until = closed[0].retain_until_ms.unwrap();
    let mut unknown = claim_for(100, actor);
    unknown.product_reserved = true;
    unknown.observe(Eligibility::Eligible, at, at).unwrap();
    unknown
        .observe(
            Eligibility::Eligible,
            at + PANDA_COOLING_MS,
            at + PANDA_COOLING_MS,
        )
        .unwrap();
    unknown.preparing_apply(at + PANDA_COOLING_MS).unwrap();
    save(&mut unknown, at);
    let mut committed = claim_for(101, actor);
    committed.view.status = PandaClaimStatus::Terminated;
    committed.view.committed_until_ms = at + 365 * DAY;
    save(&mut committed, at);
    assert_eq!(prune_history(until - 1), 0);
    assert_eq!(prune_history(until), 32);
    assert_eq!(prune_history(until), 1);
    assert_eq!(prune_history(until), 0);
    rebuild();
    assert_eq!(live_claims(), 2);
    assert_eq!(load(unknown.view.claim_id).unwrap(), unknown);
    assert_eq!(load(committed.view.claim_id).unwrap(), committed);
    let c = &closed[0];
    assert_eq!(load(c.view.claim_id), Err(Error::ResultExpired));
    assert_eq!(
        replay(c.view.claim_id, &c.view.terms),
        Err(Error::ResultExpired)
    );
    let mut changed = c.view.terms.clone();
    changed.neuron_id = Hash::new([99; 32]);
    assert_eq!(
        replay(c.view.claim_id, &changed),
        Err(Error::IdempotencyConflict)
    );
    assert!(CERT.with_borrow(|t| t.get(&key(c.view.claim_id)).is_none()));
    assert_eq!(operations(actor, false, None, 32).unwrap().claims.len(), 2);
    assert_eq!(TOMBSTONES.with_borrow(|t| t.len()), 33);
}

#[test]
fn product_calls_have_one_inflight_request_and_per_actor_and_global_budgets() {
    let at = fixture::base::NOW;
    let actor = fixture::base::principal(1);
    let id = Hash::new([1; 32]);
    let guard = product_call(id, actor, at).unwrap();
    assert!(matches!(product_call(id, actor, at), Err(Error::Pending)));
    assert_eq!(LIMITS.with_borrow(|l| l.products), 1);
    drop(guard);
    for _ in 1..10 {
        drop(product_call(id, actor, at).unwrap());
    }
    assert!(matches!(
        product_call(id, actor, at),
        Err(Error::QuotaExceeded)
    ));
    for i in 0..190 {
        reserve_call(
            at,
            CallBudget::Product(Principal::from_slice(&(i + 1000u64).to_be_bytes())),
        )
        .unwrap();
    }
    assert!(matches!(
        product_call(id, fixture::base::principal(2), at),
        Err(Error::QuotaExceeded)
    ));
    drop(product_call(id, actor, at + MINUTE).unwrap());
}

#[test]
#[ignore = "native 1,000/10,000 terminal-history and indexed-query sample"]
fn history_profile() {
    use ic_stable_structures::Memory;
    let at = fixture::base::NOW;
    for count in [1_000u64, 10_000] {
        let present = CLAIMS.with_borrow(|t| t.len());
        for i in present..count {
            let mut c = claim_for(i, fixture::base::principal(9));
            c.cancel().unwrap();
            c.reservation_released = true;
            save(&mut c, at);
        }
        let start = std::time::Instant::now();
        rebuild();
        let rebuild_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = std::time::Instant::now();
        assert!(operations(fixture::base::principal(99), false, None, 32)
            .unwrap()
            .claims
            .is_empty());
        let query_ms = start.elapsed().as_secs_f64() * 1000.;
        let pages: u64 = [1, 3, 4, 6, 7, 8]
            .into_iter()
            .map(|id| memory(id).size())
            .sum();
        println!("MEMBERSHIP count={count} stable_bytes={} rebuild_ms={rebuild_ms:.3} empty_reader_ms={query_ms:.3}", pages*65536);
    }
    let after = at + APPLICATION_TTL_MS + HISTORY_RETENTION_MS;
    while prune_history(after) > 0 {}
    let start = std::time::Instant::now();
    rebuild();
    println!(
        "MEMBERSHIP compacted=10000 full={} tombstones={} rebuild_ms={:.3}",
        CLAIMS.with_borrow(|t| t.len()),
        TOMBSTONES.with_borrow(|t| t.len()),
        start.elapsed().as_secs_f64() * 1000.
    );
}

#[test]
fn save_touches_indexes_and_certified_view_only_when_they_change() {
    let request = fixture::claim();
    let id = panda_claim_id(&request.terms);
    let n = neuron(&request.terms);
    let mut c = Claim::new(id, request, PANDA_COOLING_MS);
    save(&mut c, fixture::base::NOW);
    let leaf = |id| CERT.with_borrow(|t| t.get(&key(id)).map(<[u8]>::to_vec));
    let certified = leaf(id);
    assert_eq!(live_claims(), 1);
    assert_eq!(
        NEURONS.with_borrow(|t| t.load(n.as_slice())),
        Some(vec![id])
    );
    // A private busy marker keeps the public revision and certified leaf.
    c.busy_until_ms = 99;
    save(&mut c, fixture::base::NOW);
    assert_eq!(load(id).unwrap().view.lease_revision, 1);
    assert_eq!(leaf(id), certified);
    c.cancel().unwrap();
    save(&mut c, fixture::base::NOW);
    let saved = load(id).unwrap();
    assert_eq!(saved.view.lease_revision, 2);
    assert_ne!(leaf(id), certified);
    assert_eq!(live_claims(), 0);
    assert!(NEURONS.with_borrow(|t| t.load(n.as_slice())).is_none());
}

#[test]
fn applying_claims_keep_capacity_until_rejected_or_expired() {
    for applied in [false, true] {
        let mut request = fixture::claim();
        request.terms.offer.operation_id = Hash::new([if applied { 91 } else { 90 }; 32]);
        let id = panda_claim_id(&request.terms);
        let mut c = Claim::new(id, request, PANDA_COOLING_MS);
        c.product_reserved = true;
        let start = fixture::base::NOW;
        c.observe(Eligibility::Eligible, start, start).unwrap();
        save(&mut c, fixture::base::NOW);
        let at = start + PANDA_COOLING_MS;
        c.observe(Eligibility::Eligible, at, at).unwrap();
        c.preparing_apply(at).unwrap();
        save(&mut c, fixture::base::NOW);
        assert_eq!(live_claims(), 1);
        assert_eq!(c.expire(at + 2 * DAY), Err(Error::ExecutionUnknown));
        rebuild();
        assert_eq!(live_claims(), 1);

        let decision = c.decision.as_ref().unwrap();
        let receipt = ProductReceipt {
            version: COMMERCE_VERSION,
            decision_id: decision.decision_id,
            decision_hash: product_decision_hash(decision),
            adapter: decision.offer.adapter,
            outcome: if applied {
                ProductOutcome::Applied {
                    business_revision: 1,
                    contract_id: Hash::new([90; 32]),
                    committed_until_ms: decision.offer.expires_at_ms,
                }
            } else {
                ProductOutcome::Rejected {
                    reason: ProductRejection::Expired,
                }
            },
            applied_at_ms: at,
        };
        c.accept(receipt).unwrap();
        save(&mut c, fixture::base::NOW);
        save(&mut c, fixture::base::NOW);
        assert_eq!(live_claims(), u64::from(applied));
        if applied {
            c.expire(c.view.committed_until_ms).unwrap();
            save(&mut c, fixture::base::NOW);
            save(&mut c, fixture::base::NOW);
        }
        assert_eq!(live_claims(), 0);
        rebuild();
        assert_eq!(live_claims(), 0);
    }
}
