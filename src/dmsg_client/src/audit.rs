//! The audit record (plan R38, §6.9): who did what on this device, when and
//! through which way in — the owner from the CLI or the desktop, a hosted
//! agent through its MCP session. It covers capability grants and their
//! revocation, an agent's credential, approval decisions and what agents
//! call; the entries name methods, tools and IDs and never content,
//! secrets or recovery material.
//!
//! The record stays on this device and is readable while locked, since
//! resident agents act while locked; nothing syncs. It keeps a year, at
//! most [`KEEP`] entries.

use crate::client::Client;
use crate::now_ms;
use crate::store::AuditRow;
use dmsg_types::{Result, DAY};

/// Entries kept at most.
pub const KEEP: u64 = 10_000;
/// Entries kept at most this long.
const RETAIN_MS: u64 = 365 * DAY;
/// Most entries one read returns.
pub const MAX_PAGE: usize = 500;

impl Client {
    /// Record that `actor` did `action` through `via`. The record is best
    /// effort: a storage failure never fails what it records.
    pub fn audit(
        &self,
        actor: &str,
        via: &str,
        action: &str,
        target: Option<&str>,
        detail: Option<&str>,
        outcome: &str,
    ) {
        let now = now_ms();
        let row = AuditRow {
            seq: 0,
            at: now,
            actor: actor.into(),
            via: via.into(),
            action: action.into(),
            target: target.map(str::to_string),
            detail: detail.map(str::to_string),
            outcome: outcome.into(),
        };
        let _ = self
            .workspace
            .store
            .put_audit(&row, now.saturating_sub(RETAIN_MS), KEEP);
    }

    /// Audit entries before sequence `before` (all without it), newest
    /// first, of `actor` when given.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn audit_list(
        &self,
        before: Option<u64>,
        actor: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AuditRow>> {
        self.workspace
            .store
            .audit(before.unwrap_or(u64::MAX), actor, limit.clamp(1, MAX_PAGE))
    }
}
