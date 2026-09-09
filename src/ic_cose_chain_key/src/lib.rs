//! Shared chain-key mechanics. This crate exports no canister entry points.
//!
//! Callers own authorization, derivation domains, durable deduplication and
//! budgets. Estimate a fixed call, persist its execution intent, then execute
//! it once. Unknown outcomes must not be retried as a new operation.
use candid::CandidType;
// Chain-key helpers never obtain entropy synchronously on a canister.
use ic_cdk::call::{CallErrorExt, CallFailed, RejectCode};
use ic_cdk_management_canister as mgmt;
use ic_dummy_getrandom_for_wasm as _;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, CandidType, Deserialize, Serialize, PartialEq, Eq)]
pub struct PublicKey {
    #[serde(with = "serde_bytes")]
    pub public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub chain_code: Vec<u8>,
}

/// Management request fee plus the outgoing inter-canister call fee. This is
/// not total canister operating cost (instructions/storage/query are separate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cost {
    pub request_cycles: u128,
    pub call_cycles: u128,
}
impl Cost {
    pub fn total(self) -> Result<u128, String> {
        self.request_cycles
            .checked_add(self.call_cycles)
            .ok_or_else(|| "chain-key call cost overflowed".into())
    }
}

/// Typed management arguments are kept intact between estimation and dispatch.
#[derive(Clone, Debug)]
pub enum Operation {
    Ecdsa(mgmt::SignWithEcdsaArgs),
    Schnorr(mgmt::SignWithSchnorrArgs),
    VetKd(mgmt::VetKDDeriveKeyArgs),
}
impl Operation {
    pub fn ecdsa(key_name: String, path: Vec<Vec<u8>>, message_hash: [u8; 32]) -> Self {
        Self::Ecdsa(mgmt::SignWithEcdsaArgs {
            message_hash: message_hash.to_vec(),
            derivation_path: path,
            key_id: mgmt::EcdsaKeyId {
                curve: mgmt::EcdsaCurve::Secp256k1,
                name: key_name,
            },
        })
    }
    pub fn schnorr(
        key_name: String,
        algorithm: mgmt::SchnorrAlgorithm,
        path: Vec<Vec<u8>>,
        message: Vec<u8>,
    ) -> Self {
        Self::Schnorr(mgmt::SignWithSchnorrArgs {
            message,
            derivation_path: path,
            key_id: mgmt::SchnorrKeyId {
                algorithm,
                name: key_name,
            },
            aux: None,
        })
    }
    /// `context` and `input` are exact caller-defined bytes, never re-encoded.
    pub fn vetkd(
        key_name: String,
        context: Vec<u8>,
        input: Vec<u8>,
        transport_public_key: Vec<u8>,
    ) -> Self {
        Self::VetKd(mgmt::VetKDDeriveKeyArgs {
            context,
            input,
            transport_public_key,
            key_id: mgmt::VetKDKeyId {
                curve: mgmt::VetKDCurve::Bls12_381_G2,
                name: key_name,
            },
        })
    }
    pub fn cost(&self) -> Result<Cost, String> {
        let (request_cycles, method, payload) = match self {
            Self::Ecdsa(a) => (
                mgmt::cost_sign_with_ecdsa(a).map_err(format_error)?,
                "sign_with_ecdsa",
                candid::encode_one(a).map_err(format_error)?,
            ),
            Self::Schnorr(a) => (
                mgmt::cost_sign_with_schnorr(a).map_err(format_error)?,
                "sign_with_schnorr",
                candid::encode_one(a).map_err(format_error)?,
            ),
            Self::VetKd(a) => (
                mgmt::cost_vetkd_derive_key(a).map_err(format_error)?,
                "vetkd_derive_key",
                candid::encode_one(a).map_err(format_error)?,
            ),
        };
        let cost = Cost {
            request_cycles,
            call_cycles: ic_cdk::api::cost_call(method.len() as u64, payload.len() as u64),
        };
        cost.total()?;
        Ok(cost)
    }
    pub async fn execute(self) -> Result<Vec<u8>, mgmt::SignCallError> {
        match self {
            Self::Ecdsa(a) => mgmt::sign_with_ecdsa(&a).await.map(|r| r.signature),
            Self::Schnorr(a) => mgmt::sign_with_schnorr(&a).await.map(|r| r.signature),
            Self::VetKd(a) => mgmt::vetkd_derive_key(&a).await.map(|r| r.encrypted_key),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    NotSent,
    Rejected,
    Unknown,
}
pub fn classify_failure(error: &mgmt::SignCallError) -> FailureKind {
    match error {
        mgmt::SignCallError::SignCostError(_) => FailureKind::NotSent,
        mgmt::SignCallError::CallFailed(
            CallFailed::InsufficientLiquidCycleBalance(_) | CallFailed::CallPerformFailed(_),
        ) => FailureKind::NotSent,
        mgmt::SignCallError::CallFailed(failure)
            if failure.is_clean_reject()
                || matches!(failure, CallFailed::CallRejected(r)
                if r.raw_reject_code() == RejectCode::CanisterReject as u32) =>
        {
            FailureKind::Rejected
        }
        _ => FailureKind::Unknown,
    }
}
/// Call immediately after `execute().await`, supplying that callback's refund.
pub fn charged_cycles(cost: Cost, error: Option<&mgmt::SignCallError>, refunded: u128) -> u128 {
    if error.is_some_and(|e| classify_failure(e) == FailureKind::NotSent) {
        return 0;
    }
    cost.request_cycles
        .saturating_sub(refunded)
        .saturating_add(cost.call_cycles)
}

pub async fn ecdsa_public_key(key_name: String, path: Vec<Vec<u8>>) -> Result<PublicKey, String> {
    let r = mgmt::ecdsa_public_key(&mgmt::EcdsaPublicKeyArgs {
        canister_id: None,
        derivation_path: path,
        key_id: mgmt::EcdsaKeyId {
            curve: mgmt::EcdsaCurve::Secp256k1,
            name: key_name,
        },
    })
    .await
    .map_err(format_error)?;
    Ok(PublicKey {
        public_key: r.public_key,
        chain_code: r.chain_code,
    })
}
pub async fn schnorr_public_key(
    key_name: String,
    algorithm: mgmt::SchnorrAlgorithm,
    path: Vec<Vec<u8>>,
) -> Result<PublicKey, String> {
    let r = mgmt::schnorr_public_key(&mgmt::SchnorrPublicKeyArgs {
        canister_id: None,
        derivation_path: path,
        key_id: mgmt::SchnorrKeyId {
            algorithm,
            name: key_name,
        },
    })
    .await
    .map_err(format_error)?;
    Ok(PublicKey {
        public_key: r.public_key,
        chain_code: r.chain_code,
    })
}
pub fn vetkd_public_key_cost(key_name: &str, context: &[u8]) -> Result<u128, String> {
    let a = mgmt::VetKDPublicKeyArgs {
        canister_id: None,
        context: context.to_vec(),
        key_id: mgmt::VetKDKeyId {
            curve: mgmt::VetKDCurve::Bls12_381_G2,
            name: key_name.into(),
        },
    };
    let payload = candid::encode_one(a).map_err(format_error)?;
    Ok(ic_cdk::api::cost_call(
        "vetkd_public_key".len() as u64,
        payload.len() as u64,
    ))
}
pub async fn vetkd_public_key(key_name: String, context: Vec<u8>) -> Result<Vec<u8>, String> {
    mgmt::vetkd_public_key(&mgmt::VetKDPublicKeyArgs {
        canister_id: None,
        context,
        key_id: mgmt::VetKDKeyId {
            curve: mgmt::VetKDCurve::Bls12_381_G2,
            name: key_name,
        },
    })
    .await
    .map(|r| r.public_key)
    .map_err(format_error)
}
fn format_error(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}

pub fn derive_ecdsa_public_key(
    ecdsa_public_key: &PublicKey,
    derivation_path: Vec<Vec<u8>>,
) -> Result<PublicKey, String> {
    let path = ic_secp256k1::DerivationPath::new(
        derivation_path
            .into_iter()
            .map(ic_secp256k1::DerivationIndex)
            .collect(),
    );

    let chain_code: [u8; 32] = ecdsa_public_key
        .chain_code
        .as_slice()
        .try_into()
        .map_err(format_error)?;
    let pk = ic_secp256k1::PublicKey::deserialize_sec1(&ecdsa_public_key.public_key)
        .map_err(format_error)?;
    let (derived_public_key, derived_chain_code) =
        pk.derive_subkey_with_chain_code(&path, &chain_code);

    Ok(PublicKey {
        public_key: derived_public_key.serialize_sec1(true),
        chain_code: Vec::from(derived_chain_code),
    })
}

pub fn derive_schnorr_public_key(
    alg: mgmt::SchnorrAlgorithm,
    public_key: &PublicKey,
    derivation_path: Vec<Vec<u8>>,
) -> Result<PublicKey, String> {
    match alg {
        mgmt::SchnorrAlgorithm::Bip340secp256k1 => {
            let path = ic_secp256k1::DerivationPath::new(
                derivation_path
                    .into_iter()
                    .map(ic_secp256k1::DerivationIndex)
                    .collect(),
            );

            let chain_code: [u8; 32] = public_key
                .chain_code
                .as_slice()
                .try_into()
                .map_err(format_error)?;
            let pk = ic_secp256k1::PublicKey::deserialize_sec1(&public_key.public_key)
                .map_err(format_error)?;
            let (derived_public_key, derived_chain_code) =
                pk.derive_subkey_with_chain_code(&path, &chain_code);

            Ok(PublicKey {
                public_key: derived_public_key.serialize_sec1(true),
                chain_code: Vec::from(derived_chain_code),
            })
        }

        mgmt::SchnorrAlgorithm::Ed25519 => {
            let path = ic_ed25519::DerivationPath::new(
                derivation_path
                    .into_iter()
                    .map(ic_ed25519::DerivationIndex)
                    .collect(),
            );

            let chain_code: [u8; 32] = public_key
                .chain_code
                .as_slice()
                .try_into()
                .map_err(format_error)?;
            let pk = ic_ed25519::PublicKey::deserialize_raw(&public_key.public_key)
                .map_err(format_error)?;
            let (derived_public_key, derived_chain_code) =
                pk.derive_subkey_with_chain_code(&path, &chain_code);

            Ok(PublicKey {
                public_key: Vec::from(derived_public_key.serialize_raw()),
                chain_code: Vec::from(derived_chain_code),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_rejections_never_become_retryable_failures() {
        for code in [
            RejectCode::SysFatal,
            RejectCode::SysTransient,
            RejectCode::DestinationInvalid,
            RejectCode::CanisterReject,
        ] {
            let e = mgmt::SignCallError::CallFailed(CallFailed::CallRejected(
                ic_cdk::call::CallRejected::with_rejection(code as u32, "rejected".into()),
            ));
            assert_eq!(classify_failure(&e), FailureKind::Rejected);
        }
        for code in [RejectCode::CanisterError, RejectCode::SysUnknown] {
            let e = mgmt::SignCallError::CallFailed(CallFailed::CallRejected(
                ic_cdk::call::CallRejected::with_rejection(code as u32, "unknown".into()),
            ));
            assert_eq!(classify_failure(&e), FailureKind::Unknown);
        }
    }
    #[test]
    fn refunds_do_not_refund_call_fees_and_unsent_calls_cost_nothing() {
        let cost = Cost {
            request_cycles: 100,
            call_cycles: 7,
        };
        assert_eq!(charged_cycles(cost, None, 30), 77);
        assert_eq!(charged_cycles(cost, None, 100), 7);
        let e = mgmt::SignCallError::CallFailed(CallFailed::InsufficientLiquidCycleBalance(
            ic_cdk::call::InsufficientLiquidCycleBalance {
                available: 0,
                required: 100,
            },
        ));
        assert_eq!(classify_failure(&e), FailureKind::NotSent);
        assert_eq!(charged_cycles(cost, Some(&e), 0), 0);
        assert!(Cost {
            request_cycles: u128::MAX,
            call_cycles: 1
        }
        .total()
        .is_err());
    }
}
