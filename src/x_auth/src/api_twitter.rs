use crate::context::{unix_ms, ReqContext};
use crate::erring::HTTPError;
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect},
    Extension,
};
use base64::{engine::general_purpose, Engine};
use candid::Principal;
use chrono::{DateTime, Utc};
use http::header;
use lib_panda::{sha256, sha3_256, Cryptogram, Ed25519Message};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use url::Url;

use crate::api;

const AUTH_URL: &str = "https://twitter.com/i/oauth2/authorize";
const TOKEN_URL: &str = "https://api.twitter.com/2/oauth2/token";
const USER_URL: &str = "https://api.twitter.com/2/users/me";
const TOKEN_SCOPE: &str = "users.read tweet.read";

#[derive(Debug, Deserialize, Serialize)]
pub struct TwitterResponse<T> {
    pub data: T,
}

#[derive(Deserialize, Serialize)]
pub struct TwitterUser {
    pub id: String,
    pub name: String,
    pub username: String,
    pub created_at: Option<DateTime<Utc>>,
    pub verified: Option<bool>,
    pub profile_image_url: Option<String>,
    pub public_metrics: Option<BTreeMap<String, u64>>,
}

// {
//   "data": {
//     "username": "ICPandaDAO",
//     "created_at": "2024-01-27T15:15:46.000Z",
//     "id": "17512xxxxx64672",
//     "verified": false,
//     "name": "ICPanda DAO ∞",
//     "profile_image_url": "https://pbs.twimg.com/profile_images/1761955552111050752/5ZYo0yLx_normal.jpg",
//     "public_metrics": {
//       "followers_count": 4040,
//       "following_count": 303,
//       "tweet_count": 148,
//       "listed_count": 5,
//       "like_count": 280
//     }
//   }
// }

impl TwitterUser {
    pub fn is_real_account(&self) -> bool {
        if self.verified.unwrap_or(false) {
            return true;
        }

        let hours = match self.created_at {
            Some(ref ts) => (Utc::now() - *ts).num_hours(),
            None => 0,
        };

        if hours > 30 * 24 && self.profile_image_url.is_some() {
            if let Some(ref metrics) = self.public_metrics {
                if metrics.get("followers_count").unwrap_or(&0) > &3 {
                    return true;
                }
                if metrics.get("tweet_count").unwrap_or(&0) > &3 {
                    return true;
                }
            }
        }

        // if hours > 3 * 24 && self.profile_image_url.is_some() {
        //     return true;
        // }

        // if hours > 24 {
        //     if let Some(ref metrics) = self.public_metrics {
        //         if metrics.get("followers_count").unwrap_or(&0) > &3 {
        //             return true;
        //         }
        //         if metrics.get("tweet_count").unwrap_or(&0) > &3 {
        //             return true;
        //         }
        //     }
        // }

        false
    }
}

// http://localhost:8080/idp/twitter/authorize?principal=lmxhg-dwxes-way27-z7qni-kvm6v-nllp3-74whb-rxu3w-xstqk-f2hfq-2qe

pub async fn authorize(
    Extension(ctx): Extension<Arc<ReqContext>>,
    State(app): State<api::AppState>,
    input: Query<api::AuthorizeQuery>,
) -> Result<impl IntoResponse, HTTPError> {
    ctx.set_kvs(vec![
        ("action", "twitter_authorize".into()),
        ("env", input.env.clone().into()),
        ("principal", input.principal.clone().into()),
    ])
    .await;

    let principal = Principal::from_text(&input.principal)
        .map_err(|_| HTTPError::new(400, "invalid principal".to_string()))?;
    if input.env != "ic" && input.env != "test" && input.env != "local" {
        return Err(HTTPError::new(400, "invalid env".to_string()));
    }
    let state = api::OauthState((
        principal,
        input.env.clone(),
        api::CHALLENGE_EXPIRE + unix_ms() / 1000,
    ));
    let state = state.encode(&app.state_secret, None);
    let code_verifier = sha3_256(state.as_bytes());
    let code_verifier = general_purpose::URL_SAFE_NO_PAD.encode(code_verifier);
    let code_challenge = sha256(code_verifier.as_bytes());
    let code_challenge = general_purpose::URL_SAFE_NO_PAD.encode(code_challenge);
    let mut auth_url = Url::parse(AUTH_URL).expect("twitter authorize url");
    auth_url
        .query_pairs_mut()
        .append_pair("state", &state)
        .append_pair("client_id", &app.twitter.client_id)
        .append_pair("scope", TOKEN_SCOPE)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &app.twitter.callback_url)
        .append_pair("code_challenge_method", "s256")
        .append_pair("code_challenge", &code_challenge);

    // https://twitter.com/i/oauth2/authorize?state=my-state&code_challenge_method=s256&client_id=SjgzUlFBQnR3WXMxLS1ibzdiY1c6MTpjaQ&scope=tweet.read%20users.read%20offline.access&response_type=code&redirect_uri=http%3A%2F%2F127.0.0.1%3A3000%2Fcallback&code_challenge=uzFA2wi7V-I_dEd3fXDjOISMljtNJ5RWIlGPYZAMawc

    Ok(Redirect::to(auth_url.as_ref()))
}

// http://localhost:8080/idp/twitter/callback?state=glglglgd1ySsDGv5_BqFVZ6rVrfv_LHDG9N2vKcFF0csNQIaZhjEikiaeqBb6l7H7g&code=QVpvTXVEQ0JIeFd2RHc0S3paZUlaV3V4VC1NQzhYVWNqaUlnTjB0OGNqdXRXOjE3MTI4OTkxNTIxNzg6MTowOmFjOjE
pub async fn callback(
    Extension(ctx): Extension<Arc<ReqContext>>,
    State(app): State<api::AppState>,
    input: Query<api::OAuth2CallbackQuery>,
) -> Result<impl IntoResponse, HTTPError> {
    ctx.set_kvs(vec![("action", "twitter_callback".into())])
        .await;

    let now_ms = unix_ms();
    let state = api::OauthState::decode(&app.state_secret, None, &input.state)
        .map_err(|_| HTTPError::new(400, "invalid state".to_string()))?;
    if now_ms / 1000 > state.0 .2 {
        return Err(HTTPError::new(400, "state expired".to_string()));
    }
    let redirect_uri = match state.0 .1.to_lowercase().as_str() {
        "ic" => &app.ic_redirect_uri,
        "test" => &app.test_redirect_uri,
        "local" => &app.local_redirect_uri,
        _ => return Err(HTTPError::new(400, "invalid state".to_string())),
    };
    let code_verifier = sha3_256(input.state.as_bytes());
    let code_verifier = general_purpose::URL_SAFE_NO_PAD.encode(code_verifier);

    let res = match app
        .http_client
        .post(TOKEN_URL)
        .basic_auth(&app.twitter.client_id, Some(&app.twitter.client_secret))
        .header(header::ACCEPT, "application/json")
        .header(header::USER_AGENT, api::USER_AGENT)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &input.code),
            ("redirect_uri", &app.twitter.callback_url),
            ("client_id", &app.twitter.client_id),
            ("code_verifier", &code_verifier),
        ])
        .send()
        .await
    {
        Ok(res) => res,
        Err(err) => {
            ctx.set("error", format!("request twitter token: {:?}", err).into())
                .await;
            return Err(HTTPError::new(
                500,
                "failed to request twitter token".to_string(),
            ));
        }
    };

    let token_res: api::OAuth2TokenResponse = match res.json().await {
        Ok(token_res) => token_res,
        Err(err) => {
            ctx.set("error", format!("parse twitter token: {:?}", err).into())
                .await;
            return Err(HTTPError::new(
                500,
                "failed to parse twitter token".to_string(),
            ));
        }
    };

    // https://developer.twitter.com/en/docs/twitter-api/users/lookup/api-reference/get-users-me
    let res = match app
        .http_client
        .get(USER_URL)
        .query(&[(
            "user.fields",
            "id,name,username,created_at,verified,profile_image_url,public_metrics",
        )])
        .bearer_auth(token_res.access_token)
        .header(header::USER_AGENT, api::USER_AGENT)
        .send()
        .await
    {
        Ok(res) => res,
        Err(err) => {
            ctx.set("error", format!("request twitter user: {:?}", err).into())
                .await;
            return Err(HTTPError::new(
                500,
                "failed to request twitter user".to_string(),
            ));
        }
    };

    let TwitterResponse { data: user }: TwitterResponse<TwitterUser> = match res.json().await {
        Ok(user) => user,
        Err(err) => {
            ctx.set("error", format!("parse twitter user: {:?}", err).into())
                .await;
            return Err(HTTPError::new(
                500,
                "failed to parse twitter user".to_string(),
            ));
        }
    };

    let mut redirect_uri = Url::parse(redirect_uri).expect("app redirect_uri");
    if user.is_real_account() {
        let challenge = api::ChallengeState((
            state.0 .0,
            format!("X:{}", user.id),
            now_ms / 1000 + api::CHALLENGE_EXPIRE,
        ));
        let challenge = challenge.sign_to(&app.challenge_secret);

        redirect_uri.set_fragment(Some(format!("challenge={}", challenge).as_str()));
        ctx.set("challenge", challenge.into()).await;
    } else {
        redirect_uri.set_fragment(Some(
            "error=The account does not meet the verification requirements",
        ));
    }

    ctx.set_kvs(vec![
        ("id", user.id.into()),
        ("username", user.username.into()),
    ])
    .await;

    Ok(Redirect::to(redirect_uri.as_ref()))
}
