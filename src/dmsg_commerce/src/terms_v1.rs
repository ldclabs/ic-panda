//! Frozen display response for clients released before Google Play support.
//! New sources belong in the versioned query, never in this Candid variant.
use candid::CandidType;
use dmsg_types::{app_store::StoreEnvironment, billing::*, membership::Eligibility, *};

#[derive(CandidType)]
pub(crate) enum ContractSourceV1 {
    Cash {
        order_id: Hash,
    },
    Sns {
        claim_id: Hash,
    },
    AppStore {
        transaction_id: Hash,
        environment: Option<StoreEnvironment>,
    },
    Compensation,
}

#[derive(CandidType)]
pub(crate) struct MembershipContractV1 {
    term_starts_at_ms: u64,
    resource_pauses: Vec<(u64, Option<u64>)>,
    contract_id: Hash,
    plan: PlanVersion,
    source: ContractSourceV1,
    starts_at_ms: u64,
    expires_at_ms: u64,
    terminated_at_ms: Option<u64>,
    eligibility: Eligibility,
    observed_at_ms: u64,
    qualified_until_ms: u64,
}

#[derive(CandidType)]
pub(crate) struct MembershipTermsV1 {
    contracts: Vec<MembershipContractV1>,
    addons: Vec<StorageAddon>,
    refund: Option<RefundWindow>,
}

impl TryFrom<MembershipTerms> for MembershipTermsV1 {
    type Error = Error;

    fn try_from(terms: MembershipTerms) -> Result<Self> {
        let contracts = terms
            .contracts
            .into_iter()
            .map(|c| {
                let source = match c.source {
                    ContractSource::Cash { order_id } => ContractSourceV1::Cash { order_id },
                    ContractSource::Sns { claim_id } => ContractSourceV1::Sns { claim_id },
                    ContractSource::AppStore {
                        transaction_id,
                        environment,
                    } => ContractSourceV1::AppStore {
                        transaction_id,
                        environment,
                    },
                    ContractSource::Compensation => ContractSourceV1::Compensation,
                    // Do not omit a paid contract or disguise its payment source.
                    ContractSource::GooglePlay { .. } => return Err(Error::UnsupportedProtocol),
                };
                Ok(MembershipContractV1 {
                    term_starts_at_ms: c.term_starts_at_ms,
                    resource_pauses: c.resource_pauses,
                    contract_id: c.contract_id,
                    plan: c.plan,
                    source,
                    starts_at_ms: c.starts_at_ms,
                    expires_at_ms: c.expires_at_ms,
                    terminated_at_ms: c.terminated_at_ms,
                    eligibility: c.eligibility,
                    observed_at_ms: c.observed_at_ms,
                    qualified_until_ms: c.qualified_until_ms,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            contracts,
            addons: terms.addons,
            refund: terms.refund,
        })
    }
}
