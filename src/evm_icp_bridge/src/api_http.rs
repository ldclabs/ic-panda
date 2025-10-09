use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use candid::CandidType;
use ic_auth_types::{cbor_into_vec, ByteBufB64};
use ic_http_certification::{HeaderField, HttpRequest};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::store;

#[derive(CandidType, Deserialize, Serialize, Clone, Default)]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: Vec<HeaderField>,
    pub body: ByteBufB64,
    pub upgrade: Option<bool>,
}

static CBOR: &str = "application/cbor";
static JSON: &str = "application/json";
static IC_CERTIFICATE_HEADER: &str = "ic-certificate";
static IC_CERTIFICATE_EXPRESSION_HEADER: &str = "ic-certificateexpression";

#[ic_cdk::query(hidden = true)]
async fn http_request(request: HttpRequest<'static>) -> HttpResponse {
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

    let rt = match (request.method().as_str(), req_url.path()) {
        ("HEAD", _) => Ok(Vec::new()),
        ("GET", "/") => {
            let info = store::state::info();
            if in_cbor {
                cbor_into_vec(&info)
                    .map_err(|err| format!("failed to serialize info to cbor: {err}"))
            } else {
                serde_json::to_vec(&info)
                    .map_err(|err| format!("failed to serialize info to json: {err}"))
            }
        }
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
