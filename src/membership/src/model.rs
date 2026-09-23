use dmsg_protocol::{billing::next_year, membership::*, *};
use dmsg_types::{membership::*, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Claim {
    pub request: ClaimRequest,
    pub policy: MembershipPolicy,
    pub required_atomic: u128,
    pub view: ClaimView,
    pub cooling_since_ms: Option<u64>,
    pub busy_until_ms: u64,
    pub generation: u64,
    pub decision: Option<MembershipDecision>,
    pub receipt: Option<MembershipDecisionReceipt>,
    pub budget_reserved: bool,
    pub last_issued_until_ms: u64,
}

impl Claim {
    pub fn pending(&self) -> bool {
        matches!(
            self.view.status,
            ClaimStatus::Checking | ClaimStatus::CoolingDown | ClaimStatus::Applying
        )
    }

    pub fn terminal(&self) -> bool {
        matches!(
            self.view.status,
            ClaimStatus::Released | ClaimStatus::Rejected
        )
    }

    /// Only known commitments have a time-based release. Unknown decisions stay occupied.
    pub fn release_deadline(&self) -> Option<u64> {
        match self.view.status {
            ClaimStatus::Checking | ClaimStatus::CoolingDown => {
                Some(self.request.authorization.valid_until_ms)
            }
            ClaimStatus::Active => Some(self.view.expires_at_ms.max(self.view.release_after_ms)),
            ClaimStatus::Closing
                if self
                    .receipt
                    .as_ref()
                    .is_some_and(|r| r.outcome == DecisionOutcome::Applied) =>
            {
                Some(self.view.release_after_ms)
            }
            _ => None,
        }
    }

    pub fn release(&mut self) -> Result<()> {
        self.generation = self.generation.checked_add(1).ok_or(Error::QuotaExceeded)?;
        self.busy_until_ms = 0;
        self.view.status = ClaimStatus::Released;
        Ok(())
    }

    pub fn prepare_close(&mut self, at: u64, apply_by: u64) -> Result<bool> {
        if matches!(
            self.view.status,
            ClaimStatus::Closing | ClaimStatus::Released
        ) {
            return Ok(false);
        }
        ensure(
            self.view.status == ClaimStatus::Active,
            Error::VersionConflict,
        )?;
        self.generation = self.generation.checked_add(1).ok_or(Error::QuotaExceeded)?;
        let id = digest(
            "membership/decision-id/v1",
            &(self.view.claim_id, "close", self.generation),
        );
        let mut d = self.decision.clone().ok_or(Error::IntegrityFailed)?;
        d.decision_id = id;
        d.kind = DecisionKind::Close;
        d.apply_by_ms = apply_by;
        self.decision = Some(d);
        self.receipt = None;
        self.busy_until_ms = 0;
        self.view.status = ClaimStatus::Closing;
        self.view.decision_id = Some(id);
        self.view.valid_until_ms = at;
        Ok(true)
    }

    pub fn term(&self, now: u64) -> Result<(u64, u64)> {
        match self.request.term {
            TermRule::CalendarYear => Ok((now, next_year(now)?)),
            TermRule::Fixed {
                starts_at_ms,
                expires_at_ms,
            } => {
                ensure(
                    starts_at_ms < expires_at_ms
                        && now < expires_at_ms
                        && expires_at_ms <= next_year(starts_at_ms)?,
                    invalid("claim interval"),
                )?;
                Ok((
                    if matches!(
                        self.request.change,
                        ClaimChange::Upgrade { .. } | ClaimChange::Replace { .. }
                    ) {
                        now.max(starts_at_ms)
                    } else {
                        starts_at_ms
                    },
                    expires_at_ms,
                ))
            }
        }
    }

    pub fn observe(&mut self, status: Eligibility, observed: u64, now: u64) -> Result<()> {
        self.view.eligibility = status;
        self.view.observed_at_ms = observed;
        self.view.valid_until_ms = if self.view.eligibility == Eligibility::Eligible {
            observed
                .saturating_add(MAX_LEASE_MS)
                .min(self.view.expires_at_ms)
        } else {
            now
        };
        if self.view.eligibility == Eligibility::Eligible {
            ensure(now < self.view.valid_until_ms, Error::MembershipStale)?;
            self.last_issued_until_ms = self.last_issued_until_ms.max(self.view.valid_until_ms);
            self.view.release_after_ms = self.view.release_after_ms.max(self.view.valid_until_ms);
        }
        Ok(())
    }

    pub fn applying(&mut self, now: u64, observed: u64) -> Result<()> {
        let (start, end) = self.term(now)?;
        let id = digest("membership/decision-id/v1", &(self.view.claim_id, "apply"));
        self.view.starts_at_ms = start;
        self.view.expires_at_ms = end;
        self.observe(Eligibility::Eligible, observed, now)?;
        self.view.status = ClaimStatus::Applying;
        self.view.decision_id = Some(id);
        self.decision = Some(MembershipDecision {
            decision_id: id,
            claim_id: self.view.claim_id,
            kind: DecisionKind::Apply,
            request: self.request.clone(),
            policy: self.policy.clone(),
            required_atomic: self.required_atomic,
            starts_at_ms: start,
            expires_at_ms: end,
            apply_by_ms: now
                .saturating_add(5 * MINUTE)
                .min(self.request.authorization.valid_until_ms),
            observed_at_ms: observed,
            qualification_until_ms: self.view.valid_until_ms,
        });
        Ok(())
    }

    pub fn accept_receipt(&mut self, receipt: MembershipDecisionReceipt) -> Result<()> {
        let d = self.decision.as_ref().ok_or(Error::IntegrityFailed)?;
        ensure(
            self.receipt.is_none()
                && matches!(
                    (&self.view.status, &d.kind),
                    (ClaimStatus::Applying, DecisionKind::Apply)
                        | (ClaimStatus::Closing, DecisionKind::Close)
                ),
            Error::VersionConflict,
        )?;
        ensure(
            receipt.decision_id == d.decision_id && receipt.decision_digest == decision_digest(d),
            Error::IntegrityFailed,
        )?;
        if receipt.outcome == DecisionOutcome::Applied {
            ensure(
                receipt.starts_at_ms == d.starts_at_ms && receipt.expires_at_ms == d.expires_at_ms,
                Error::IntegrityFailed,
            )?;
            self.view.status = if d.kind == DecisionKind::Close {
                ClaimStatus::Closing
            } else {
                ClaimStatus::Active
            };
            self.view.release_after_ms = receipt.commitment_until_ms.max(self.last_issued_until_ms);
        } else {
            // A durable adapter rejection proves this exact action did not grant benefits.
            self.view.status = if d.kind == DecisionKind::Close {
                ClaimStatus::Active
            } else {
                ClaimStatus::Rejected
            };
        }
        self.receipt = Some(receipt);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stable_codec::tests::claim;

    #[test]
    fn unknown_decisions_never_expire_and_released_claims_reject_receipts() {
        let mut c = claim(true);
        let at = c.view.observed_at_ms;
        assert_eq!(c.release_deadline(), Some(c.view.expires_at_ms));
        assert!(c.prepare_close(at, at + MINUTE).unwrap());
        assert_eq!(c.release_deadline(), None);
        let decision = c.decision.clone().unwrap();
        let receipt = MembershipDecisionReceipt {
            decision_id: decision.decision_id,
            decision_digest: decision_digest(&decision),
            outcome: DecisionOutcome::Applied,
            contract_id: Some(c.view.claim_id),
            starts_at_ms: decision.starts_at_ms,
            expires_at_ms: decision.expires_at_ms,
            business_revision: 1,
            commitment_until_ms: at,
        };
        c.accept_receipt(receipt.clone()).unwrap();
        assert_eq!(c.release_deadline(), Some(c.last_issued_until_ms));
        let generation = c.generation;
        c.release().unwrap();
        assert!(c.generation > generation);
        assert_eq!(c.release_deadline(), None);
        assert!(!c.prepare_close(at, at + MINUTE).unwrap());
        assert_eq!(c.accept_receipt(receipt), Err(Error::VersionConflict));
        assert_eq!(c.view.status, ClaimStatus::Released);
        c = claim(false);
        c.applying(at, at).unwrap();
        assert_eq!(c.release_deadline(), None);
    }
}
