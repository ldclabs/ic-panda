use hmac::{Hmac, Mac};
use sha3::Sha3_256;

pub fn mac_256(key: &[u8], add: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha3_256>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(add);
    mac.finalize().into_bytes().into()
}

pub fn mac_256_2(key: &[u8], add1: &[u8], add2: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha3_256>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(add1);
    mac.update(add2);
    mac.finalize().into_bytes().into()
}
