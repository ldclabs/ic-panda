use dmsg_types::*;

use ic_certification::HashTree;
use serde_bytes::ByteBuf;

/// Maximum Candid-encoded successful `Result<CertifiedBatch>` response.
pub const MAX_CERTIFIED_RESPONSE_BYTES: usize = 262_144;

/// The data certificate of this non-replicated query.
pub fn query_certificate() -> Result<Vec<u8>> {
    // The replica query cache keys on caller/method/arguments, not transport
    // nonce. Depending on batch time prevents a cached data_certificate from
    // outliving our 60-second client freshness window without any state write.
    // See dfinity/ic query_handler/query_cache.rs, EntryValue::new/is_valid.
    #[cfg(target_arch = "wasm32")]
    let _certificate_batch_time = ic_cdk::api::time();
    ic_cdk::api::data_certificate()
        .ok_or_else(|| Error::Unavailable("replicated call has no query certificate".into()))
}

/// Assemble the certified values and witnesses `prove` returns for each key,
/// within the 1..=64 key and response size limits.
pub fn certified_batch(
    canister: candid::Principal,
    keys: Vec<Vec<u8>>,
    certificate: Vec<u8>,
    mut prove: impl FnMut(&[u8]) -> (Option<Vec<u8>>, HashTree),
) -> Result<CertifiedBatch> {
    ensure(
        !keys.is_empty()
            && keys.len() <= MAX_BATCH
            && certificate.len() <= MAX_CERTIFIED_RESPONSE_BYTES,
        Error::QuotaExceeded,
    )?;
    let mut bytes = certificate.len();
    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        let (value, witness) = prove(&key);
        bytes = bytes
            .saturating_add(key.len())
            .saturating_add(value.as_ref().map_or(0, Vec::len));
        ensure(bytes <= MAX_CERTIFIED_RESPONSE_BYTES, Error::QuotaExceeded)?;
        let witness = cbor2::to_vec(&witness).expect("witness");
        bytes = bytes.saturating_add(witness.len());
        ensure(bytes <= MAX_CERTIFIED_RESPONSE_BYTES, Error::QuotaExceeded)?;
        entries.push(CertifiedEntry {
            key: key.into(),
            value: value.map(ByteBuf::from),
            witness: witness.into(),
        });
    }
    let batch = CertifiedBatch {
        schema: 1,
        canister,
        certificate: certificate.into(),
        entries,
    };
    // Include Candid's type table, Result variant, vector lengths and
    // certificate. Raw field lengths alone are not a wire-response bound.
    ensure(
        candid::encode_one(Ok::<_, Error>(&batch))
            .expect("certified response")
            .len()
            <= MAX_CERTIFIED_RESPONSE_BYTES,
        Error::QuotaExceeded,
    )?;
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ic_certification::pruned;

    fn batch(keys: Vec<Vec<u8>>, value: usize, certificate: usize) -> Result<CertifiedBatch> {
        certified_batch(
            candid::Principal::from_slice(&[1]),
            keys,
            vec![0; certificate],
            |_| (Some(vec![1; value]), pruned([2; 32])),
        )
    }

    #[test]
    fn batch_limits_include_certificate_and_candid_envelope() {
        let keys = (0..64u8).map(|key| vec![key]).collect::<Vec<_>>();
        let witness = cbor2::to_vec(&pruned([2; 32])).unwrap().len();
        // The first value size rejected for 64 keys and a 1,000-byte certificate.
        let rejected = (3_000..4_100)
            .find(|&v| batch(keys.clone(), v, 1000).is_err())
            .unwrap();
        // Raw lengths still fit: Candid's type table, variants and vector
        // lengths are what exceed the limit.
        assert!(1000 + 64 * (1 + rejected + witness) <= MAX_CERTIFIED_RESPONSE_BYTES);
        let accepted = batch(keys.clone(), rejected - 1, 1000).unwrap();
        assert!(
            candid::encode_one(Ok::<_, Error>(accepted)).unwrap().len()
                <= MAX_CERTIFIED_RESPONSE_BYTES
        );
        assert_eq!(batch(keys, rejected, 1000), Err(Error::QuotaExceeded));
        for keys in [vec![], vec![vec![0]; 65]] {
            assert_eq!(batch(keys, 1, 0), Err(Error::QuotaExceeded));
        }
        assert_eq!(
            batch(vec![vec![0]], 1, MAX_CERTIFIED_RESPONSE_BYTES + 1),
            Err(Error::QuotaExceeded)
        );
    }
}
