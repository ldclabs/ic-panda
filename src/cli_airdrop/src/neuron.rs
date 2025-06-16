use candid::{CandidType, Principal};
use ic_agent::Agent;
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;

use super::{agent::query_call, valid_principal};

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct NeuronId {
    pub id: ByteBuf,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ListNeurons {
    pub limit: u32,
    pub start_page_at: Option<NeuronId>,
    pub of_principal: Option<Principal>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ListNeuronsResponse {
    pub neurons: Vec<Neuron>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct NeuronPermission {
    pub principal: Option<Principal>,
    pub permission_type: Vec<i32>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct Followees {
    pub followees: Vec<NeuronId>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct DisburseMaturityInProgress {
    pub amount_e8s: u64,
    pub timestamp_of_disbursement_seconds: u64,
    pub account_to_disburse_to: Option<Account>,
    pub finalize_disbursement_timestamp_seconds: Option<u64>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub enum DissolveState {
    WhenDissolvedTimestampSeconds(u64),
    DissolveDelaySeconds(u64),
}

#[derive(CandidType, Default, Clone, Debug, Deserialize, Serialize)]
pub struct Neuron {
    pub id: Option<NeuronId>,
    pub permissions: Vec<NeuronPermission>,
    pub cached_neuron_stake_e8s: u64,
    pub neuron_fees_e8s: u64,
    pub created_timestamp_seconds: u64,
    pub aging_since_timestamp_seconds: u64,
    pub followees: BTreeMap<u64, Followees>,
    pub maturity_e8s_equivalent: u64,
    pub voting_power_percentage_multiplier: u64,
    pub source_nns_neuron_id: Option<u64>,
    pub staked_maturity_e8s_equivalent: Option<u64>,
    pub auto_stake_maturity: Option<bool>,
    pub vesting_period_seconds: Option<u64>,
    pub disburse_maturity_in_progress: Vec<DisburseMaturityInProgress>,
    pub dissolve_state: Option<DissolveState>,
}

impl Neuron {
    // from:
    // https://dashboard.internetcomputer.org/canister/dwv6s-6aaaa-aaaaq-aacta-cai
    // get_nervous_system_parameters
    const MAX_DISSOLVE_DELAY_SECONDS: u64 = 63_115_200;
    const MAX_NEURON_AGE_FOR_AGE_BONUS_SECONDS: u64 = 15_780_096;
    const MIN_DISSOLVE_DELAY_SECONDS: u64 = 2_630_016;

    pub fn get_id(&self) -> Option<(ByteBuf, Principal)> {
        match self.id.as_ref() {
            Some(NeuronId { id }) => self
                .permissions
                .iter()
                .find_map(|p| {
                    if let Some(principal) = p.principal.as_ref() {
                        if valid_principal(principal) {
                            Some(*principal)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .map(|principal| (id.clone(), principal)),
            None => None,
        }
    }

    // voting_power = (staked_tokens + staked_maturity) × (1 + age_bonus) × (1 + dissolve_delay_bonus)
    pub fn voting_power(&self, now_seconds: u64) -> u64 {
        let stake_e8s =
            self.cached_neuron_stake_e8s + self.staked_maturity_e8s_equivalent.unwrap_or(0);
        if stake_e8s == 0 {
            return 0;
        }

        let mut age_bonus = if self.aging_since_timestamp_seconds > now_seconds {
            0f64
        } else {
            let age = now_seconds - self.aging_since_timestamp_seconds;
            if age > Self::MAX_NEURON_AGE_FOR_AGE_BONUS_SECONDS {
                0.2f64
            } else {
                (age as f64 / Self::MAX_NEURON_AGE_FOR_AGE_BONUS_SECONDS as f64) * 0.2
            }
        };

        let dissolve_delay_seconds = match self.dissolve_state {
            Some(DissolveState::DissolveDelaySeconds(seconds)) => seconds,
            Some(DissolveState::WhenDissolvedTimestampSeconds(at_seconds)) => {
                at_seconds.saturating_sub(now_seconds)
            }
            _ => 0,
        };

        let dissolve_delay_bonus = if dissolve_delay_seconds <= Self::MIN_DISSOLVE_DELAY_SECONDS {
            // no bonus if neuron is already dissolved
            age_bonus = 0.0;
            0.0
        } else if dissolve_delay_seconds >= Self::MAX_DISSOLVE_DELAY_SECONDS {
            1f64
        } else {
            dissolve_delay_seconds as f64 / Self::MAX_DISSOLVE_DELAY_SECONDS as f64
        };

        let voting_power = stake_e8s as f64 * (1f64 + age_bonus) * (1f64 + dissolve_delay_bonus);
        voting_power.round() as u64
    }
}

// "dwv6s-6aaaa-aaaaq-aacta-cai" ICPanda DAO canister id
#[allow(dead_code)]
static DAO_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 166, 1, 1]);

#[derive(Clone)]
pub struct NeuronAgent {
    pub agent: Agent,
    pub canister_id: Principal,
}

impl NeuronAgent {
    pub async fn list_neurons(&self, request: ListNeurons) -> Result<ListNeuronsResponse, String> {
        let output = query_call(&self.agent, &self.canister_id, "list_neurons", (request,)).await?;
        Ok(output)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn neuron_get_principal() {
        let principal =
            Principal::from_text("ntihc-z566a-oifro-4tvua-vkbdv-ndi7q-tx2h6-yblw7-t6ofd-g7fwa-gqe")
                .unwrap();
        let id = ByteBuf::from(
            hex::decode("c461817a8e32773e8704dab175629ccc2f78c5879d7ce5ac50ad92343c5a527e")
                .unwrap(),
        );
        let neuron = Neuron {
            id: Some(NeuronId { id: id.clone() }),
            permissions: vec![
                NeuronPermission {
                    principal: Some(principal),
                    permission_type: vec![1],
                },
                NeuronPermission {
                    principal: Some(Principal::from_text("iqhbb-kqaaa-aaaar-a3xaa-cai").unwrap()),
                    permission_type: vec![1],
                },
            ],
            ..Default::default()
        };
        assert_eq!(neuron.get_id(), Some((id, principal)));
    }

    #[test]
    fn neuron_voting_power() {
        let now_seconds = 1730003368u64;
        let neuron = Neuron {
            cached_neuron_stake_e8s: 0,
            aging_since_timestamp_seconds: 1_729_959_774,
            dissolve_state: Some(DissolveState::DissolveDelaySeconds(5_184_000)),
            ..Default::default()
        };
        assert_eq!(neuron.voting_power(now_seconds), 0);

        let neuron = Neuron {
            cached_neuron_stake_e8s: 15_554_499_930_000,
            aging_since_timestamp_seconds: 1_729_959_774,
            dissolve_state: Some(DissolveState::DissolveDelaySeconds(5_184_000)),
            ..Default::default()
        };
        assert_eq!(neuron.voting_power(now_seconds), 16841376965563);

        let neuron = Neuron {
            cached_neuron_stake_e8s: 900_000_000_000_000,
            aging_since_timestamp_seconds: 18_446_744_073_709_551_615,
            dissolve_state: Some(DissolveState::WhenDissolvedTimestampSeconds(1_792_458_567)),
            ..Default::default()
        };
        assert_eq!(neuron.voting_power(now_seconds), 1790588623659594);
    }
}
