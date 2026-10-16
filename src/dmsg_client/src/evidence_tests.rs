use super::*;
use crate::testing::{device, security, signed};

const REVISION: &str = "dmsg/vault/revision/v1";

type Payload = BTreeMap<String, String>;

#[test]
fn commands_verify_against_the_devices_of_their_epoch() {
    let (a, active) = device(1, None);
    let (b, other) = device(2, None);
    // Device A signed under epoch 3; recovery later replaced it with B.
    let then = security(3, BTreeMap::from([(a, active.clone())]));
    let today = security(4, BTreeMap::from([(b, other)]));
    let cose = signed(1, 3, REVISION);
    let (signer, body) = under::<Payload>(&then, &cose, Some(REVISION)).unwrap();
    assert_eq!(signer, a);
    assert_eq!(body.payload["by"], "1");
    // Today's devices know nothing of A.
    assert_eq!(
        under::<Payload>(&today, &cose, Some(REVISION)).unwrap_err(),
        Error::AuthRequired
    );
    // The command must be of the evidence's epoch and of the action asked.
    assert_eq!(
        under::<Payload>(&then, &signed(1, 2, REVISION), Some(REVISION)).unwrap_err(),
        Error::PolicyStale
    );
    assert_eq!(
        under::<Payload>(&then, &cose, Some("dmsg/upload/reserve/v1")).unwrap_err(),
        Error::IntegrityFailed
    );
    // At that epoch the device had to be unrevoked and a content signer.
    let (_, revoked) = device(1, Some(5));
    assert_eq!(
        under::<Payload>(&security(3, BTreeMap::from([(a, revoked)])), &cose, None).unwrap_err(),
        Error::AuthRequired
    );
    let mut reader = active;
    reader.input.capabilities = vec![Capability::VaultUnlock];
    assert_eq!(
        under::<Payload>(&security(3, BTreeMap::from([(a, reader)])), &cose, None).unwrap_err(),
        Error::AuthRequired
    );
}
