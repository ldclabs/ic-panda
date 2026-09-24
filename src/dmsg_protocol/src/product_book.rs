//! Shared product-side interval CAS. Product roles/pricing are checked by its adapter before entry.
use crate::{commerce_v2::*, integration::*};
use dmsg_types::{integration::*, integration_billing::*, membership::Beneficiary, *};
use serde::{Deserialize, Serialize};

/// One accepted application owns its interval until a definite outcome.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reservation {
    /// Original authorized bill; a fresh device approval may not change these terms.
    pub request: ProductAuthorizationRequest,
    /// Exclusive reservation end while no Apply is in flight.
    pub until_ms: u64,
    /// Once assigned, expiry cannot erase an unknown Apply.
    pub decision_id: Option<Hash>,
}

/// Only live contracts stay here; adapters archive completed records separately.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductBook {
    /// Fixed product subject.
    pub beneficiary: Beneficiary,
    /// Changes only when business terms/permissions change or a contract commits.
    pub business_revision: u64,
    /// At most current and next live contracts.
    pub contracts: Vec<SubscriptionContract>,
    /// At most one accepted uncommitted billing interval per subject.
    pub reservation: Option<Reservation>,
}

/// Account approval purpose must match the decision's settlement source.
fn check_purpose(r: &Reservation, decision: &ProductDecision) -> Result<()> {
    let method = match decision.source {
        SettlementSource::Cash { .. } => SettlementMethod::Cash,
        SettlementSource::Panda { .. } => SettlementMethod::Panda,
    };
    ensure(
        r.request.account_approval.purpose == approval_purpose(&method),
        Error::Forbidden,
    )
}

impl ProductBook {
    /// New product subject with a defined CAS revision.
    pub fn new(beneficiary: Beneficiary, revision: u64) -> Self {
        Self {
            beneficiary,
            business_revision: revision,
            contracts: vec![],
            reservation: None,
        }
    }

    /// Product-initiated configuration changes invalidate uncommitted offers.
    pub fn bump(&mut self) -> Result<()> {
        self.business_revision = self
            .business_revision
            .checked_add(1)
            .ok_or(Error::QuotaExceeded)?;
        Ok(())
    }

    /// Whether a live contract overlaps the half-open offer interval.
    fn overlaps(&self, offer: &BillingOffer) -> bool {
        self.contracts.iter().any(|c| {
            c.status != SubscriptionStatus::Cancelled
                && c.offer.starts_at_ms < offer.expires_at_ms
                && offer.starts_at_ms < c.offer.expires_at_ms
        })
    }

    /// Never evicts an unknown Apply. Historical records must already exist in the adapter archive.
    pub fn prune(&mut self, at: u64) {
        self.contracts
            .retain(|c| c.offer.expires_at_ms > at && c.status != SubscriptionStatus::Cancelled);
        if self
            .reservation
            .as_ref()
            .is_some_and(|r| r.decision_id.is_none() && at >= r.until_ms)
        {
            self.reservation = None;
        }
    }

    /// Reserve only after both independent authorities approve; quoting does not call this method.
    pub fn reserve(
        &mut self,
        request: ProductAuthorizationRequest,
        until_ms: u64,
        at: u64,
    ) -> Result<()> {
        if let Some(old) = &self.reservation {
            if old.request.offer.operation_id == request.offer.operation_id {
                ensure(
                    old.request.offer == request.offer
                        && old.until_ms == until_ms
                        && old.request.account_approval.approving_account
                            == request.account_approval.approving_account
                        && old.request.account_approval.actor == request.account_approval.actor
                        && old.request.account_approval.service == request.account_approval.service
                        && old.request.account_approval.action_digest
                            == request.account_approval.action_digest
                        && old.request.product_approval == request.product_approval,
                    Error::IdempotencyConflict,
                )?;
                return Ok(());
            }
            ensure(
                old.decision_id.is_none() && at >= old.until_ms,
                Error::Pending,
            )?;
        }
        ensure(
            request.offer.beneficiary == self.beneficiary
                && request.offer.expected_business_revision == self.business_revision,
            Error::VersionConflict,
        )?;
        ensure(
            at < until_ms
                && until_ms <= request.offer.expires_at_ms
                && until_ms
                    <= request
                        .offer
                        .accept_by_ms
                        .saturating_add(APPLICATION_TTL_MS),
            Error::Expired,
        )?;
        ensure(!self.overlaps(&request.offer), Error::IntervalReserved)?;
        ensure(
            self.contracts
                .iter()
                .filter(|c| c.offer.expires_at_ms > at && c.status != SubscriptionStatus::Cancelled)
                .count()
                < 2,
            Error::QuotaExceeded,
        )?;
        self.prune(at);
        self.reservation = Some(Reservation {
            request,
            until_ms,
            decision_id: None,
        });
        Ok(())
    }

    /// Mark before any delivery-time role/qualification await; a sweep cannot erase it.
    pub fn begin_apply(&mut self, decision: &ProductDecision, at: u64) -> Result<()> {
        validate_decision(decision, at)?;
        let r = self.reservation.as_ref().ok_or(Error::NotFound)?;
        check_purpose(r, decision)?;
        ensure(
            r.request.offer == decision.offer && at < r.until_ms,
            Error::VersionConflict,
        )?;
        ensure(
            r.decision_id.is_none_or(|id| id == decision.decision_id),
            Error::IdempotencyConflict,
        )?;
        self.reservation.as_mut().expect("checked").decision_id = Some(decision.decision_id);
        Ok(())
    }

    /// Release only an unused reservation; it cannot end an applied contract or unknown Apply.
    pub fn release(&mut self, offer: &BillingOffer) -> Result<()> {
        if let Some(r) = &self.reservation {
            if r.request.offer.operation_id == offer.operation_id {
                ensure(r.request.offer == *offer, Error::IdempotencyConflict)?;
                ensure(r.decision_id.is_none(), Error::ExecutionUnknown)?;
                self.reservation = None;
            }
        }
        Ok(())
    }

    /// All fallible contract construction precedes mutation; the adapter saves this with its receipt.
    pub fn apply(
        &mut self,
        decision: &ProductDecision,
        source: SubscriptionSource,
        at: u64,
    ) -> Result<(SubscriptionContract, ProductReceipt)> {
        validate_decision(decision, at)?;
        let r = self.reservation.as_ref().ok_or(Error::NotFound)?;
        check_purpose(r, decision)?;
        ensure(
            r.request.offer == decision.offer
                && r.decision_id == Some(decision.decision_id)
                && decision.offer.expected_business_revision == self.business_revision,
            Error::VersionConflict,
        )?;
        ensure(!self.overlaps(&decision.offer), Error::IntervalReserved)?;
        let revision = self
            .business_revision
            .checked_add(1)
            .ok_or(Error::QuotaExceeded)?;
        let c = contract(decision, source, revision, at)?;
        let receipt = ProductReceipt {
            version: 2,
            decision_id: decision.decision_id,
            decision_hash: product_decision_hash(decision),
            adapter: decision.offer.adapter,
            outcome: ProductOutcome::Applied {
                business_revision: revision,
                contract_id: c.contract_id,
                committed_until_ms: c.offer.expires_at_ms,
            },
            applied_at_ms: at,
        };
        self.prune(at);
        self.business_revision = revision;
        self.contracts.push(c.clone());
        self.reservation = None;
        Ok((c, receipt))
    }

    /// A definite rejection retires only that decision's reservation.
    pub fn reject(
        &mut self,
        decision: &ProductDecision,
        reason: ProductRejection,
        at: u64,
    ) -> ProductReceipt {
        if self.reservation.as_ref().is_some_and(|r| {
            r.request.offer == decision.offer
                && r.decision_id.is_none_or(|id| id == decision.decision_id)
        }) {
            self.reservation = None;
        }
        rejected(decision, reason, at)
    }

    /// PANDA is never cancellable. Unstarted cash terms have not issued any rights.
    pub fn cancel_cash(
        &mut self,
        order_id: Hash,
        contract_id: Hash,
        decision_hash: Hash,
        at: u64,
    ) -> Result<CashCancellationReceipt> {
        let c = self
            .contracts
            .iter()
            .find(|c| c.contract_id == contract_id)
            .ok_or(Error::NotFound)?;
        ensure(
            matches!(c.source,SubscriptionSource::Cash {order_id:id,..} if id==order_id),
            Error::Forbidden,
        )?;
        let allowed = matches!(
            c.status,
            SubscriptionStatus::Active | SubscriptionStatus::Terminated
        ) && at < c.offer.starts_at_ms
            && c.max_issued_until_ms <= at;
        let revision = if allowed {
            self.business_revision
                .checked_add(1)
                .ok_or(Error::QuotaExceeded)?
        } else {
            self.business_revision
        };
        if allowed {
            self.contracts
                .iter_mut()
                .find(|c| c.contract_id == contract_id)
                .expect("checked")
                .status = SubscriptionStatus::Cancelled;
            self.business_revision = revision;
        }
        Ok(CashCancellationReceipt {
            order_id,
            contract_id,
            decision_hash,
            cancelled_at_ms: at,
            cancelled: allowed,
            business_revision: revision,
        })
    }
}

/// A transport error does not establish this receipt. Only an authenticated adapter may issue one.
pub fn rejected(decision: &ProductDecision, reason: ProductRejection, at: u64) -> ProductReceipt {
    ProductReceipt {
        version: 2,
        decision_id: decision.decision_id,
        decision_hash: product_decision_hash(decision),
        adapter: decision.offer.adapter,
        outcome: ProductOutcome::Rejected { reason },
        applied_at_ms: at,
    }
}
