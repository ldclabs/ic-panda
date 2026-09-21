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
        self.view.lease_revision = self
            .view
            .lease_revision
            .checked_add(1)
            .ok_or(Error::QuotaExceeded)?;
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
