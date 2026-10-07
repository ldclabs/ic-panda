use crate::*;
use candid::Principal;
use dmsg_types::{agent::DelegationAuthority, cose::*, user::*, *};
use serde_bytes::ByteArray;

/// Local validation of a device enrollment proposal; no device registration or authorization.
pub trait DeviceInputExt {
    /// Check nonzero device/HPKE bytes, 1..5 unique capabilities and a nonweak Ed25519 key.
    ///
    /// This does not verify proof of possession, fully validate an HPKE point or
    /// apply account-specific role rules.
    ///
    /// # Errors
    /// Invalid identifiers/capabilities return `Error::InvalidInput`; invalid signing
    /// keys return `Error::IntegrityFailed`.
    fn validate(&self) -> Result<()>;
}

impl DeviceInputExt for DeviceInput {
    fn validate(&self) -> Result<()> {
        nonzero(self.device_id.as_slice())?;
        nonzero(self.hpke_pub.as_slice())?;
        ensure_valid(
            !self.capabilities.is_empty() && self.capabilities.len() <= 5,
            "capabilities",
        )?;
        // At most five capabilities: a slice scan avoids a tree allocation.
        ensure_valid(
            self.capabilities
                .iter()
                .enumerate()
                .all(|(i, capability)| !self.capabilities[..i].contains(capability)),
            "duplicate capability",
        )?;
        validate_ed25519_key(self.signing_pub.as_slice())
    }
}

/// Exact signing input of a statement attested by one device key.
pub struct PreparedAttestation {
    /// Canonical COSE Sig_structure bytes the device signs.
    pub to_be_signed: Vec<u8>,
    /// Public-only COSE_Key of the device signing key, kid = thumbprint.
    pub cose_key: Vec<u8>,
    /// RFC 9679 thumbprint of the device key; also the artifact kid.
    pub thumbprint: Hash,
    /// Policy domain of the statement.
    pub purpose: KeyPurpose,
}

/// Prepare the exact bytes a device signs for a statement.
///
/// The kid is the device key's RFC 9679 thumbprint, so an artifact names the
/// key that signed it. Validates the statement; signs nothing.
///
/// # Errors
/// Propagates [`prepare_cose`] and key encoding errors.
pub fn prepare_attestation(statement: &Statement, signing_pub: &Hash) -> Result<PreparedAttestation> {
    let thumbprint = key_thumbprint(&public_cose_key(&[], signing_pub.as_slice())?)?;
    let cose_key = public_cose_key(thumbprint.as_slice(), signing_pub.as_slice())?;
    let (_, to_be_signed) = prepare_cose(statement, thumbprint.as_slice())?;
    Ok(PreparedAttestation {
        to_be_signed,
        cose_key,
        thumbprint,
        purpose: statement_purpose(statement),
    })
}

/// Build the statement an application-action attestation signs.
///
/// Checks that the approving account is the action's signing account and that
/// the approval does not outlive the action.
///
/// # Errors
/// A foreign signing account returns `Error::Forbidden`; a late approval `Error::Expired`.
pub fn app_action_statement(request: &AppActionAttestRequest) -> Result<Statement> {
    ensure(
        request.account_id == request.action.signing_account,
        Error::Forbidden,
    )?;
    ensure(
        request.approval.expires_at <= request.action.expires_at_ms,
        Error::Expired,
    )?;
    Ok(Statement {
        issuer: request.issuer.clone(),
        subject: None,
        issued_at: None,
        content: StatementContent::AppAction(Box::new(request.action.clone())),
    })
}

/// vetKD context of every content-root key:
/// `canonical(("dmsg/content-root/v2", environment, derivation_version))`.
///
/// Each account's IBE identity is `canonical((account_id, generation))`, and
/// the context's vetKD public key is the COSE master pin.
pub fn content_root_context(environment: &Environment, derivation_version: u16) -> Vec<u8> {
    canonical(&("dmsg/content-root/v2", environment, derivation_version))
}

/// Validate fixed COSE deployment configuration without accessing ICP master keys.
pub trait CoseInitExt {
    /// Check executing canister against id, derivation version 2, namespace, user
    /// homes, governance, key name and positive budgets. Production requires
    /// key_1 and a nonzero fingerprint; this does not fetch/check actual keys.
    ///
    /// # Errors
    /// Invalid configuration returns `Error::InvalidInput`; excluded user-home or
    /// governance Principals return `Error::AuthRequired`; homes are checked by
    /// [`crate::agent::validate_user_homes`].
    fn validate(&self, id: Principal) -> Result<()>;
}

impl CoseInitExt for CoseInit {
    fn validate(&self, id: Principal) -> Result<()> {
        ensure_valid(
            self.executing_canister == id && self.derivation_version == 2,
            "immutable key home/version",
        )?;
        validate_namespace(&self.issuer_namespace)?;
        crate::agent::validate_user_homes(
            &self.environment,
            &self.issuer_namespace,
            &self.user_homes,
        )?;
        authenticated(self.governance)?;
        if self.environment == Environment::Production {
            ensure_valid(self.master.key_name == "key_1", "production requires key_1")?;
            nonzero(self.master.expected_fingerprint.as_slice())?;
        } else {
            ensure_valid(
                matches!(
                    self.master.key_name.as_str(),
                    "key_1" | "test_key_1" | "dfx_test_key"
                ),
                "key name",
            )?;
        }
        ensure_valid(
            self.daily_executions > 0 && self.daily_cycles > 0,
            "hard budgets",
        )
    }
}

/// Device-approval domain of every [`AttestRequest`] and [`AppActionAttestRequest`].
pub const ATTEST_APPROVAL_DOMAIN: &str = "dmsg/attest/v1";

/// Command an attestation approval binds under [`ATTEST_APPROVAL_DOMAIN`]:
/// the statement, the checked origin and the device's document signature.
///
/// Devices sign [`approval_message`] over this triple, and the user canister
/// verifies the same triple; the approval's account and replay context are
/// bound separately.
pub fn attest_approval_command<'a>(
    statement: &'a Statement,
    origin: &'a str,
    signature: &'a Ed25519Signature,
) -> (&'a Statement, &'a str, &'a Ed25519Signature) {
    (statement, origin, signature)
}

/// Device-approval domain of every [`DeriveRootRequest`].
pub const DERIVE_APPROVAL_DOMAIN: &str = "dmsg/derive-root/v1";

/// Command a derivation approval binds under [`DERIVE_APPROVAL_DOMAIN`]: the
/// root generation, the transport public key and the cycles ceiling.
pub fn derive_approval_command(request: &DeriveRootRequest) -> (u64, &ByteArray<48>, u128) {
    (
        request.generation,
        &request.transport_public_key,
        request.max_cycles,
    )
}

/// Build the proof-of-possession digest a new device signs for a recovery
/// request under `dmsg/recovery-device/v1`.
///
/// Binds the user home, account and the full request. This constructs bytes
/// only; it checks nothing.
pub fn recovery_device_message(home: Principal, account_id: &AccountId, request: &RecoveryRequest) -> Hash {
    digest("dmsg/recovery-device/v1", &(home, account_id, request))
}

/// Build the proof-of-possession digest a controller key signs for its
/// registration under `dmsg/controller-pop/v1`.
///
/// Binds the user home, account, generation, delegation ceiling, superseded
/// generations and the approval's request ID. Sign the digest with the
/// controller's private key, not a device key. This checks nothing.
pub fn controller_pop_message(
    home: Principal,
    account_id: &AccountId,
    generation: u32,
    delegation: &DelegationAuthority,
    supersedes: &[u32],
    request_id: OpId,
) -> Hash {
    digest(
        "dmsg/controller-pop/v1",
        &(
            home,
            account_id,
            generation,
            delegation,
            supersedes,
            request_id,
        ),
    )
}

/// Digest of the recipients a root bundle of `generation` is wrapped to:
/// the active device IDs in ascending order plus the generation, which names
/// the vetKD recovery identity. `devices` may be given in any order.
pub fn root_recipients_digest(devices: &[Hash], generation: u64) -> Hash {
    let mut ids: Vec<Hash> = devices.to_vec();
    ids.sort_unstable();
    digest("dmsg/root-recipients/1", &(ids, generation))
}

/// Commitment to a root bundle: its recipients digest and the SHA-256 of its
/// body. The user home checks both parts at `CommitRoot`.
pub fn root_bundle_digest(recipients_digest: Hash, body_digest: Hash) -> Hash {
    digest(
        "dmsg/root-bundle-digest/2",
        &(recipients_digest, body_digest),
    )
}

#[cfg(test)]
mod tests;
