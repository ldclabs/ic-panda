//! Frozen v1 and v2 display projections. New sources use the open v3 record.
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
pub(crate) enum ContractSourceV2 {
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
    GooglePlay {
        transaction_id: Hash,
        token_hash: Hash,
        environment: crate::PlayEnvironment,
    },
    Compensation,
}

impl TryFrom<ContractSource> for ContractSourceV1 {
    type Error = Error;

    fn try_from(source: ContractSource) -> Result<Self> {
        Ok(match source {
            ContractSource::Cash { order_id } => Self::Cash { order_id },
            ContractSource::Sns { claim_id } => Self::Sns { claim_id },
            ContractSource::AppStore {
                transaction_id,
                environment,
            } => Self::AppStore {
                transaction_id,
                environment,
            },
            ContractSource::Compensation => Self::Compensation,
            ContractSource::GooglePlay { .. } | ContractSource::Promo { .. } => {
                return Err(Error::UnsupportedProtocol);
            }
        })
    }
}

impl TryFrom<ContractSource> for ContractSourceV2 {
    type Error = Error;

    fn try_from(source: ContractSource) -> Result<Self> {
        Ok(match source {
            ContractSource::Cash { order_id } => Self::Cash { order_id },
            ContractSource::Sns { claim_id } => Self::Sns { claim_id },
            ContractSource::AppStore {
                transaction_id,
                environment,
            } => Self::AppStore {
                transaction_id,
                environment,
            },
            ContractSource::GooglePlay {
                transaction_id,
                token_hash,
                environment,
            } => Self::GooglePlay {
                transaction_id,
                token_hash,
                environment,
            },
            ContractSource::Compensation => Self::Compensation,
            ContractSource::Promo { .. } => return Err(Error::UnsupportedProtocol),
        })
    }
}

#[derive(CandidType)]
pub(crate) struct LegacyContract<S> {
    term_starts_at_ms: u64,
    resource_pauses: Vec<(u64, Option<u64>)>,
    contract_id: Hash,
    plan: PlanVersion,
    source: S,
    starts_at_ms: u64,
    expires_at_ms: u64,
    terminated_at_ms: Option<u64>,
    eligibility: Eligibility,
    observed_at_ms: u64,
    qualified_until_ms: u64,
}

#[derive(CandidType)]
pub(crate) struct LegacyTerms<S> {
    contracts: Vec<LegacyContract<S>>,
    addons: Vec<StorageAddon>,
    refund: Option<RefundWindow>,
}

impl<S: CandidType + TryFrom<ContractSource, Error = Error>> TryFrom<MembershipTerms>
    for LegacyTerms<S>
{
    type Error = Error;

    fn try_from(terms: MembershipTerms) -> Result<Self> {
        let contracts = terms
            .contracts
            .into_iter()
            .map(|c| {
                Ok(LegacyContract {
                    term_starts_at_ms: c.term_starts_at_ms,
                    resource_pauses: c.resource_pauses,
                    contract_id: c.contract_id,
                    plan: c.plan,
                    source: S::try_from(c.source)?,
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

pub(crate) type MembershipTermsV1 = LegacyTerms<ContractSourceV1>;
pub(crate) type MembershipTermsV2 = LegacyTerms<ContractSourceV2>;
