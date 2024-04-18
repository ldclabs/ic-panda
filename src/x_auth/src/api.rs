use crate::cbor::Cbor;
use crate::context::{unix_ms, ReqContext};
use crate::erring::{HTTPError, SuccessResponse};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Extension,
};
use candid::Principal;
use lib_panda::{ChallengeState, Ed25519Message, SigningKey};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::sync::Arc;

pub static CHALLENGE_EXPIRE: u64 = 180; // 3 minutes
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

#[derive(Clone)]
pub struct AuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub callback_url: String,
}

pub async fn healthz() -> impl IntoResponse {
    "OK"
}

#[derive(Clone, Deserialize)]
pub struct ChallengeInput {
    pub principal: Principal,
    pub message: ByteBuf,
}

pub async fn challenge(
    Extension(ctx): Extension<Arc<ReqContext>>,
    State(app): State<AppState>,
    Path(kind): Path<String>,
    Cbor(input): Cbor<ChallengeInput>,
) -> Result<impl IntoResponse, HTTPError> {
    ctx.set_kvs(vec![
        ("action", "challenge".into()),
        ("kind", kind.as_str().into()),
        ("principal", input.principal.to_string().into()),
    ])
    .await;

    match kind.as_str() {
        "claim_prize" => {}
        _ => return Err(HTTPError::new(400, format!("invalid kind: {}", kind))),
    }

    let state = ChallengeState((input.principal, input.message, 60 + unix_ms() / 1000));
    Ok(Cbor(SuccessResponse::new(
        state.sign_to(&app.challenge_secret),
    )))
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
