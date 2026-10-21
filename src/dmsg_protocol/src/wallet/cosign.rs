//! Cosigner signatures over cosign messages: raw keys (Secure Enclave, TPM,
//! software), CTAP2 assertions and WebAuthn assertions.
use base64::Engine;
use dmsg_types::{wallet::*, *};
use sha2::{Digest, Sha256};

/// An address policy required before an intent may move assets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CosignRequirement {
    /// Address protected by this policy.
    pub address: u32,
    /// Its independent threshold; shared keys may satisfy multiple addresses.
    pub policy: CosignPolicy,
}

/// General canister calls can spend from any ICP subaccount or grant an
/// allowance, so they require every protected ICP address's policy.
pub fn cosign_requirements(intent: &Intent, addresses: &[Address]) -> Vec<CosignRequirement> {
    let source = match intent {
        Intent::Transfer { from, .. }
        | Intent::Batch { from, .. }
        | Intent::Refuel { from, .. } => Some(*from),
        Intent::CancelEvmNonce { address, .. } | Intent::InitSolNonce { address, .. } => {
            Some(*address)
        }
        _ => None,
    };
    addresses
        .iter()
        .filter(|a| {
            source == Some(a.id)
                || (matches!(intent, Intent::CanisterCall { .. }) && a.chain == Chain::Icp)
        })
        .filter_map(|a| {
            a.cosign.clone().map(|policy| CosignRequirement {
                address: a.id,
                policy,
            })
        })
        .collect()
}

/// Verify provided signatures in digest order for each key, advancing its
/// authenticator counter exactly as execution does. Partial packets are valid;
/// only keys covering every digest are returned, with their final counter.
///
/// # Errors
/// Rejects unknown keys, duplicate/out-of-range indices and invalid signatures.
pub fn verify_cosignatures(
    cosigners: &[Cosigner],
    messages: &[Hash],
    signatures: &[CosignerSig],
) -> Result<Vec<(u32, u32)>> {
    ensure(
        !messages.is_empty() && signatures.len() <= MAX_COSIGNERS * MAX_BTC_INPUTS,
        Error::QuotaExceeded,
    )?;
    let mut groups = std::collections::BTreeMap::<u32, Vec<&CosignerSig>>::new();
    for sig in signatures {
        groups.entry(sig.cosigner).or_default().push(sig);
    }
    let mut complete = vec![];
    for (id, mut signatures) in groups {
        let mut key = cosigners
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or(Error::Forbidden)?;
        signatures.sort_by_key(|s| s.digest_index);
        let mut previous = None;
        for sig in &signatures {
            ensure(previous != Some(sig.digest_index), Error::IntegrityFailed)?;
            let message = messages
                .get(sig.digest_index as usize)
                .ok_or(Error::IntegrityFailed)?;
            key.counter = verify_cosign(&key, message, sig)?;
            previous = Some(sig.digest_index);
        }
        if signatures.len() == messages.len() {
            complete.push((id, key.counter));
        }
    }
    Ok(complete)
}

/// Check a cosigner public key and envelope as proposed.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a malformed key and
/// `Error::InvalidInput` for an envelope the curve cannot use.
pub fn validate_cosigner(input: &CosignerInput) -> Result<()> {
    let key = input.public_key.as_slice();
    match input.kind {
        CosignerKind::P256 => {
            p256::ecdsa::VerifyingKey::from_sec1_bytes(key).map_err(|_| Error::IntegrityFailed)?;
            ensure(key.len() == 33, Error::IntegrityFailed)?;
        }
        CosignerKind::Secp256k1 => {
            k256::ecdsa::VerifyingKey::from_sec1_bytes(key).map_err(|_| Error::IntegrityFailed)?;
            ensure(key.len() == 33, Error::IntegrityFailed)?;
        }
        CosignerKind::Bip340 => {
            k256::schnorr::VerifyingKey::from_slice(key).map_err(|_| Error::IntegrityFailed)?;
        }
        CosignerKind::Ed25519 => crate::validate_ed25519_key(key)?,
    }
    match &input.envelope {
        CosignerEnvelope::Raw => Ok(()),
        CosignerEnvelope::Fido2 { .. } => ensure_valid(
            matches!(input.kind, CosignerKind::P256 | CosignerKind::Ed25519),
            "fido2 cosigner curve",
        ),
        CosignerEnvelope::WebAuthn { origins, .. } => ensure_valid(
            matches!(input.kind, CosignerKind::P256 | CosignerKind::Ed25519)
                && !origins.is_empty()
                && origins.len() <= 8
                && origins
                    .iter()
                    .all(|o| o.starts_with("https://") && o.len() <= 256),
            "webauthn cosigner",
        ),
    }?;
    ensure_valid(input.label.len() <= 64, "cosigner label")
}

fn verify_key(kind: CosignerKind, key: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
    match kind {
        CosignerKind::P256 => {
            use p256::ecdsa::signature::Verifier;
            let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(key)
                .map_err(|_| Error::IntegrityFailed)?;
            let sig = p256::ecdsa::Signature::from_slice(signature)
                .map_err(|_| Error::IntegrityFailed)?;
            key.verify(message, &sig)
                .map_err(|_| Error::IntegrityFailed)
        }
        CosignerKind::Secp256k1 => {
            use k256::ecdsa::signature::Verifier;
            let key = k256::ecdsa::VerifyingKey::from_sec1_bytes(key)
                .map_err(|_| Error::IntegrityFailed)?;
            let sig = k256::ecdsa::Signature::from_slice(signature)
                .map_err(|_| Error::IntegrityFailed)?;
            key.verify(message, &sig)
                .map_err(|_| Error::IntegrityFailed)
        }
        CosignerKind::Bip340 => {
            let key =
                k256::schnorr::VerifyingKey::from_slice(key).map_err(|_| Error::IntegrityFailed)?;
            let sig = k256::schnorr::Signature::try_from(signature)
                .map_err(|_| Error::IntegrityFailed)?;
            key.verify_raw(message, &sig)
                .map_err(|_| Error::IntegrityFailed)
        }
        CosignerKind::Ed25519 => {
            let key: Hash = Hash::new(key.try_into().map_err(|_| Error::IntegrityFailed)?);
            crate::verify(&key, message, signature)
        }
    }
}

/// Flags and counter of CTAP2 authenticator data, after checking its
/// relying party.
fn authenticator(data: &[u8], rp_id_hash: &Hash, user_verification: bool) -> Result<u32> {
    ensure(data.len() >= 37, Error::IntegrityFailed)?;
    ensure(data[..32] == rp_id_hash[..], Error::Forbidden)?;
    let flags = data[32];
    // UP (user present) is always required, UV when the policy asks.
    ensure(
        flags & 0x01 != 0 && (!user_verification || flags & 0x04 != 0),
        Error::Forbidden,
    )?;
    Ok(u32::from_be_bytes(
        data[33..37].try_into().expect("4 bytes"),
    ))
}

fn next_counter(stored: u32, seen: u32) -> Result<u32> {
    // Authenticators without counters report zero every time.
    ensure(
        seen == 0 && stored == 0 || seen > stored,
        Error::IntegrityFailed,
    )?;
    Ok(seen.max(stored))
}

/// Verify `sig` by `cosigner` over the 32-byte cosign `message`; returns the
/// authenticator counter to store.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a bad signature, envelope or
/// counter and `Error::Forbidden` for another relying party, origin or a
/// missing user presence or verification.
pub fn verify_cosign(cosigner: &Cosigner, message: &Hash, sig: &CosignerSig) -> Result<u32> {
    let input = &cosigner.input;
    let key = input.public_key.as_slice();
    match &input.envelope {
        CosignerEnvelope::Raw => {
            verify_key(input.kind, key, &message[..], &sig.signature)?;
            Ok(cosigner.counter)
        }
        CosignerEnvelope::Fido2 {
            rp_id_hash,
            user_verification,
        } => {
            let data = sig
                .authenticator_data
                .as_ref()
                .ok_or(Error::IntegrityFailed)?;
            let seen = authenticator(data, rp_id_hash, *user_verification)?;
            // CTAP2 signs authenticatorData || clientDataHash.
            let signed = [data.as_slice(), &message[..]].concat();
            verify_key(input.kind, key, &signed, &sig.signature)?;
            next_counter(cosigner.counter, seen)
        }
        CosignerEnvelope::WebAuthn {
            rp_id_hash,
            origins,
            user_verification,
        } => {
            let data = sig
                .authenticator_data
                .as_ref()
                .ok_or(Error::IntegrityFailed)?;
            let client = sig
                .client_data_json
                .as_ref()
                .ok_or(Error::IntegrityFailed)?;
            let seen = authenticator(data, rp_id_hash, *user_verification)?;
            let parsed: serde_json::Value =
                serde_json::from_slice(client).map_err(|_| Error::IntegrityFailed)?;
            let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&message[..]);
            ensure(
                parsed["type"] == "webauthn.get" && parsed["challenge"] == challenge.as_str(),
                Error::IntegrityFailed,
            )?;
            ensure(
                parsed["origin"]
                    .as_str()
                    .is_some_and(|o| origins.iter().any(|x| x == o)),
                Error::Forbidden,
            )?;
            let signed = [data.as_slice(), &Sha256::digest(client)[..]].concat();
            verify_key(input.kind, key, &signed, &sig.signature)?;
            next_counter(cosigner.counter, seen)
        }
    }
}
