//! One immutable PANDA application and its irreversible post-Apply commitment.
use dmsg_protocol::{commerce_v2::*, integration::*, *};
use dmsg_types::{integration::*, integration_membership::*, membership::Eligibility, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claim {
    pub view: PandaClaimView,
    pub authorization: dmsg_types::integration_billing::ProductAuthorizationRequest,
    pub cooling_ms: u64,
    pub decision: Option<ProductDecision>,
    pub budget_reserved: bool,
    pub product_reserved: bool,
    pub reservation_released: bool,
    pub generation: u64,
    pub busy_until_ms: u64,
}

impl Claim {
    pub fn new(request: PandaClaimRequest, cooling_ms: u64) -> Self {
        let id = panda_claim_id(&request.terms);
        Self {
            view: PandaClaimView {
                version: 2,
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
            cooling_ms,
            decision: None,
            budget_reserved: true,
            product_reserved: false,
            reservation_released: false,
            generation: 0,
            busy_until_ms: 0,
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
            PandaClaimStatus::Checking | PandaClaimStatus::CoolingDown => {
                Some(self.view.terms.quote.application_deadline_ms)
            }
            PandaClaimStatus::Active | PandaClaimStatus::Terminated => {
                Some(self.view.committed_until_ms)
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
        let revision = self
            .view
            .lease_revision
            .checked_add(1)
            .ok_or(Error::QuotaExceeded)?;
        let pending = matches!(
            self.view.status,
            PandaClaimStatus::Checking | PandaClaimStatus::CoolingDown
        );
        let elapsed = if !pending
            && eligibility != Eligibility::Unverifiable
            && self.view.eligibility == Eligibility::Ineligible
        {
            self.view
                .repair_elapsed_ms
                .saturating_add(observed - self.view.observed_at_ms)
        } else {
            self.view.repair_elapsed_ms
        };
        self.view.lease_revision = revision;
        self.view.eligibility = eligibility.clone();
        self.view.observed_at_ms = observed;
        self.view.valid_until_ms = lease;
        if pending {
            match eligibility {
                Eligibility::Eligible => {
                    self.view
                        .cooling_until_ms
                        .get_or_insert(observed.saturating_add(self.cooling_ms));
                    self.view.status = PandaClaimStatus::CoolingDown;
                }
                Eligibility::Ineligible => self.view.status = PandaClaimStatus::Rejected,
                Eligibility::Unverifiable => {}
            }
        } else {
            self.view.repair_elapsed_ms = elapsed;
            if elapsed >= REPAIR_WINDOW_MS {
                self.view.status = PandaClaimStatus::Terminated;
                self.view.valid_until_ms = at;
            } else if eligibility == Eligibility::Eligible {
                self.view.repair_elapsed_ms = 0;
            }
        }
        Ok(())
    }

    pub fn preparing_apply(&mut self, at: u64) -> Result<()> {
        ensure(
            self.view.status == PandaClaimStatus::CoolingDown
                && self.view.eligibility == Eligibility::Eligible
                && self.view.cooling_until_ms.is_some_and(|ready| ready <= at)
                && at < self.view.valid_until_ms
                && at < self.view.terms.quote.application_deadline_ms
                && self.product_reserved,
            Error::MembershipIneligible,
        )?;
        let terms = &self.view.terms;
        let decision = ProductDecision {
            version: 2,
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
            self.view.status = PandaClaimStatus::Rejected;
            self.view.valid_until_ms = 0;
        }
        self.view.receipt = Some(receipt);
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<()> {
        if self.view.status == PandaClaimStatus::Cancelled {
            return Ok(());
        }
        ensure(
            self.decision.is_none()
                && matches!(
                    self.view.status,
                    PandaClaimStatus::Checking | PandaClaimStatus::CoolingDown
                ),
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
#[path = "../../dmsg_types/tests/support/commerce.rs"]
mod fixture;
#[cfg(test)]
mod tests {
    use super::*;
    use fixture::base::*;
    #[test]
    fn cooling_and_unknown_apply_never_release_an_accepted_commitment() {
        let mut c = Claim::new(fixture::claim(), PANDA_COOLING_MS);
        c.product_reserved = true;
        c.observe(Eligibility::Eligible, NOW, NOW).unwrap();
        assert!(c.preparing_apply(NOW + PANDA_COOLING_MS - 1).is_err());
        let at = NOW + PANDA_COOLING_MS;
        c.observe(Eligibility::Eligible, at, at).unwrap();
        c.preparing_apply(at).unwrap();
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
            applied_at_ms: at,
        };
        c.accept(receipt.clone()).unwrap();
        c.accept(receipt).unwrap();
        let end = c.view.committed_until_ms;
        assert!(c.cancel().is_err());
        assert!(c.expire(end - 1).is_err());
        c.observe(Eligibility::Ineligible, at + 1, at + 1).unwrap();
        c.observe(Eligibility::Ineligible, at + 8 * DAY, at + 8 * DAY)
            .unwrap();
        assert_eq!(c.view.status, PandaClaimStatus::Terminated);
        assert!(c.holds());
        assert_eq!(c.view.committed_until_ms, end);
        c.observe(Eligibility::Eligible, at + 9 * DAY, at + 9 * DAY)
            .unwrap();
        assert_eq!(c.view.status, PandaClaimStatus::Terminated);
        c.expire(end).unwrap();
        c.expire(end + 1).unwrap();
        assert_eq!(c.view.status, PandaClaimStatus::Released);
        assert!(!c.holds());
    }

    #[test]
    fn unverified_observations_pause_repair_without_extending_old_lease() {
        let mut c = Claim::new(fixture::claim(), PANDA_COOLING_MS);
        c.view.status = PandaClaimStatus::Active;
        c.view.committed_until_ms = c.view.terms.offer.expires_at_ms;
        c.observe(Eligibility::Ineligible, NOW, NOW).unwrap();
        c.observe(Eligibility::Unverifiable, NOW + DAY, NOW + DAY)
            .unwrap();
        assert_eq!(c.view.repair_elapsed_ms, 0);
        assert_eq!(c.view.valid_until_ms, NOW + DAY);
        c.observe(Eligibility::Ineligible, NOW + 20 * DAY, NOW + 20 * DAY)
            .unwrap();
        assert_eq!(c.view.repair_elapsed_ms, 0);
        assert_eq!(c.view.status, PandaClaimStatus::Active);
    }
}
