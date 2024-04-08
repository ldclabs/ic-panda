use base64::{engine::general_purpose, Engine};
use candid::CandidType;
use ic_cdk::api::management_canister::http_request::{
    http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformArgs,
    TransformContext,
};
use serde::{Deserialize, Serialize};
use serde_json::{self, json};

use crate::{store, utils};

const GOOGLE_RECAPTCHA_ID: &str = "6LduPbIpAAAAANSOUfb-8bU45eilZFSmlSguN5TO";

#[derive(Debug, CandidType, Serialize, Deserialize)]
pub struct RiskAnalysis {
    pub reasons: Vec<String>,
    pub score: f32,
}

#[derive(Debug, CandidType, Serialize, Deserialize)]
pub struct TokenProperties {
    pub action: String,
    pub hostname: String,
    pub valid: bool,
}

// {
//     "riskAnalysis": {
//         "extendedVerdictReasons": [],
//         "reasons": [],
//         "score": 0.9
//     },
//     "tokenProperties": {
//         "action": "test",
//         "androidPackageName": "",
//         "createTime": "2024-04-07T23:51:57.923Z",
//         "hostname": "panda.fans",
//         "invalidReason": "INVALID_REASON_UNSPECIFIED",
//         "iosBundleId": "",
//         "valid": true
//     }
// }
#[derive(Debug, CandidType, Serialize, Deserialize)]
pub struct VerifyResponse {
    #[serde(rename = "riskAnalysis")]
    pub risk_analysis: RiskAnalysis,
    #[serde(rename = "tokenProperties")]
    pub token_properties: TokenProperties,
}

impl VerifyResponse {
    pub fn is_valid(&self, score: f32) -> bool {
        self.token_properties.valid && self.risk_analysis.score >= score
    }
}

pub async fn verify(token: &str, action: &str) -> Result<VerifyResponse, String> {
    let url = "https://grecaptcha.panda.fans/URL_GRE";
    let access_token = store::access_token::with_token(|t| format!("Bearer {}", t.0));

    let request_headers = vec![
        HttpHeader {
            name: "host".to_string(),
            value: "grecaptcha.panda.fans:443".to_string(),
        },
        HttpHeader {
            name: "user-agent".to_string(),
            value: "ic_panda_luckypool canister".to_string(),
        },
        HttpHeader {
            name: "idempotency-key".to_string(),
            value: general_purpose::URL_SAFE_NO_PAD.encode(utils::sha3_256(token.as_bytes())),
        },
        HttpHeader {
            name: "content-type".to_string(),
            value: "application/json".to_string(),
        },
        HttpHeader {
            name: "x-json-mask".to_string(),
            value: "riskAnalysis,tokenProperties".to_string(),
        },
        HttpHeader {
            name: "authorization".to_string(),
            value: access_token,
        },
    ];

    let body = json!(
        {
            "event": {
                "token": token,
                "expectedAction": action,
                "siteKey": GOOGLE_RECAPTCHA_ID,
            }
        }
    );
    let body: Vec<u8> =
        serde_json::to_vec(&body).map_err(|e| format!("Failed to serialize body: {}", e))?;

    let request = CanisterHttpRequestArgument {
        url: url.to_string(),
        max_response_bytes: None, //optional for request
        method: HttpMethod::POST,
        headers: request_headers,
        body: Some(body.clone()),
        transform: Some(TransformContext::from_name(
            "transform_recaptcha".to_string(),
            vec![],
        )), //optional for request
    };

    match http_request(request, 21_000_000_000).await {
        Ok((res,)) => {
            if res.status == 200u64 {
                let mut response: VerifyResponse =
                    serde_json::from_slice(&res.body).map_err(|e| {
                        format!(
                            "Failed to deserialize response: {}, {}, {}",
                            e,
                            String::from_utf8(body).unwrap_or_default(),
                            String::from_utf8(res.body).unwrap_or_default()
                        )
                    })?;
                if response.token_properties.action != action {
                    response.token_properties.valid = false;
                }
                Ok(response)
            } else {
                Err(format!(
                    "Failed to send request. status: {}, body: {:?}, url: {}",
                    res.status,
                    String::from_utf8(res.body).unwrap_or_default(),
                    url
                ))
            }
        }
        Err((r, m)) => Err(format!(
            "The http_request resulted into error. code: {r:?}, error: {m}"
        )),
    }
}

#[ic_cdk::query(hidden = true)]
fn transform_recaptcha(raw: TransformArgs) -> HttpResponse {
    HttpResponse {
        status: raw.response.status.clone(),
        body: raw.response.body,
        ..Default::default()
    }
}
