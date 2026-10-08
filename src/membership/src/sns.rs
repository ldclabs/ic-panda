//! Minimal Candid projection of DFINITY IC 2967c1cc9ba88fd85f196a09d05ea941bbabe830
//! rs/sns/governance/proto/ic_sns_governance/pb/v1/governance.proto. Only the fields the
//! qualification reads are decoded; a reply they no longer fit is unverifiable.
use candid::{CandidType, Principal};
use dmsg_types::{membership::Eligibility, *};
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NeuronId {
    pub id: Vec<u8>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NeuronPermission {
    pub principal: Option<Principal>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Neuron {
    pub id: Option<NeuronId>,
    pub permissions: Vec<NeuronPermission>,
    pub cached_neuron_stake_e8s: u64,
    pub neuron_fees_e8s: u64,
    pub dissolve_state: Option<DissolveState>,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct GovernanceError {
    pub error_type: i32,
    pub error_message: String,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub enum NeuronResult {
    Neuron(Neuron),
    Error(GovernanceError),
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct GetNeuronResponse {
    pub result: Option<NeuronResult>,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct GetNeuronRequest {
    pub neuron_id: Option<NeuronId>,
}

#[derive(CandidType, Deserialize)]
pub struct SnsCanisters {
    pub root: Option<Principal>,
    pub governance: Option<Principal>,
    pub ledger: Option<Principal>,
}

#[derive(CandidType, Serialize)]
pub struct ListRequest {}

/// The actor only has to be listed on the neuron, alongside any other principals; the
/// global occupancy keeps one neuron from supporting two benefits. The earliest unlock
/// must not precede the membership end `end` (exclusive), so the neuron stays locked
/// for the entire paid interval.
pub fn assess(
    n: &Neuron,
    id: Hash,
    actor: Principal,
    required: u128,
    end: u64,
    observed: u64,
) -> Eligibility {
    let result = (|| -> Result<bool> {
        ensure(
            n.id.as_ref().is_some_and(|v| v.id == id.as_slice()),
            Error::IntegrityFailed,
        )?;
        let unlock = match n
            .dissolve_state
            .as_ref()
            .ok_or(Error::UnsupportedProtocol)?
        {
            DissolveState::DissolveDelaySeconds(s) => observed
                .checked_add(s.checked_mul(1000).ok_or(Error::IntegrityFailed)?)
                .ok_or(Error::IntegrityFailed)?,
            DissolveState::WhenDissolvedTimestampSeconds(s) => {
                s.checked_mul(1000).ok_or(Error::IntegrityFailed)?
            }
        };
        Ok(n.permissions.iter().any(|p| p.principal == Some(actor))
            && u128::from(n.cached_neuron_stake_e8s.saturating_sub(n.neuron_fees_e8s)) >= required
            && unlock >= end)
    })();
    match result {
        Ok(true) => Eligibility::Eligible,
        Ok(false) => Eligibility::Ineligible,
        Err(_) => Eligibility::Unverifiable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neuron() -> Neuron {
        Neuron {
            id: Some(NeuronId { id: vec![1; 32] }),
            permissions: vec![NeuronPermission {
                principal: Some(Principal::from_slice(&[1])),
            }],
            cached_neuron_stake_e8s: 110,
            neuron_fees_e8s: 10,
            dissolve_state: Some(DissolveState::DissolveDelaySeconds(10)),
        }
    }

    fn check(n: &Neuron) -> Eligibility {
        assess(
            n,
            Hash::new([1; 32]),
            Principal::from_slice(&[1]),
            100,
            11_000,
            1000,
        )
    }

    #[test]
    fn net_principal_and_exact_unlock_boundary() {
        let mut n = neuron();
        assert_eq!(check(&n), Eligibility::Eligible);
        n.neuron_fees_e8s += 1;
        assert_eq!(check(&n), Eligibility::Ineligible);
        n.neuron_fees_e8s = 10;
        n.dissolve_state = Some(DissolveState::WhenDissolvedTimestampSeconds(11));
        assert_eq!(check(&n), Eligibility::Eligible);
        assert_eq!(
            assess(
                &n,
                Hash::new([1; 32]),
                Principal::from_slice(&[1]),
                100,
                11_001,
                1000
            ),
            Eligibility::Ineligible
        );
    }

    #[test]
    fn a_listed_actor_qualifies_beside_the_owner_and_an_unlisted_one_does_not() {
        let mut n = neuron();
        n.permissions.insert(
            0,
            NeuronPermission {
                principal: Some(Principal::from_slice(&[2])),
            },
        );
        assert_eq!(check(&n), Eligibility::Eligible);
        n.permissions.remove(1);
        assert_eq!(check(&n), Eligibility::Ineligible);
    }

    #[test]
    fn missing_dissolve_state_and_overflow_are_unverifiable() {
        let mut n = neuron();
        n.dissolve_state = Some(DissolveState::DissolveDelaySeconds(u64::MAX));
        assert_eq!(check(&n), Eligibility::Unverifiable);
        n = neuron();
        n.dissolve_state = None;
        assert_eq!(check(&n), Eligibility::Unverifiable);
        n = neuron();
        n.id = Some(NeuronId { id: vec![2; 32] });
        assert_eq!(check(&n), Eligibility::Unverifiable);
    }
}
