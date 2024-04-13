use axum::response::IntoResponse;
use candid::Principal;
use lib_panda::SigningKey;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub static CHALLENGE_EXPIRE: u64 = 60; // 1 minutes
pub static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Clone)]
pub struct AppState {
    pub http_client: Arc<Client>,
    pub state_secret: Vec<u8>,
    pub challenge_secret: SigningKey,
    pub twitter: AuthConfig,
    pub ic_redirect_uri: String,
    pub test_redirect_uri: String,
    pub local_redirect_uri: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeQuery {
    pub principal: String,
    pub env: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OAuth2CallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OAuth2TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: Option<String>,
    pub scope: Option<String>,   // Comma-separated list of scopes
    pub expires_in: Option<u32>, // in seconds
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OauthState(pub (Principal, String, u64));

#[derive(Clone)]
pub struct AuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub callback_url: String,
}

// ChallengeState: (Principal, ID, Expire in seconds)
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct ChallengeState(pub (Principal, String, u64));

pub async fn healthz() -> impl IntoResponse {
    "OK"
}

#[cfg(test)]
mod test {
    use super::*;
    use base64::{engine::general_purpose, Engine};
    use lib_panda::{Ed25519Message, VerifyingKey};
    use rand_core::{OsRng, RngCore};

    #[test]
    fn test_ed25519_message() {
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);

        let sk = SigningKey::from_bytes(&secret);
        let pk = VerifyingKey::from(&sk);
        println!(
            "secret key: {:?}",
            general_purpose::URL_SAFE_NO_PAD.encode(sk.to_bytes())
        );
        println!(
            "public key: {:?}",
            general_purpose::URL_SAFE_NO_PAD.encode(pk.to_bytes())
        );
        let state = ChallengeState((Principal::anonymous(), "1234567890".to_string(), 1000));
        let msg = state.sign_to(&sk);
        println!("message: {}", msg); // message: glGDQQRqMTIzNDU2Nzg5MBkD6FhAyABxVV7f4L9QXL_MP0-VZE5EMzu288JeF0kHz4FxvByZlHaSmZx_BCEIxOouOY0CCgJEuTxkUnpZF24EpQozBw
        let state2 = ChallengeState::verify_from(&pk, &msg).unwrap();
        assert_eq!(state, state2);
    }
}
