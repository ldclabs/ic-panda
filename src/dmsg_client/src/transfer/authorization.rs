//! The host's one-use permission to send a reviewed library snapshot.

use super::{Category, MAX_ENTRIES, Permit, SessionId, library, manifest_digest};
use crate::approval::{Action, HIGH_RISK_TTL_MS, State};
use crate::hold::Hold;
use crate::now_ms;
use dmsg_types::{Error, Hash, Result, ensure};
use serde_json::{Value as Json, json};

/// Ask in the approval center before exporting to this receiving session.
/// The existing high-risk flow checks system presence where available.
///
/// # Errors
/// Returns the library's errors, or `Error::Locked` while locked.
pub async fn request<H: Hold>(h: &H, source: &str, to: Hash, session: SessionId) -> Result<Json> {
    h.with(|c| {
        let own = c.device_id()?;
        ensure(
            own != to,
            Error::InvalidInput("the receiver is this device".into()),
        )?;
        let entries = library::entries(c)?;
        ensure(entries.len() <= MAX_ENTRIES, Error::QuotaExceeded)?;
        let now = now_ms();
        let request = c.approval_create_at(
            source,
            Action::DeviceTransfer {
                session: hex::encode(session),
                to_device: to,
                manifest: manifest_digest(&entries.iter().collect::<Vec<_>>()),
                bytes: entries.iter().map(|e| e.blob_bytes).sum(),
            },
            now,
            now + HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}

/// Consume a completed approval for this exact peer, session and manifest.
/// The permit also pins the current unlock session through connection setup.
///
/// # Errors
/// Returns `Error::Forbidden` before approval or after use, `Error::Expired`
/// after its deadline, and `Error::VersionConflict` if the library changed.
pub async fn claim<H: Hold>(h: &H, id: &str, to: Hash, session: SessionId) -> Result<Permit> {
    h.with(|c| {
        c.keys()?;
        let mut request = c.approval_record(id)?;
        ensure(
            request.state == State::Done && request.delivery_report.is_none(),
            Error::Forbidden,
        )?;
        let now = now_ms();
        ensure(now < request.expires_at, Error::Expired)?;
        let Action::DeviceTransfer {
            session: approved,
            to_device,
            manifest,
            ..
        } = &request.action
        else {
            return Err(Error::Forbidden);
        };
        ensure(
            *to_device == to && *approved == hex::encode(session),
            Error::IntegrityFailed,
        )?;
        let entries = library::entries(c)?;
        ensure(
            manifest_digest(&entries.iter().collect::<Vec<_>>()) == *manifest,
            Error::VersionConflict,
        )?;
        let permit = Permit {
            to,
            session,
            manifest: *manifest,
            generation: c.session_generation,
            device: c.workspace.meta.device_id,
        };
        request.delivery_report =
            Some(json!({"transfer_started_at": now, "categories": Category::ALL}));
        c.save_approval(&request)?;
        Ok(permit)
    })
    .await
}
