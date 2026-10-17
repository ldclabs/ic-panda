//! Commands of other accounts as the relay keeps them: each stored command
//! carries the certified security evidence the relay checked when it
//! accepted it. A reader verifies that evidence against the IC root and the
//! deployment's user homes at the relay's acceptance time, finds the signing
//! device in it, and checks the signature. This proves the key and its
//! authority when the relay stored the command, not an external timestamp.

use crate::account::{verify_security, Security};
use crate::cloud::{CommandBody, Evidence, SignedCommand};
use crate::codec::{canonical, decode, sha256, unb64};
use candid::Principal;
use dmsg_types::{
    ensure,
    user::{Capability, Device},
    AccountId, CertifiedBatch, CertifiedEntry, Error, Hash, Result,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Mutex;

/// How far a certificate may run ahead of this device's clock.
pub const CLOCK_SKEW_MS: u64 = 10_000;

/// What other accounts' evidence is checked against. The evidence it
/// verifies is remembered while it lives, so one certificate a device
/// presented with many commands is verified once.
#[derive(Debug)]
pub struct Trust {
    /// The deployment's user homes.
    pub homes: Vec<Principal>,
    /// The issuer namespace: an account's issuer is this followed by its ID.
    pub namespace: String,
    /// The IC root key, DER.
    pub root_key: Vec<u8>,
    verified: Mutex<BTreeMap<[u8; 32], Security>>,
}

/// A stored command checked against its signer at acceptance time.
#[derive(Clone, Debug)]
pub struct Verified<P> {
    /// The signing account.
    pub account: AccountId,
    /// The signing device.
    pub device: Hash,
    /// The account's certified security when the relay accepted it.
    pub security: Security,
    /// The command.
    pub body: CommandBody<P>,
}

impl Trust {
    /// Trust in the deployment of `homes` under `namespace` and `root_key`.
    pub fn new(homes: Vec<Principal>, namespace: String, root_key: Vec<u8>) -> Self {
        Self {
            homes,
            namespace,
            root_key,
            verified: Mutex::new(BTreeMap::new()),
        }
    }

    /// An account's issuer URI.
    pub fn issuer(&self, account: &AccountId) -> String {
        format!("{}{account}", self.namespace)
    }

    /// The certified security of `account` in relay evidence, at the
    /// relay's time `at`.
    ///
    /// # Errors
    /// Evidence from another home or account, outside 60 seconds of `at`,
    /// or with a device map the leaf does not commit to returns
    /// `Error::IntegrityFailed`.
    pub fn account(
        &self,
        evidence: &Evidence,
        account: &AccountId,
        at: u64,
        now: u64,
    ) -> Result<Security> {
        let home = Principal::from_text(&evidence.canister).map_err(|_| Error::IntegrityFailed)?;
        ensure(
            evidence.schema == 1
                && self.homes.contains(&home)
                && evidence.entries.len() <= 64
                && at > 0
                && at <= now + CLOCK_SKEW_MS,
            Error::IntegrityFailed,
        )?;
        let text = account.to_string();
        let mut entries = evidence.entries.iter().filter(|e| e.account_id == text);
        let (Some(entry), None) = (entries.next(), entries.next()) else {
            return Err(Error::IntegrityFailed);
        };
        // The certificate was checked within 60 seconds of another time;
        // whole milliseconds may only narrow what that check accepts.
        let key = *sha256(&canonical(&(
            &evidence.canister,
            &evidence.certificate,
            entry,
        )));
        let mut verified = self.verified.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(security) = verified.get(&key)
            && security.certified_at < at + 60_000
            && security.certified_at + 60_000 >= at
        {
            return Ok(security.clone());
        }
        let batch = CertifiedBatch {
            schema: 1,
            canister: home,
            certificate: ByteBuf::from(unb64(&evidence.certificate)?),
            entries: vec![CertifiedEntry {
                key: ByteBuf::from(account.as_slice().to_vec()),
                value: Some(ByteBuf::from(unb64(&entry.value)?)),
                witness: ByteBuf::from(unb64(&entry.witness)?),
            }],
        };
        let devices: BTreeMap<Hash, Device> = decode(&unb64(&entry.devices)?, 65_536)?;
        ensure(devices.len() <= 16, Error::IntegrityFailed)?;
        let security = verify_security(home, account, &batch, devices, &self.root_key, at)?;
        ensure(
            security.snapshot.issuer == self.issuer(account),
            Error::IntegrityFailed,
        )?;
        verified.insert(key, security.clone());
        Ok(security)
    }

    /// An unrevoked device of `account` that may sign content, in relay
    /// evidence at the relay's time `at`.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` for a missing, revoked or unqualified
    /// device, and the errors of [`Self::account`].
    pub fn device(
        &self,
        evidence: &Evidence,
        account: &AccountId,
        device: &Hash,
        at: u64,
        now: u64,
    ) -> Result<(Security, Device)> {
        let security = self.account(evidence, account, at, now)?;
        let found = content_signer(&security, device)?.clone();
        Ok((security, found))
    }

    /// The certified security of `account` under its security epoch
    /// `epoch`, from the evidence the relay kept when it first accepted a
    /// command under that epoch, at the relay's time `at`. Adding, revoking
    /// or changing a device starts a new epoch, so within one the keys and
    /// authority of its devices stay as one check finds them, for every
    /// command signed under it, whatever replaced those devices since.
    ///
    /// # Errors
    /// Evidence of another epoch returns `Error::IntegrityFailed`, and the
    /// errors of [`Self::account`].
    pub fn epoch(
        &self,
        evidence: &Evidence,
        account: &AccountId,
        epoch: u64,
        at: u64,
        now: u64,
    ) -> Result<Security> {
        let security = self.account(evidence, account, at, now)?;
        ensure(
            security.snapshot.security_epoch == epoch,
            Error::IntegrityFailed,
        )?;
        Ok(security)
    }

    /// Verify a stored command with the evidence the relay kept beside it at
    /// `stored_at`; `action`, when given, must be the command's.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` for a signer outside this deployment or
    /// without authority, `Error::PolicyStale` for a command signed under
    /// another security epoch or past its deadline when stored, and
    /// `Error::IntegrityFailed` for a bad signature.
    pub fn command<P: DeserializeOwned + Serialize>(
        &self,
        cose: &str,
        evidence: &Evidence,
        stored_at: u64,
        action: Option<&str>,
        now: u64,
    ) -> Result<Verified<P>> {
        let command = SignedCommand::parse(cose)?;
        let account = command
            .issuer
            .strip_prefix(&self.namespace)
            .and_then(|id| AccountId::from_str(id).ok())
            .ok_or(Error::AuthRequired)?;
        let security = self.account(evidence, &account, stored_at, now)?;
        let (device, body) = under_parsed(&security, &command, action)?;
        ensure(body.deadline > stored_at, Error::PolicyStale)?;
        Ok(Verified {
            account,
            device,
            security,
            body,
        })
    }
}

/// An unrevoked device of `security` that may sign content.
fn content_signer<'a>(security: &'a Security, device: &Hash) -> Result<&'a Device> {
    security
        .devices
        .get(device)
        .filter(|d| {
            d.revoked_at.is_none() && d.input.capabilities.contains(&Capability::ContentSign)
        })
        .ok_or(Error::AuthRequired)
}

/// Verify a command of the account of `security`, signed under its security
/// epoch by one of its devices that could sign content then; `action`, when
/// given, must be the command's. This is what the relay checked when it
/// accepted the command, against the certified security of that epoch.
///
/// # Errors
/// Returns `Error::AuthRequired` for another account or a device without
/// authority under that epoch, `Error::PolicyStale` for a command signed
/// under another epoch, and `Error::IntegrityFailed` for a bad signature.
pub fn under<P: DeserializeOwned + Serialize>(
    security: &Security,
    cose: &str,
    action: Option<&str>,
) -> Result<(Hash, CommandBody<P>)> {
    under_parsed(security, &SignedCommand::parse(cose)?, action)
}

/// [`under`] for a command already parsed.
fn under_parsed<P: DeserializeOwned + Serialize>(
    security: &Security,
    command: &SignedCommand,
    action: Option<&str>,
) -> Result<(Hash, CommandBody<P>)> {
    ensure(
        command.issuer == security.snapshot.issuer,
        Error::AuthRequired,
    )?;
    let device = content_signer(security, &command.kid)?;
    let body: CommandBody<P> = command.verify(&device.input.signing_pub, action)?;
    ensure(
        body.security_epoch == security.snapshot.security_epoch,
        Error::PolicyStale,
    )?;
    Ok((command.kid, body))
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
