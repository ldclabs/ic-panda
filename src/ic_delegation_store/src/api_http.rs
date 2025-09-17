use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use candid::CandidType;
use ciborium::from_reader;
use ic_auth_types::{cbor_into_vec, ByteBufB64};
use ic_http_certification::{HeaderField, HttpRequest, HttpUpdateRequest};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use url::Url;

use crate::store;

#[derive(CandidType, Deserialize, Serialize, Clone, Default)]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: Vec<HeaderField>,
    pub body: ByteBufB64,
    pub upgrade: Option<bool>,
}

// type HttpResponse = record {
//     status_code: nat16;
//     headers: vec HeaderField;
//     body: blob;
//     upgrade : opt bool;
//     streaming_strategy: opt StreamingStrategy;
// };

static CBOR: &str = "application/cbor";
static JSON: &str = "application/json";
static IC_CERTIFICATE_HEADER: &str = "ic-certificate";
static IC_CERTIFICATE_EXPRESSION_HEADER: &str = "ic-certificateexpression";

// request url example:
// https://asxpf-ciaaa-aaaap-an33a-cai.icp0.io/delegation?pubkey=MCowBQYDK2VwAyEAV8Gp3Z2GnO3BQWpfxQD0KWD0GVm8Mk8f_B_hGLDHYFg
// Response body example:
// {
//   "result": "pGFhYGFkgqJhZKJhZRsYboTFK2eWmGFw..gteG"
// }
#[ic_cdk::query(hidden = true)]
async fn http_request(request: HttpRequest<'static>) -> HttpResponse {
    if request.method().as_str() == "POST" {
        return HttpResponse {
            status_code: 200,
            headers: vec![],
            body: b"Upgrade".to_vec().into(),
            upgrade: Some(true),
        };
    }

    let witness = store::state::http_tree_with(|t| {
        t.witness(&store::state::DEFAULT_CERT_ENTRY, request.url())
            .expect("get witness failed")
    });

    let certified_data = ic_cdk::api::data_certificate().expect("no data certificate available");

    let mut headers = vec![
        ("x-content-type-options".to_string(), "nosniff".to_string()),
        (
            IC_CERTIFICATE_EXPRESSION_HEADER.to_string(),
            store::state::DEFAULT_CEL_EXPR.clone(),
        ),
        (
            IC_CERTIFICATE_HEADER.to_string(),
            format!(
                "certificate=:{}:, tree=:{}:, expr_path=:{}:, version=2",
                BASE64.encode(certified_data),
                BASE64.encode(cbor_into_vec(&witness).expect("failed to serialize witness")),
                BASE64.encode(
                    cbor_into_vec(&store::state::DEFAULT_EXPR_PATH.to_expr_path())
                        .expect("failed to serialize expr path")
                )
            ),
        ),
    ];

    let req_url = match parse_url(request.url()) {
        Ok(url) => url,
        Err(err) => {
            headers.push(("content-type".to_string(), "text/plain".to_string()));
            return HttpResponse {
                status_code: 400,
                headers,
                body: err.into_bytes().into(),
                upgrade: None,
            };
        }
    };

    let in_cbor = supports_cbor(request.headers());
    let origin = request
        .headers()
        .iter()
        .find(|(name, _)| name == "origin")
        .map(|(_, value)| value.clone());

    let rt = match (request.method().as_str(), req_url.path()) {
        ("HEAD", _) => Ok(Vec::new()),
        ("GET", "/delegation") => get_delegation(req_url, origin, in_cbor),
        (method, path) => Err(format!("http_request, method {method}, path: {path}")),
    };

    match rt {
        Ok(body) => {
            if in_cbor {
                headers.push(("content-type".to_string(), CBOR.to_string()));
            } else {
                headers.push(("content-type".to_string(), JSON.to_string()));
            }
            headers.push(("content-length".to_string(), body.len().to_string()));
            HttpResponse {
                status_code: 200,
                headers,
                body: body.into(),
                upgrade: None,
            }
        }
        Err(err) => {
            headers.push(("content-type".to_string(), "text/plain".to_string()));
            HttpResponse {
                status_code: 400,
                headers,
                body: err.into_bytes().into(),
                upgrade: None,
            }
        }
    }
}

// request url example:
// https://asxpf-ciaaa-aaaap-an33a-cai.icp0.io/delegation
// Request body example:
// {
//   "payload": "hMECAaEwggKtBgkq..rjv"
// }
// Response body example:
// {
//   "result": "MCowBQYDK2VwAyEAV8Gp3Z2GnO3BQWpfxQD0KWD0GVm8Mk8f_B_hGLDHYFg="
// }
#[ic_cdk::update(hidden = true)]
async fn http_request_update(request: HttpUpdateRequest<'static>) -> HttpResponse {
    let mut headers = vec![("x-content-type-options".to_string(), "nosniff".to_string())];

    let req_url = match parse_url(request.url()) {
        Ok(url) => url,
        Err(err) => {
            return HttpResponse {
                status_code: 400,
                headers,
                body: err.into_bytes().into(),
                upgrade: None,
            };
        }
    };

    let in_cbor = supports_cbor(request.headers());
    let origin = request
        .headers()
        .iter()
        .find(|(name, _)| name == "origin")
        .map(|(_, value)| value.clone())
        .unwrap_or_default();

    let rt = match (request.method().as_str(), req_url.path()) {
        ("POST", "/delegation") => put_delegation(request.body(), origin, in_cbor).await,
        (method, path) => Err(format!(
            "http_request_update, method {method}, path: {path}"
        )),
    };

    match rt {
        Ok(body) => {
            if in_cbor {
                headers.push(("content-type".to_string(), CBOR.to_string()));
            } else {
                headers.push(("content-type".to_string(), JSON.to_string()));
            }
            headers.push(("content-length".to_string(), body.len().to_string()));
            HttpResponse {
                status_code: 200,
                headers,
                body: body.into(),
                upgrade: None,
            }
        }

        Err(err) => {
            headers.push(("content-type".to_string(), "text/plain".to_string()));
            HttpResponse {
                status_code: 400,
                headers,
                body: err.into_bytes().into(),
                upgrade: None,
            }
        }
    }
}

#[derive(Deserialize)]
struct Request {
    payload: ByteBufB64,
}

#[derive(Serialize)]
struct Response {
    result: ByteBufB64,
}

fn get_delegation(url: Url, origin: Option<String>, in_cbor: bool) -> Result<Vec<u8>, String> {
    if let Some((key, value)) = url.query_pairs().next() {
        match key.as_ref() {
            "pubkey" => {
                let pubkey = ByteBufB64::from_str(value.as_ref())
                    .map_err(|err| format!("invalid pubkey: {value}, error: {err}"))?;
                if pubkey.len() > 48 {
                    return Err(format!("pubkey too long: {}", pubkey.len()));
                }

                let result = store::state::get_delegation(pubkey, origin)
                    .ok_or_else(|| format!("no delegation found for pubkey: {}", value))?;
                if in_cbor {
                    return cbor_into_vec(&Response { result })
                        .map_err(|err| format!("failed to serialize agent in CBOR, error: {err}"));
                } else {
                    return serde_json::to_vec(&Response { result })
                        .map_err(|err| format!("failed to serialize agent in JSON, error: {err}"));
                }
            }
            other => {
                Err(format!("invalid query parameter: {other}={value}"))?;
            }
        }
    }

    Err("missing query parameter".to_string())
}

async fn put_delegation(body: &[u8], origin: String, in_cbor: bool) -> Result<Vec<u8>, String> {
    let req: Request = if in_cbor {
        from_reader(body)
            .map_err(|err| format!("failed to decode Request from CBOR, error: {err}"))?
    } else {
        serde_json::from_slice(body)
            .map_err(|err| format!("failed to decode Request from JSON, error: {err}"))?
    };

    let result = store::state::put_delegation(req.payload, origin)?;
    if in_cbor {
        cbor_into_vec(&Response { result })
            .map_err(|err| format!("failed to serialize agent in CBOR, error: {err}"))
    } else {
        serde_json::to_vec(&Response { result })
            .map_err(|err| format!("failed to serialize agent in JSON, error: {err}"))
    }
}

fn parse_url(s: &str) -> Result<Url, String> {
    let url = if s.starts_with('/') {
        Url::parse(format!("http://localhost{}", s).as_str())
    } else {
        Url::parse(s)
    };
    url.map_err(|err| format!("failed to parse url {s}, error: {err}"))
}

fn supports_cbor(headers: &[HeaderField]) -> bool {
    headers
        .iter()
        .any(|(name, value)| (name == "accept" || name == "content-type") && value.contains(CBOR))
}
