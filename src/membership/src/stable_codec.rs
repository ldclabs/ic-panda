//! Compact stable-memory records are independent of public certification bytes.
use crate::{model::Claim, store::Config};
use cbor2::Cbor;
use dmsg_runtime::{stable_types::*, storage::StableCodec};
use dmsg_types::membership::*;

#[derive(Cbor)]
pub struct ClaimRepr {
    #[cbor(key = 0)]
    pub schema: u16,
    #[cbor(key = 1)]
    pub request: ClaimRequestRepr,
    #[cbor(key = 2)]
    pub policy: MembershipPolicyRepr,
    #[cbor(key = 3)]
    pub required_atomic: u128,
    #[cbor(key = 4)]
    pub view: ClaimViewRepr,
    #[cbor(key = 5)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooling_since_ms: Option<u64>,
    #[cbor(key = 6)]
    pub busy_until_ms: u64,
    #[cbor(key = 7)]
    pub generation: u64,
    #[cbor(key = 8)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<MembershipDecisionRepr>,
    #[cbor(key = 9)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<MembershipDecisionReceiptRepr>,
    #[cbor(key = 10)]
    pub budget_reserved: bool,
    #[cbor(key = 11)]
    pub last_issued_until_ms: u64,
}

impl StableCodec for Claim {
    type Repr = ClaimRepr;

    fn to_repr(&self) -> Self::Repr {
        ClaimRepr {
            schema: 2,
            request: self.request.to_repr(),
            policy: self.policy.to_repr(),
            required_atomic: self.required_atomic,
            view: self.view.to_repr(),
            cooling_since_ms: self.cooling_since_ms,
            busy_until_ms: self.busy_until_ms,
            generation: self.generation,
            decision: self.decision.to_repr(),
            receipt: self.receipt.to_repr(),
            budget_reserved: self.budget_reserved,
            last_issued_until_ms: self.last_issued_until_ms,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        assert_eq!(repr.schema, 2, "incompatible development state");
        Self {
            request: ClaimRequest::from_repr(repr.request),
            policy: MembershipPolicy::from_repr(repr.policy),
            required_atomic: repr.required_atomic,
            view: ClaimView::from_repr(repr.view),
            cooling_since_ms: repr.cooling_since_ms,
            busy_until_ms: repr.busy_until_ms,
            generation: repr.generation,
            decision: Option::<MembershipDecision>::from_repr(repr.decision),
            receipt: Option::<MembershipDecisionReceipt>::from_repr(repr.receipt),
            budget_reserved: repr.budget_reserved,
            last_issued_until_ms: repr.last_issued_until_ms,
        }
    }
}

#[derive(Cbor)]
pub struct Record<T> {
    #[cbor(key = 0)]
    pub schema: u16,
    #[cbor(key = 1)]
    pub value: T,
}

macro_rules! codec {
    ($t:ty) => {
        impl StableCodec for $t {
            type Repr = Record<Self>;

            fn to_repr(&self) -> Self::Repr {
                Record {
                    schema: 1,
                    value: self.clone(),
                }
            }

            fn from_repr(r: Self::Repr) -> Self {
                assert_eq!(r.schema, 1, "incompatible development state");
                r.value
            }
        }
    };
}
codec!(Config);

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use cbor2::Value;
    use dmsg_protocol::{canonical, membership::decision_digest};
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes, CompactStored, MapExt};
    use dmsg_types::{Environment, Hash};
    use ic_stable_structures::{StableBTreeMap, VectorMemory};

    fn claim(populated: bool) -> Claim {
        let now = 1_700_000_000_000;
        let beneficiary = Beneficiary {
            product_id: "dmsg".into(),
            authority_canister: Principal::from_slice(&[1, 1]),
            subject_schema: "account/1".into(),
            subject_bytes: vec![2; 12].into(),
        };
        let mut claim = Claim {
            request: ClaimRequest {
                authorization: MembershipIntent {
                    application_id: Hash::new([3; 32]),
                    environment: Environment::Production,
                    service_canister: Principal::from_slice(&[4, 1]),
                    beneficiary: beneficiary.clone(),
                    actor: Principal::from_slice(&[5, 1]),
                    action_digest: Hash::new([6; 32]),
                    nonce: Hash::new([7; 32]),
                    valid_until_ms: now + 60_000,
                },
                neuron_id: Hash::new([8; 32]),
                policy_version: 9,
                benefit_id: Hash::new([10; 32]),
                expected_business_revision: 11,
                term: TermRule::CalendarYear,
                change: ClaimChange::Start,
            },
            policy: MembershipPolicy {
                version: 9,
                product_id: beneficiary.product_id.clone(),
                benefit_id: Hash::new([10; 32]),
                threshold: Threshold::AnnualPrice {
                    price_cents: 12_000,
                    r_num: 200,
                    r_den: 1,
                },
                effective_at_ms: now - 60_000,
                subsidy_units: 13,
            },
            required_atomic: 2_400_000_000_000,
            view: ClaimView {
                schema: 1,
                home_membership: Principal::from_slice(&[4, 1]),
                claim_id: Hash::new([14; 32]),
                beneficiary,
                benefit_id: Hash::new([10; 32]),
                policy_version: 9,
                status: ClaimStatus::Checking,
                eligibility: Eligibility::Unverifiable,
                starts_at_ms: 0,
                expires_at_ms: 0,
                observed_at_ms: 0,
                valid_until_ms: 0,
                lease_revision: 0,
                decision_id: None,
                release_after_ms: 0,
            },
            cooling_since_ms: None,
            busy_until_ms: 0,
            generation: 15,
            decision: None,
            receipt: None,
            budget_reserved: false,
            last_issued_until_ms: 0,
        };
        if populated {
            claim.cooling_since_ms = Some(now - 3_900_000);
            claim.busy_until_ms = now + 30_000;
            claim.budget_reserved = true;
            claim.applying(now, now).unwrap();
            let decision = claim.decision.as_ref().unwrap();
            claim
                .accept_receipt(MembershipDecisionReceipt {
                    decision_id: decision.decision_id,
                    decision_digest: decision_digest(decision),
                    outcome: DecisionOutcome::Applied,
                    contract_id: Some(Hash::new([16; 32])),
                    starts_at_ms: decision.starts_at_ms,
                    expires_at_ms: decision.expires_at_ms,
                    business_revision: 12,
                    commitment_until_ms: decision.expires_at_ms,
                })
                .unwrap();
        }
        claim
    }

    fn assert_integer_fields(value: &Value) {
        match value {
            Value::Map(entries) => {
                for (key, value) in entries {
                    match key {
                        Value::Integer(_) => {}
                        // Enum discriminants retain their names; their payload fields
                        // must still use integer keys, just like nested structs.
                        Value::Text(name)
                            if entries.len() == 1
                                && matches!(
                                    name.as_str(),
                                    "FixedPanda"
                                        | "AnnualPrice"
                                        | "Fixed"
                                        | "Renew"
                                        | "Upgrade"
                                        | "Replace"
                                ) => {}
                        other => panic!("non-integer stable field key: {other:?}"),
                    }
                    assert_integer_fields(value);
                }
            }
            Value::Array(values) => values.iter().for_each(assert_integer_fields),
            Value::Tag(_, value) => assert_integer_fields(value),
            _ => {}
        }
    }

    fn assert_public_text_keys<T: serde::Serialize>(value: &T) {
        let Value::Map(entries) = cbor2::from_slice(&canonical(value)).unwrap() else {
            panic!("public record must be a map")
        };
        assert!(entries.iter().all(|(key, _)| matches!(key, Value::Text(_))));
    }

    #[test]
    fn claim_storage_is_recursive_compact_and_lossless() {
        for populated in [false, true] {
            let claim = claim(populated);
            let compact = compact_bytes(&claim);
            let old = cbor2::to_vec(&Record {
                schema: 1,
                value: claim.clone(),
            })
            .unwrap();
            let encoded: Value = cbor2::from_slice(&compact).unwrap();
            assert_integer_fields(&encoded);
            let Value::Map(entries) = encoded else {
                panic!("stable claim must be a map")
            };
            let keys: Vec<_> = entries.into_iter().map(|(key, _)| key).collect();
            let expected: Vec<_> = (0_u64..=11)
                .filter(|key| populated || !matches!(key, 5 | 8 | 9))
                .map(Value::from)
                .collect();
            assert_eq!(keys, expected);
            assert!(
                compact.len() * 100 <= old.len() * 70,
                "{} !<= 70% of {}",
                compact.len(),
                old.len()
            );
            eprintln!(
                "populated={populated}: {} -> {} bytes",
                old.len(),
                compact.len()
            );

            let restored: Claim = compact_from_bytes(&compact);
            assert_eq!(restored, claim);
            assert_eq!(canonical(&restored.view), canonical(&claim.view));
            assert_public_text_keys(&restored.view);
            assert_public_text_keys(&restored.request);
            assert_public_text_keys(&restored.request.authorization);
            assert_public_text_keys(&restored.view.beneficiary);
            assert_public_text_keys(&restored.policy);
            if let Some(decision) = &restored.decision {
                assert_public_text_keys(decision);
                assert_eq!(
                    decision_digest(decision),
                    restored.receipt.as_ref().unwrap().decision_digest
                );
                assert_public_text_keys(restored.receipt.as_ref().unwrap());
            }
        }
    }

    #[test]
    fn claim_nested_variants_and_large_amounts_round_trip() {
        let previous_claim = Hash::new([17; 32]);
        for threshold in [
            Threshold::FixedPanda { atomic: u128::MAX },
            Threshold::AnnualPrice {
                price_cents: u64::MAX,
                r_num: u128::MAX - 1,
                r_den: u128::MAX - 2,
            },
        ] {
            for term in [
                TermRule::CalendarYear,
                TermRule::Fixed {
                    starts_at_ms: 1_700_000_000_000,
                    expires_at_ms: 1_730_000_000_000,
                },
            ] {
                for change in [
                    ClaimChange::Start,
                    ClaimChange::Renew { previous_claim },
                    ClaimChange::Upgrade { previous_claim },
                    ClaimChange::Replace { previous_claim },
                ] {
                    let mut claim = claim(true);
                    claim.required_atomic = u128::MAX;
                    claim.policy.threshold = threshold.clone();
                    claim.request.term = term.clone();
                    claim.request.change = change.clone();
                    claim.view.status = ClaimStatus::Closing;
                    claim.view.eligibility = Eligibility::Ineligible;
                    let decision = claim.decision.as_mut().unwrap();
                    decision.kind = DecisionKind::Close;
                    decision.policy.threshold = threshold.clone();
                    decision.request.term = term.clone();
                    decision.request.change = change;
                    // A persisted decision is an independent snapshot, not an alias
                    // of the current request/policy/amount.
                    decision.request.expected_business_revision = 21;
                    decision.policy.version = 22;
                    decision.required_atomic = u128::MAX - 3;
                    let receipt = claim.receipt.as_mut().unwrap();
                    receipt.outcome = DecisionOutcome::Rejected;
                    receipt.contract_id = None;
                    let bytes = compact_bytes(&claim);
                    assert_integer_fields(&cbor2::from_slice(&bytes).unwrap());
                    assert_eq!(compact_from_bytes::<Claim>(&bytes), claim);
                }
            }
        }
    }

    #[test]
    fn claim_survives_stable_map_reopen() {
        let memory = VectorMemory::default();
        let claim = claim(true);
        {
            let mut claims =
                StableBTreeMap::<Vec<u8>, CompactStored<Claim>, _>::init(memory.clone());
            claims.put(claim.view.claim_id.as_slice(), &claim);
        }
        let claims = StableBTreeMap::<Vec<u8>, CompactStored<Claim>, _>::init(memory);
        assert_eq!(claims.load(claim.view.claim_id.as_slice()), Some(claim));
    }

    #[test]
    #[should_panic(expected = "incompatible development state")]
    fn claim_rejects_incompatible_schema() {
        let mut repr = claim(false).to_repr();
        repr.schema = 1;
        compact_from_bytes::<Claim>(&cbor2::to_vec(&repr).unwrap());
    }
}
