//! One immutable PANDA application and its irreversible post-Apply commitment.
use dmsg_protocol::{integration::*, *};
use dmsg_types::{integration::*, integration_membership::*, membership::Eligibility, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claim {
    pub view: PandaClaimView,
    pub authorization: dmsg_types::integration_billing::ProductAuthorizationRequest,
    pub decision: Option<ProductDecision>,
    pub product_reserved: bool,
    pub reservation_released: bool,
    pub generation: u64,
    pub busy_until_ms: u64,
    pub retain_until_ms: Option<u64>,
}

impl Claim {
    pub fn new(id: Hash, request: PandaClaimRequest) -> Self {
        Self {
            view: PandaClaimView {
                version: COMMERCE_VERSION,
                claim_id: id,
                terms: request.terms,
                status: PandaClaimStatus::Checking,
                eligibility: Eligibility::Unverifiable,
                observed_at_ms: 0,
                valid_until_ms: 0,
                lease_revision: 1,
                cooling_until_ms: None,
                committed_until_ms: 0,
                repair_elapsed_ms: 0,
                receipt: None,
            },
            authorization: request.authorization,
            decision: None,
            product_reserved: false,
            reservation_released: false,
            generation: 0,
            busy_until_ms: 0,
            retain_until_ms: None,
        }
    }

    pub fn holds(&self) -> bool {
        !matches!(
            self.view.status,
            PandaClaimStatus::Cancelled | PandaClaimStatus::Rejected | PandaClaimStatus::Released
        )
    }

    pub fn release_at(&self) -> Option<u64> {
        match self.view.status {
            PandaClaimStatus::Checking => Some(self.view.terms.quote.application_deadline_ms),
            PandaClaimStatus::Active => Some(self.view.committed_until_ms),
            // Keep the neuron occupied until the last lease issued before the
            // ineligible observation has expired. No new lease is issued.
            PandaClaimStatus::Terminated => {
                Some(self.view.observed_at_ms.saturating_add(PANDA_LEASE_MS))
            }
            _ => None,
        }
    }

    pub fn observe(&mut self, eligibility: Eligibility, observed: u64, at: u64) -> Result<()> {
        ensure(
            observed <= at && observed >= self.view.observed_at_ms,
            Error::PolicyStale,
        )?;
        if !self.holds() || self.view.status == PandaClaimStatus::Terminated {
            return Ok(());
        }
        let end = self.view.terms.offer.expires_at_ms;
        let lease = if eligibility == Eligibility::Eligible {
            observed.saturating_add(PANDA_LEASE_MS).min(end)
        } else {
            at
        };
        if eligibility == Eligibility::Eligible {
            ensure(at < lease, Error::MembershipStale)?;
        }
        let pending = self.view.status == PandaClaimStatus::Checking;
        self.view.eligibility = eligibility.clone();
        self.view.observed_at_ms = observed;
        self.view.valid_until_ms = lease;
        if pending {
            match eligibility {
                Eligibility::Eligible => self.view.status = PandaClaimStatus::Checking,
                Eligibility::Ineligible => self.view.status = PandaClaimStatus::Rejected,
                Eligibility::Unverifiable => self.view.status = PandaClaimStatus::Checking,
            }
        } else {
            match eligibility {
                Eligibility::Eligible => self.view.status = PandaClaimStatus::Active,
                Eligibility::Ineligible => {
                    self.view.status = PandaClaimStatus::Terminated;
                    self.view.valid_until_ms = at;
                }
                Eligibility::Unverifiable => self.view.status = PandaClaimStatus::Active,
            }
        }
        Ok(())
    }

    pub fn preparing_apply(&mut self, at: u64) -> Result<()> {
        ensure(
            self.view.status == PandaClaimStatus::Checking
                && self.view.eligibility == Eligibility::Eligible
                && at < self.view.valid_until_ms
                && at < self.view.terms.quote.application_deadline_ms
                && self.product_reserved,
            Error::MembershipIneligible,
        )?;
        let terms = &self.view.terms;
        let decision = ProductDecision {
            version: COMMERCE_VERSION,
            offer: terms.offer.clone(),
            decision_id: digest("dmsg/panda/apply/v2", &self.view.claim_id),
            source: SettlementSource::Panda {
                claim_id: self.view.claim_id,
                quote_hash: panda_quote_hash(&terms.quote),
                committed_until_ms: terms.quote.committed_until_ms,
                lease_until_ms: self.view.valid_until_ms,
            },
            decided_at_ms: at,
            apply_by_ms: terms
                .quote
                .application_deadline_ms
                .min(at.saturating_add(5 * MINUTE)),
        };
        self.decision = Some(decision);
        self.view.status = PandaClaimStatus::Applying;
        Ok(())
    }

    pub fn accept(&mut self, receipt: ProductReceipt) -> Result<()> {
        if let Some(old) = &self.view.receipt {
            return ensure(*old == receipt, Error::IdempotencyConflict);
        }
        ensure(
            self.view.status == PandaClaimStatus::Applying,
            Error::VersionConflict,
        )?;
        let decision = self.decision.as_ref().ok_or(Error::IntegrityFailed)?;
        match_product_receipt(&receipt, decision)?;
        if matches!(receipt.outcome, ProductOutcome::Applied { .. }) {
            self.view.status = PandaClaimStatus::Active;
            self.view.committed_until_ms = self.view.terms.offer.expires_at_ms;
        } else {
            self.reject();
        }
        self.view.receipt = Some(receipt);
        Ok(())
    }

    /// A definite product refusal proves that no rights were delivered.
    pub fn reject(&mut self) {
        self.view.status = PandaClaimStatus::Rejected;
        self.view.valid_until_ms = 0;
    }

    pub fn cancel(&mut self) -> Result<()> {
        if self.view.status == PandaClaimStatus::Cancelled {
            return Ok(());
        }
        // A decision exists from Applying onwards, so pending status alone proves none was prepared.
        ensure(
            self.view.status == PandaClaimStatus::Checking,
            Error::Forbidden,
        )?;
        self.generation = self.generation.checked_add(1).ok_or(Error::QuotaExceeded)?;
        self.busy_until_ms = 0;
        self.view.status = PandaClaimStatus::Cancelled;
        self.view.valid_until_ms = 0;
        Ok(())
    }

    pub fn expire(&mut self, at: u64) -> Result<()> {
        if !self.holds() {
            return Ok(());
        }
        let deadline = self.release_at().ok_or(Error::ExecutionUnknown)?;
        ensure(at >= deadline, Error::Forbidden)?;
        self.generation = self.generation.checked_add(1).ok_or(Error::QuotaExceeded)?;
        self.busy_until_ms = 0;
        self.view.status = if self.view.committed_until_ms > 0 {
            PandaClaimStatus::Released
        } else {
            PandaClaimStatus::Cancelled
        };
        self.view.valid_until_ms = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{self, base::*};

    #[test]
    fn an_eligible_observation_can_apply_immediately() {
        let mut c = Claim::new(Hash::new([7; 32]), fixture::claim());
        c.product_reserved = true;
        c.observe(Eligibility::Eligible, NOW, NOW).unwrap();
        c.preparing_apply(NOW).unwrap();
        assert_eq!(c.view.status, PandaClaimStatus::Applying);
    }

    #[test]
    fn ineligible_after_activation_terminates_and_releases_after_the_lease() {
        let mut c = Claim::new(Hash::new([7; 32]), fixture::claim());
        c.view.status = PandaClaimStatus::Active;
        c.view.committed_until_ms = c.view.terms.offer.expires_at_ms;
        c.observe(Eligibility::Ineligible, NOW, NOW).unwrap();
        assert_eq!(c.view.status, PandaClaimStatus::Terminated);
        assert!(c.holds());
        assert_eq!(c.release_at(), Some(NOW + PANDA_LEASE_MS));
        assert!(c.expire(NOW + PANDA_LEASE_MS - 1).is_err());
        c.expire(NOW + PANDA_LEASE_MS).unwrap();
        assert_eq!(c.view.status, PandaClaimStatus::Released);
        assert!(!c.holds());
        c.observe(
            Eligibility::Eligible,
            NOW + 2 * PANDA_LEASE_MS,
            NOW + 2 * PANDA_LEASE_MS,
        )
        .unwrap();
        assert_eq!(c.view.status, PandaClaimStatus::Released);
    }

    #[test]
    fn unverifiable_does_not_terminate_and_does_not_extend_the_lease() {
        let mut c = Claim::new(Hash::new([7; 32]), fixture::claim());
        c.view.status = PandaClaimStatus::Active;
        c.view.committed_until_ms = c.view.terms.offer.expires_at_ms;
        c.observe(Eligibility::Eligible, NOW, NOW).unwrap();
        c.observe(Eligibility::Unverifiable, NOW + DAY, NOW + DAY)
            .unwrap();
        assert_eq!(c.view.status, PandaClaimStatus::Active);
        assert_eq!(c.view.eligibility, Eligibility::Unverifiable);
        assert_eq!(c.view.valid_until_ms, NOW + DAY);
    }

    #[test]
    fn a_prepared_apply_cannot_be_cancelled_or_expired() {
        let mut c = Claim::new(Hash::new([7; 32]), fixture::claim());
        c.product_reserved = true;
        c.observe(Eligibility::Eligible, NOW, NOW).unwrap();
        c.preparing_apply(NOW).unwrap();
        assert!(c.cancel().is_err());
        assert_eq!(c.expire(NOW + 2 * DAY), Err(Error::ExecutionUnknown));
        let d = c.decision.as_ref().unwrap();
        let receipt = ProductReceipt {
            version: 2,
            decision_id: d.decision_id,
            decision_hash: product_decision_hash(d),
            adapter: d.offer.adapter,
            outcome: ProductOutcome::Applied {
                business_revision: 10,
                contract_id: Hash::new([90; 32]),
                committed_until_ms: d.offer.expires_at_ms,
            },
            applied_at_ms: NOW,
        };
        c.accept(receipt.clone()).unwrap();
        c.accept(receipt).unwrap();
        assert_eq!(c.view.status, PandaClaimStatus::Active);
    }
}
