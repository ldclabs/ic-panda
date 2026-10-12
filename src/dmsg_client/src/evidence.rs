//! Commands of other accounts as the relay keeps them: each stored command
//! carries the certified security evidence the relay checked when it
//! accepted it. A reader verifies that evidence against the IC root and the
//! deployment's user homes at the relay's acceptance time, finds the signing
//! device in it, and checks the signature. This proves the key and its
//! authority when the relay stored the command, not an external timestamp.

use crate::account::{verify_security, Security};
use crate::cloud::{command_signer, verify_command_body, CommandBody, Evidence};
use crate::codec::{decode, unb64};
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

/// How far a certificate may run ahead of this device's clock.
pub const CLOCK_SKEW_MS: u64 = 10_000;

/// What other accounts' evidence is checked against.
#[derive(Clone, Debug)]
pub struct Trust {
    /// The deployment's user homes.
    pub homes: Vec<Principal>,
    /// The issuer namespace: an account's issuer is this followed by its ID.
    pub namespace: String,
    /// The IC root key, DER.
    pub root_key: Vec<u8>,
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
        let found = security
            .devices
            .get(device)
            .filter(|d| {
                d.revoked_at.is_none() && d.input.capabilities.contains(&Capability::ContentSign)
            })
            .cloned()
            .ok_or(Error::AuthRequired)?;
        Ok((security, found))
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
        let (issuer, kid) = command_signer(cose)?;
        let account = issuer
            .strip_prefix(&self.namespace)
            .and_then(|id| AccountId::from_str(id).ok())
            .ok_or(Error::AuthRequired)?;
        let (security, device) = self.device(evidence, &account, &kid, stored_at, now)?;
        let body: CommandBody<P> =
            verify_command_body(cose, &issuer, &kid, &device.input.signing_pub, action)?;
        ensure(
            body.security_epoch == security.snapshot.security_epoch && body.deadline > stored_at,
            Error::PolicyStale,
        )?;
        Ok(Verified {
            account,
            device: kid,
            security,
            body,
        })
    }
}
