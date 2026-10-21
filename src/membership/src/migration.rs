//! One-time conversion of claims written before PANDA activation became immediate.
use crate::{claim::Claim, store};
use dmsg_protocol::canonical;
use dmsg_types::{
    integration::*,
    integration_billing::ProductAuthorizationRequest,
    integration_membership::{PandaApplicationTerms, PandaClaimStatus, PandaClaimView},
    membership::Eligibility,
    *,
};
use ic_stable_structures::{Storable, storable::Bound};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, ops::Bound as KeyBound};

/// Stable claim records written before immediate activation still carry cooling
/// and repair fields, and one status variant the current enum no longer has.
#[derive(Serialize, Deserialize)]
struct LegacyClaim {
    view: LegacyView,
    authorization: ProductAuthorizationRequest,
    cooling_ms: u64,
    decision: Option<ProductDecision>,
    product_reserved: bool,
    reservation_released: bool,
    generation: u64,
    busy_until_ms: u64,
    retain_until_ms: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct LegacyView {
    version: u16,
    claim_id: Hash,
    terms: PandaApplicationTerms,
    status: LegacyStatus,
    eligibility: Eligibility,
    observed_at_ms: u64,
    valid_until_ms: u64,
    lease_revision: u64,
    cooling_until_ms: Option<u64>,
    committed_until_ms: u64,
    repair_elapsed_ms: u64,
    receipt: Option<ProductReceipt>,
}

#[derive(Serialize, Deserialize)]
enum LegacyStatus {
    Checking,
    CoolingDown,
    Applying,
    Active,
    Terminated,
    Cancelled,
    Rejected,
    Released,
}

impl LegacyClaim {
    fn into_claim(self) -> Claim {
        let status = match self.view.status {
            LegacyStatus::Checking => PandaClaimStatus::Checking,
            // The old cooling period was only a wait; no Apply exists yet.
            LegacyStatus::CoolingDown => PandaClaimStatus::Checking,
            LegacyStatus::Applying => PandaClaimStatus::Applying,
            LegacyStatus::Active => PandaClaimStatus::Active,
            LegacyStatus::Terminated => PandaClaimStatus::Terminated,
            LegacyStatus::Cancelled => PandaClaimStatus::Cancelled,
            LegacyStatus::Rejected => PandaClaimStatus::Rejected,
            LegacyStatus::Released => PandaClaimStatus::Released,
        };
        Claim {
            view: PandaClaimView {
                version: self.view.version,
                claim_id: self.view.claim_id,
                terms: self.view.terms,
                status,
                eligibility: self.view.eligibility,
                observed_at_ms: self.view.observed_at_ms,
                valid_until_ms: self.view.valid_until_ms,
                lease_revision: self.view.lease_revision,
                cooling_until_ms: None,
                committed_until_ms: self.view.committed_until_ms,
                repair_elapsed_ms: 0,
                receipt: self.view.receipt,
            },
            authorization: self.authorization,
            decision: self.decision,
            product_reserved: self.product_reserved,
            reservation_released: self.reservation_released,
            generation: self.generation,
            busy_until_ms: self.busy_until_ms,
            retain_until_ms: self.retain_until_ms,
        }
    }
}

/// Raw stable value, so migration can inspect records without asking the
/// current `Claim` decoder to accept the old schema.
struct Raw(Vec<u8>);

impl Storable for Raw {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.0)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        Self(bytes.into_owned())
    }
}

/// Rewrite legacy claim records in the claim map. A record already in the
/// current schema is left alone, so an interrupted upgrade can be retried.
pub(crate) fn claims() {
    let mut claims: ic_stable_structures::StableBTreeMap<Vec<u8>, Raw, _> =
        ic_stable_structures::StableBTreeMap::init(store::memory(1));
    migrate_records(&mut claims, |claim| {
        store::CERT.with_borrow_mut(|t| {
            t.insert(store::key(claim.view.claim_id), &canonical(&claim.view));
        });
    });
}

fn migrate_records<M: ic_stable_structures::Memory>(
    claims: &mut ic_stable_structures::StableBTreeMap<Vec<u8>, Raw, M>,
    mut migrated: impl FnMut(&Claim),
) {
    let mut after: Option<Vec<u8>> = None;
    loop {
        let start = after
            .as_ref()
            .map_or(KeyBound::Unbounded, |key| KeyBound::Excluded(key.clone()));
        let batch: Vec<(Vec<u8>, Vec<u8>)> = claims
            .range((start, KeyBound::Unbounded))
            .take(64)
            .map(|entry| (entry.key().clone(), entry.value().0.clone()))
            .collect();
        if batch.is_empty() {
            return;
        }
        for (key, bytes) in &batch {
            if let Ok(old) = cbor2::from_slice::<LegacyClaim>(bytes) {
                let claim = old.into_claim();
                let bytes = cbor2::to_vec(&claim).expect("claim migration encoding");
                claims.insert(key.clone(), Raw(bytes));
                migrated(&claim);
            } else {
                let claim: Claim = cbor2::from_slice(bytes).expect("claim schema");
                let bytes = cbor2::to_vec(&claim).expect("claim rewrite encoding");
                claims.insert(key.clone(), Raw(bytes));
                migrated(&claim);
            }
        }
        after = batch.last().map(|(key, _)| key.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use dmsg_types::{integration_membership::PandaServiceConfig, membership::MembershipInit};
    use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap};

    fn legacy_cooling_claim() -> LegacyClaim {
        let request = fixture::claim();
        LegacyClaim {
            view: LegacyView {
                version: COMMERCE_VERSION,
                claim_id: Hash::new([7; 32]),
                terms: request.terms.clone(),
                status: LegacyStatus::CoolingDown,
                eligibility: Eligibility::Eligible,
                observed_at_ms: fixture::base::NOW,
                valid_until_ms: fixture::base::NOW + PANDA_LEASE_MS,
                lease_revision: 1,
                cooling_until_ms: Some(fixture::base::NOW),
                committed_until_ms: 0,
                repair_elapsed_ms: 0,
                receipt: None,
            },
            authorization: request.authorization,
            cooling_ms: 65 * MINUTE,
            decision: None,
            product_reserved: true,
            reservation_released: false,
            generation: 0,
            busy_until_ms: 0,
            retain_until_ms: None,
        }
    }

    #[test]
    fn a_cooling_claim_becomes_checking_and_keeps_its_terms() {
        let old = legacy_cooling_claim();
        let terms = old.view.terms.clone();
        let bytes = cbor2::to_vec(&old).expect("legacy encoding");
        let decoded: LegacyClaim = cbor2::from_slice(&bytes).expect("legacy decoding");
        let claim = decoded.into_claim();
        assert_eq!(claim.view.status, PandaClaimStatus::Checking);
        assert_eq!(claim.view.terms, terms);
        assert!(claim.product_reserved);
        let current = cbor2::to_vec(&claim).expect("current encoding");
        let decoded: Claim = cbor2::from_slice(&current).expect("current decoding");
        assert_eq!(decoded.view.status, PandaClaimStatus::Checking);
    }

    #[test]
    fn the_claim_map_rewrites_legacy_bytes() {
        let old = legacy_cooling_claim();
        let id = old.view.claim_id;
        let mut records: StableBTreeMap<Vec<u8>, Raw, _> =
            StableBTreeMap::init(DefaultMemoryImpl::default());
        records.insert(
            id.as_slice().to_vec(),
            Raw(cbor2::to_vec(&old).expect("legacy encoding")),
        );
        let mut migrated = vec![];
        migrate_records(&mut records, |claim| migrated.push(claim.view.claim_id));
        assert_eq!(migrated, vec![id]);
        let bytes = records.get(&id.as_slice().to_vec()).expect("record").0;
        let claim: Claim = cbor2::from_slice(&bytes).expect("current decoding");
        assert_eq!(claim.view.status, PandaClaimStatus::Checking);
        assert_eq!(claim.view.terms, old.view.terms);
    }

    #[derive(Serialize)]
    struct LegacyConfig {
        schema: u16,
        init: MembershipInit,
        service: Option<PandaServiceConfig>,
        sns_verified: bool,
        sns_verified_at_ms: u64,
        paused: bool,
        application_hour: u64,
        applications: u64,
    }

    #[test]
    fn a_config_without_a_claim_schema_reads_as_zero() {
        let init = MembershipInit {
            environment: Environment::Local,
            governance: fixture::base::principal(90),
            sns_root: fixture::base::principal(90),
            panda_ledger: fixture::base::principal(90),
        };
        let old = LegacyConfig {
            schema: crate::store::STABLE_SCHEMA,
            init,
            service: None,
            sns_verified: false,
            sns_verified_at_ms: 0,
            paused: false,
            application_hour: 0,
            applications: 0,
        };
        let bytes = cbor2::to_vec(&old).expect("legacy config encoding");
        let config: crate::store::Config =
            cbor2::from_slice(&bytes).expect("legacy config decoding");
        assert_eq!(config.claim_schema, 0);
    }
    #[derive(Serialize)]
    struct V2View {
        version: u16,
        claim_id: Hash,
        terms: PandaApplicationTerms,
        status: PandaClaimStatus,
        eligibility: Eligibility,
        observed_at_ms: u64,
        valid_until_ms: u64,
        lease_revision: u64,
        committed_until_ms: u64,
        receipt: Option<ProductReceipt>,
    }

    #[derive(Serialize)]
    struct V2Claim {
        view: V2View,
        authorization: ProductAuthorizationRequest,
        decision: Option<ProductDecision>,
        product_reserved: bool,
        reservation_released: bool,
        generation: u64,
        busy_until_ms: u64,
        retain_until_ms: Option<u64>,
    }

    #[test]
    fn a_claim_written_without_the_legacy_fields_is_rewritten() {
        let base = legacy_cooling_claim().into_claim();
        let old = V2Claim {
            view: V2View {
                version: base.view.version,
                claim_id: base.view.claim_id,
                terms: base.view.terms.clone(),
                status: base.view.status.clone(),
                eligibility: base.view.eligibility.clone(),
                observed_at_ms: base.view.observed_at_ms,
                valid_until_ms: base.view.valid_until_ms,
                lease_revision: base.view.lease_revision,
                committed_until_ms: base.view.committed_until_ms,
                receipt: None,
            },
            authorization: base.authorization,
            decision: None,
            product_reserved: true,
            reservation_released: false,
            generation: 0,
            busy_until_ms: 0,
            retain_until_ms: None,
        };
        let bytes = cbor2::to_vec(&old).expect("v2 encoding");
        let decoded: Claim = cbor2::from_slice(&bytes).expect("v2 decoding");
        assert_eq!(decoded.view.cooling_until_ms, None);
        assert_eq!(decoded.view.repair_elapsed_ms, 0);
        let id = decoded.view.claim_id;
        let mut records: StableBTreeMap<Vec<u8>, Raw, _> =
            StableBTreeMap::init(DefaultMemoryImpl::default());
        records.insert(id.as_slice().to_vec(), Raw(bytes));
        let mut migrated = vec![];
        migrate_records(&mut records, |claim| migrated.push(claim.view.claim_id));
        assert_eq!(migrated, vec![id]);
        let bytes = records.get(&id.as_slice().to_vec()).expect("record").0;
        let claim: Claim = cbor2::from_slice(&bytes).expect("rewritten claim");
        assert_eq!(claim.view.status, PandaClaimStatus::Checking);
        assert_eq!(claim.view.cooling_until_ms, None);
        assert_eq!(claim.view.repair_elapsed_ms, 0);
    }
}
