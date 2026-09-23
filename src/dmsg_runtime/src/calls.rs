use candid::{utils::ArgumentEncoder, CandidType, Principal};
use dmsg_types::{Error, Result};
use ic_cdk::call::CallErrorExt;
use serde::de::DeserializeOwned;

/// Outcome of this call attempt, independent of any earlier attempt of the operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallFailure {
    /// The CDK guarantees this attempt made no changes at the destination.
    NotExecuted,
    /// The destination may have committed, including when reply decoding failed.
    Unknown,
}

impl CallFailure {
    fn classify(error: &impl CallErrorExt) -> Self {
        if error.is_clean_reject() {
            Self::NotExecuted
        } else {
            Self::Unknown
        }
    }

    /// A clean rejection cannot resolve an earlier ambiguous attempt.
    pub fn preserves_unknown(self, was_unknown: bool) -> bool {
        was_unknown || self == Self::Unknown
    }
}

impl From<CallFailure> for Error {
    fn from(failure: CallFailure) -> Self {
        match failure {
            CallFailure::NotExecuted => {
                Error::Unavailable("cross-canister call not executed".into())
            }
            CallFailure::Unknown => Error::ExecutionUnknown,
        }
    }
}

/// Bounded call retaining whether this attempt could have changed remote state.
pub async fn call_classified<In, Out>(
    id: Principal,
    method: &str,
    args: In,
) -> std::result::Result<Out, CallFailure>
where
    In: ArgumentEncoder + Send,
    Out: CandidType + DeserializeOwned,
{
    let response = ic_cdk::call::Call::bounded_wait(id, method)
        .with_args(&args)
        .await
        .map_err(|error| CallFailure::classify(&error))?;
    decode_response(&response)
}

fn decode_response<Out: CandidType + DeserializeOwned>(
    bytes: &[u8],
) -> std::result::Result<Out, CallFailure> {
    candid::decode_one(bytes).map_err(|_| CallFailure::Unknown)
}

/// Bounded call for consumers that only need an application error.
/// State-changing retries should use [`call_classified`] and preserve prior uncertainty.
pub async fn call<In, Out>(id: Principal, method: &str, args: In) -> Result<Out>
where
    In: ArgumentEncoder + Send,
    Out: CandidType + DeserializeOwned,
{
    call_classified(id, method, args).await.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ic_cdk::call::{
        CallPerformFailed, CallRejected, InsufficientLiquidCycleBalance, RejectCode,
    };

    #[test]
    fn only_clean_rejections_prove_this_attempt_did_not_execute() {
        assert_eq!(
            CallFailure::classify(&CallPerformFailed),
            CallFailure::NotExecuted
        );
        assert_eq!(
            CallFailure::classify(&InsufficientLiquidCycleBalance {
                available: 0,
                required: 1
            }),
            CallFailure::NotExecuted
        );
        for (code, expected) in [
            (RejectCode::SysTransient, CallFailure::NotExecuted),
            (RejectCode::DestinationInvalid, CallFailure::NotExecuted),
            (RejectCode::SysUnknown, CallFailure::Unknown),
            (RejectCode::CanisterReject, CallFailure::Unknown),
            (RejectCode::CanisterError, CallFailure::Unknown),
        ] {
            let error = CallRejected::with_rejection(code as u32, "test".into());
            assert_eq!(CallFailure::classify(&error), expected);
        }
        assert!(!CallFailure::NotExecuted.preserves_unknown(false));
        assert!(CallFailure::NotExecuted.preserves_unknown(true));
        assert!(CallFailure::Unknown.preserves_unknown(false));
        assert!(CallFailure::Unknown.preserves_unknown(true));
    }

    #[test]
    fn malformed_or_wrong_type_replies_do_not_prove_nonexecution() {
        assert_eq!(
            decode_response::<u64>(b"not candid"),
            Err(CallFailure::Unknown)
        );
        assert_eq!(
            decode_response::<u64>(&candid::encode_one("wrong type").unwrap()),
            Err(CallFailure::Unknown)
        );
        assert_eq!(
            decode_response::<u64>(&candid::encode_one(42u64).unwrap()),
            Ok(42)
        );
    }
}
