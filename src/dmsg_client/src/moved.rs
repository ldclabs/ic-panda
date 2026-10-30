//! Copies set aside under `moved/` ([`crate::hardware`]), opened as the
//! device each was. The user home gives the login that device's unlock
//! secret until the device is revoked, so the copy opens here without its
//! old hardware, unless the device bound its workspace to that hardware.
//! Its file library imports through the device transfer's merge, and its
//! device can approve this one as its successor, the one thing the copy
//! still signs.

use crate::approval::{Action, HIGH_RISK_TTL_MS};
use crate::client::{Client, Slot};
use crate::crypto::Key;
use crate::hardware::{MOVED, plain_name};
use crate::hold::{Hold, Unlocked};
use crate::now_ms;
use crate::transfer::library::{self, Outcome, PHASES};
use crate::transfer::{MAX_WANT, Report};
use dmsg_types::user::{Capability, ControllerRole, DeviceInput, HardwarePurpose};
use dmsg_types::{Error, Hash, Result, ensure};
use serde_json::Value as Json;
use zeroize::Zeroizing;

/// Mutable handover journals are separate from the preserved source.
pub(crate) const OPERATIONS: &str = "moved-operations";

impl Client {
    /// The account a copy must belong to: this device's, or the one its
    /// login found while this device waits to join.
    fn moved_account(&self) -> Result<(candid::Principal, dmsg_types::AccountId)> {
        match self.account() {
            Ok(account) => Ok((account.home, account.id)),
            Err(_) => self.located.ok_or(Error::AuthRequired),
        }
    }

    /// A client over copy `id` as the device it was, with this client's
    /// login, unlocked with that device's `secret`.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown copy, `Error::IntegrityFailed`
    /// for a copy of another account or environment, and `Error::Forbidden`
    /// for a copy bound to its device's hardware.
    pub(crate) fn moved_client(&self, id: &str, secret: &Key) -> Result<Client> {
        ensure(plain_name(id), Error::NotFound)?;
        let (home, account) = self.moved_account()?;
        let dir = self.dir.join(MOVED);
        let mut copy = Client::open(self.config.clone(), &dir, None)?;
        copy.login = self.login.clone();
        let path = dir.join(id);
        copy.workspace = crate::workspace::Workspace::load(
            crate::store::Store::read_only(&path.join(crate::client::WORKSPACE))?,
            &self.config.environment,
            now_ms(),
        )?;
        copy.slot = Slot::Moved(path.clone());
        copy.blobs = Some(crate::library::blobs::Blobs::new(path.join("blobs")));
        ensure(
            copy.workspace.meta.account.as_ref().map(|a| (a.home, a.id)) == Some((home, account)),
            Error::IntegrityFailed,
        )?;
        // Its local data key opens only with the hardware it left behind.
        ensure(!copy.workspace.bound()?, Error::Forbidden)?;
        copy.keys = Some(copy.workspace.unlock(secret)?);
        Ok(copy)
    }

    /// The one permitted signing flow journals in a separate SQLite image;
    /// neither it nor revocation cleanup can change the import source.
    fn moved_signing_client(&self, id: &str, secret: &Key) -> Result<Client> {
        let mut copy = self.moved_client(id, secret)?;
        let device = copy.workspace.meta.device_id;
        let path = self.dir.join(OPERATIONS).join(id);
        crate::library::blobs::create_private_dir(&path)?;
        let database = path.join(crate::client::WORKSPACE);
        if !database.exists() {
            let pending = path.join("preparing.sqlite");
            copy.workspace.store.backup_to(&pending)?;
            let store = crate::store::Store::open(&pending)?;
            // These belonged to work started on the old device, not to the
            // replacement approved here. The original journal stays intact.
            store.delete_sealed("journal")?;
            store.delete_sealed(crate::root::ROOT_JOB)?;
            drop(store);
            std::fs::rename(&pending, &database).map_err(crate::storage)?;
        }
        copy.workspace =
            crate::workspace::Workspace::open(&database, &self.config.environment, now_ms())?;
        ensure(
            copy.workspace.meta.device_id == device,
            Error::IntegrityFailed,
        )?;
        copy.slot = Slot::Moved(path);
        copy.keys = Some(copy.workspace.unlock(secret)?);
        Ok(copy)
    }

    /// [`Self::moved_client`] with the device's unlock secret from the
    /// user home, and the content root that device opens: what signing as
    /// it needs.
    ///
    /// # Errors
    /// Returns `Error::DeviceNotApproved` once that device is revoked, and
    /// as [`Self::moved_client`].
    pub(crate) async fn moved_signer(&self, id: &str) -> Result<Client> {
        let (ic, home, account, device) = moved_secret_query(self, id)?;
        let secret = unlock_secret(&ic, home, &account, device).await?;
        let mut copy = self.moved_signing_client(id, &secret)?;
        copy.open_root().await?;
        Ok(copy)
    }
}

impl Client {
    /// Approve the pairing in succession `token` as copy `id`'s device,
    /// for an approval of this device: the enrollment, then a new content
    /// root the new device opens. `started` is set once the enrollment
    /// went through.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` when the token no longer enrolls
    /// `device`, and as [`Client::moved_signer`].
    pub(crate) async fn moved_approve(
        &self,
        id: &str,
        token: &str,
        device: &DeviceInput,
        started: &mut bool,
    ) -> Result<()> {
        let mut copy = self.moved_signer(id).await?;
        let pairing = copy.inspect_succession(token)?.0;
        ensure(pairing.device == *device, Error::IntegrityFailed)?;
        copy.approve_pairing(&pairing).await?;
        *started = true;
        copy.rotate_root().await.map(|_| ())
    }

    /// Settle an approval [`Self::moved_approve`] left unfinished.
    ///
    /// # Errors
    /// As [`Self::moved_approve`].
    pub(crate) async fn moved_resume(&self, id: &str) -> Result<()> {
        let mut copy = self.moved_signer(id).await?;
        copy.resume().await?;
        copy.rotate_root().await.map(|_| ())
    }
}

/// What asking for the unlock secret of copy `id`'s device needs.
fn moved_secret_query(
    c: &Client,
    id: &str,
) -> Result<(
    crate::ic::Ic,
    candid::Principal,
    dmsg_types::AccountId,
    Hash,
)> {
    ensure(plain_name(id), Error::NotFound)?;
    let copy = c
        .moved_copies()?
        .into_iter()
        .find(|copy| copy.id == id)
        .ok_or(Error::NotFound)?;
    let device = hex::decode(&copy.device_id)
        .ok()
        .and_then(|b| b.try_into().ok())
        .map(Hash::new)
        .ok_or(Error::IntegrityFailed)?;
    let (home, account) = c.moved_account()?;
    Ok((c.session()?.ic.clone(), home, account, device))
}

/// The user home's unlock secret of `device`, for the login.
async fn unlock_secret(
    ic: &crate::ic::Ic,
    home: candid::Principal,
    account: &dmsg_types::AccountId,
    device: Hash,
) -> Result<Zeroizing<Key>> {
    let secret: Result<Hash> = ic.query(home, "unlock_secret", (account, device)).await?;
    Ok(Zeroizing::new(*secret?))
}

/// Copy `id` opened as the device it was.
async fn opened<H: Hold>(h: &H, id: &str) -> Result<Client> {
    let (ic, home, account, device) = h.with(|c| moved_secret_query(c, id)).await?;
    let secret = unlock_secret(&ic, home, &account, device).await?;
    h.with(|c| c.moved_client(id, &secret)).await
}

/// Import the file library of copy `id` into this device as a transfer
/// from the copy's device would: each record's chunks first, checked and
/// cloned where the file system shares blocks, then folders, versions and
/// files through the transfer's merge, which only adds. No record of the
/// copy changes. With `succession`, the import settles that succession's
/// content step.
///
/// # Errors
/// Returns `Error::DeviceNotApproved` once the copy's device is revoked,
/// `Error::Forbidden` for a copy bound to its old hardware or a succession
/// of another device or past its content step, and `Error::Locked` while
/// this device is locked.
pub async fn import<H: Hold>(h: &H, id: &str, succession: Option<Hash>) -> Result<Report> {
    let bound = Unlocked::new(h).await?;
    let h = &bound;
    let mut copy = opened(h, id).await?;
    let peer = copy.workspace.meta.device_id;
    if let Some(task) = succession {
        // The task succeeds the copy's device, with its content step open.
        ensure(
            crate::succession::peer(h, task).await? == peer,
            Error::Forbidden,
        )?;
    }
    let entries = library::entries(&mut copy)?;
    let source = copy.blobs()?.clone();
    let blobs = h
        .with(|c| {
            c.keys()?;
            c.account()?;
            Ok(c.blobs()?.clone())
        })
        .await?;
    let mut report = Report {
        peer: Some(peer),
        entries: entries.len(),
        total_bytes: entries.iter().map(|e| e.blob_bytes).sum(),
        pending_sync: copy.workspace.store.count_in(crate::store::LOCAL)?,
        ..Report::default()
    };
    for phase in PHASES {
        let named: Vec<_> = entries
            .iter()
            .filter(|e| e.key.starts_with(phase))
            .collect();
        for batch in named.chunks(MAX_WANT) {
            let held: Vec<bool> = h
                .with(|c| batch.iter().map(|e| library::held(c, &e.key)).collect())
                .await?;
            let mut records = vec![];
            for (entry, held) in batch.iter().zip(held) {
                if held {
                    report.kept += 1;
                    continue;
                }
                for digest in entry.digests() {
                    let copied = blobs.copy_from(&source, &hex::encode(*digest))?;
                    if copied > 0 {
                        report.blobs += 1;
                        report.bytes += copied;
                    }
                }
                match copy.workspace.open_sealed(copy.keys()?, &entry.key)? {
                    Some(value) => records.push((entry.key.clone(), value)),
                    None => report.skipped += 1,
                }
            }
            let outcomes = h
                .with(|c| {
                    records
                        .iter()
                        .map(|(key, value)| library::commit(c, key, value))
                        .collect::<Result<Vec<_>>>()
                })
                .await?;
            for outcome in outcomes {
                match outcome {
                    Outcome::Added => report.records += 1,
                    Outcome::Kept => report.kept += 1,
                    Outcome::Skipped => report.skipped += 1,
                }
            }
        }
    }
    if let Some(task) = succession {
        crate::succession::transferred(h, task, &report).await?;
    }
    Ok(report)
}

/// Ask to have copy `id`'s device approve this new device as its
/// successor: this device asks to succeed it
/// ([`crate::succession::request`]), and the approval request, once
/// approved, signs the enrollment as the copy's device and rotates the
/// content root to this one. The copy's device must be an active
/// administrator, and the account must not guard device control with
/// hardware keys, which the copy cannot use on this hardware.
///
/// # Errors
/// Returns `Error::Forbidden` when the copy's device cannot approve, and as
/// [`import`] and [`crate::succession::request`].
pub async fn approve_successor<H: Hold>(h: &H, source: &str, id: &str) -> Result<Json> {
    let bound = crate::hold::Session::new(h).await?;
    let h = &bound;
    let copy = opened(h, id).await?;
    let predecessor = copy.workspace.meta.device_id;
    let (ic, (home, account)) = h
        .with(|c| Ok((c.session()?.ic.clone(), c.moved_account()?)))
        .await?;
    let info = crate::account::read_account(&ic, home, &account)
        .await?
        .info;
    let device = info
        .devices
        .get(&predecessor)
        .filter(|d| d.revoked_at.is_none())
        .ok_or(Error::DeviceNotApproved)?;
    ensure(
        device.input.role == ControllerRole::Administrator
            && device.input.capabilities.contains(&Capability::RootManage)
            && !info
                .hardware_policy
                .contains(&HardwarePurpose::DeviceControl),
        Error::Forbidden,
    )?;
    let made = crate::succession::request_moved(h, predecessor, copy.wallet_principal()?).await?;
    let token = made["token"].as_str().ok_or(Error::IntegrityFailed)?;
    let (pairing, _) = copy.inspect_succession(token)?;
    h.with(|c| {
        let expires_at = pairing
            .mutation
            .approval
            .expires_at
            .min(now_ms() + HIGH_RISK_TTL_MS);
        let request = c.approval_create(
            source,
            Action::DevicePair {
                token: token.into(),
                device: pairing.device,
                succession: true,
                moved: Some(id.into()),
            },
            expires_at,
        )?;
        let mut view = c.approval_view(&request)?;
        view["succession"] = made["id"].clone();
        Ok(view)
    })
    .await
}

#[cfg(test)]
#[path = "moved_tests.rs"]
mod tests;
