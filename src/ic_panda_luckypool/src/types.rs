use base64::{engine::general_purpose, Engine};
use candid::CandidType;
use ciborium::{from_reader, into_writer};
use serde::{Deserialize, Serialize};

use crate::crypto::mac_256_2;

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct CaptchaOutput {
    pub img_base64: String,
    pub challenge: String,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct AirdropClaimInput {
    pub code: String,
    pub challenge: String,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct LuckyDrawInput {
    // ICP tokens to be used for luckydraw, [1, 10]
    pub icp: u8,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct LuckyDrawOutput {
    // Token amount in E8
    pub amount: u64,
    // random number generated by luckydraw
    pub random: u32,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct LogsInput {
    pub index: Option<u64>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct LogsOutput<T> {
    pub next_index: Option<u64>,
    pub logs: Vec<T>,
}

pub struct Challenge {
    pub code: String,
    pub time: u64, // in seconds since the epoch (1970-01-01)
}

impl Challenge {
    pub fn sign_to_base64(&self, key: &[u8]) -> Result<String, String> {
        let mut time: Vec<u8> = Vec::new();
        into_writer(&self.time, &mut time).map_err(|_err| "failed to encode time".to_string())?;

        let mac = &mac_256_2(key, self.code.as_bytes(), &time)[0..16];
        let mut challenge: Vec<u8> = Vec::new();
        into_writer(&vec![time.as_slice(), mac], &mut challenge)
            .map_err(|_err| "failed to encode challenge".to_string())?;

        Ok(general_purpose::URL_SAFE_NO_PAD.encode(challenge))
    }

    pub fn verify_from_base64(
        key: &[u8],
        code: &str,
        challenge: &str,
        expire_at: u64,
    ) -> Result<Self, String> {
        let challenge = general_purpose::URL_SAFE_NO_PAD
            .decode(challenge.as_bytes())
            .map_err(|_err| "invalid challenge".to_string())?;

        let arr: Vec<Vec<u8>> =
            from_reader(&challenge[..]).map_err(|_err| "failed to decode challenge")?;

        if arr.len() != 2 {
            return Err("invalid challenge".to_string());
        }
        let time: u64 = from_reader(&arr[0][..]).map_err(|_err| "failed to decode time")?;
        if time < expire_at {
            return Err("challenge expired".to_string());
        }

        let mac = &mac_256_2(key, code.as_bytes(), &arr[0])[0..16];
        if mac != &arr[1][..] {
            return Err("failed to verify challenge".to_string());
        }

        Ok(Challenge {
            code: code.to_string(),
            time,
        })
    }
}
