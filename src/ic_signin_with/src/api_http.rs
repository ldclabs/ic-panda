use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use candid::{CandidType, Principal};
use ciborium::from_reader;
use ic_auth_types::{cbor_into_vec, ByteArrayB64, ByteBufB64};
use ic_http_certification::{HeaderField, HttpRequest};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::store;

#[derive(CandidType, Deserialize, Serialize, Clone, Default)]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: Vec<HeaderField>,
    pub body: ByteBuf,
    pub upgrade: Option<bool>,
}

struct HttpError {
    status_code: u16,
    message: String,
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

    let req_path = match request.get_path() {
        Ok(path) => path,
        Err(err) => {
            headers.push(("content-type".to_string(), "text/plain".to_string()));
            return HttpResponse {
                status_code: 400,
                headers,
                body: err.to_string().into_bytes().into(),
                upgrade: None,
            };
        }
    };

    let in_cbor = supports_cbor(request.headers());

    let rt = match (request.method().as_str(), req_path.as_str()) {
        ("HEAD", _) => Ok(Vec::new()),
        ("POST", "/verify_envelope") => post_verify(request.body(), in_cbor),
        (method, path) => Err(HttpError {
            status_code: 404,
            message: format!("method {method}, path: {path}"),
        }),
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
                status_code: err.status_code,
                headers,
                body: err.message.into_bytes().into(),
                upgrade: None,
            }
        }
    }
}

fn post_verify(body: &[u8], in_cbor: bool) -> Result<Vec<u8>, HttpError> {
    let req: VerifyEnvelopeReqeust = if in_cbor {
        from_reader(body).map_err(|e| HttpError {
            status_code: 400,
            message: format!("failed to decode request body: {:?}", e),
        })?
    } else {
        serde_json::from_slice(body).map_err(|e| HttpError {
            status_code: 400,
            message: format!("failed to decode request body: {:?}", e),
        })?
    };

    let principal =
        crate::api::verify_envelope(req.signed_envelope, req.expect_target, req.expect_digest)
            .map_err(|e| HttpError {
                status_code: 401,
                message: format!("failed to verify envelope: {}", e),
            })?;

    if in_cbor {
        cbor_into_vec(&VerifyEnvelopeResponse { result: principal }).map_err(|e| HttpError {
            status_code: 500,
            message: format!("failed to encode response body: {:?}", e),
        })
    } else {
        serde_json::to_vec(&VerifyEnvelopeResponse { result: principal }).map_err(|e| HttpError {
            status_code: 500,
            message: format!("failed to encode response body: {:?}", e),
        })
    }
}

fn supports_cbor(headers: &[HeaderField]) -> bool {
    headers
        .iter()
        .any(|(name, value)| (name == "accept" || name == "content-type") && value.contains(CBOR))
}

#[derive(Deserialize, Clone, Default)]
struct VerifyEnvelopeReqeust {
    pub signed_envelope: ByteBufB64,
    pub expect_target: Option<Principal>,
    pub expect_digest: Option<ByteArrayB64<32>>,
}

#[derive(Serialize, Clone)]
struct VerifyEnvelopeResponse {
    pub result: Principal,
}
