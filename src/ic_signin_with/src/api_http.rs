use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use candid::{CandidType, Principal};
use cbor2::from_slice;
use ic_auth_types::{cbor_into_vec, ByteArrayB64, ByteBufB64};
use ic_http_certification::{HeaderField, HttpRequest};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::store;

const CBOR: &str = "application/cbor";
const JSON: &str = "application/json";
const IC_CERTIFICATE_HEADER: &str = "ic-certificate";
const IC_CERTIFICATE_EXPRESSION_HEADER: &str = "ic-certificateexpression";
const MAX_HTTP_REQUEST_BODY_BYTES: usize = 512 * 1_024;

#[derive(CandidType, Serialize)]
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

#[ic_cdk::query(hidden = true)]
fn http_request(request: HttpRequest<'static>) -> HttpResponse {
    // Build the certification headers first so error responses stay verifiable by the
    // HTTP gateway; an uncertified response is rejected before the client ever sees it.
    let certified = certified_headers(request.url());
    let req_path = match request.get_path() {
        Ok(path) => path,
        Err(err) => {
            let headers = certified.unwrap_or_else(|_| basic_headers());
            return error_response(400, err.to_string(), headers);
        }
    };

    let mut headers = match certified {
        Ok(headers) => headers,
        Err(err) => return error_response(500, err, basic_headers()),
    };
    let request_cbor = header_contains(request.headers(), "content-type", CBOR);
    let response_cbor = request_cbor || header_contains(request.headers(), "accept", CBOR);

    let result = match (request.method().as_str(), req_path.as_str()) {
        ("HEAD", _) => Ok(Vec::new()),
        ("POST", "/verify_envelope") => post_verify(request.body(), request_cbor, response_cbor),
        (method, path) => Err(HttpError {
            status_code: 404,
            message: format!("method {method}, path: {path}"),
        }),
    };

    match result {
        Ok(body) => {
            headers.push((
                "content-type".to_string(),
                if response_cbor { CBOR } else { JSON }.to_string(),
            ));
            headers.push(("content-length".to_string(), body.len().to_string()));
            HttpResponse {
                status_code: 200,
                headers,
                body: body.into(),
                upgrade: None,
            }
        }
        Err(err) => error_response(err.status_code, err.message, headers),
    }
}

fn certified_headers(request_url: &str) -> Result<Vec<HeaderField>, String> {
    let witness = store::state::http_witness(request_url)?;
    let certificate = ic_cdk::api::data_certificate()
        .ok_or_else(|| "data certificate is unavailable".to_string())?;
    let witness = cbor_into_vec(&witness)
        .map_err(|err| format!("failed to serialize HTTP witness: {err}"))?;
    let expression_path = cbor_into_vec(&store::state::DEFAULT_EXPR_PATH.to_expr_path())
        .map_err(|err| format!("failed to serialize expression path: {err}"))?;

    Ok(vec![
        ("x-content-type-options".to_string(), "nosniff".to_string()),
        (
            IC_CERTIFICATE_EXPRESSION_HEADER.to_string(),
            store::state::DEFAULT_CEL_EXPR.clone(),
        ),
        (
            IC_CERTIFICATE_HEADER.to_string(),
            format!(
                "certificate=:{}:, tree=:{}:, expr_path=:{}:, version=2",
                BASE64.encode(certificate),
                BASE64.encode(witness),
                BASE64.encode(expression_path),
            ),
        ),
    ])
}

fn post_verify(body: &[u8], request_cbor: bool, response_cbor: bool) -> Result<Vec<u8>, HttpError> {
    if body.len() > MAX_HTTP_REQUEST_BODY_BYTES {
        return Err(HttpError {
            status_code: 413,
            message: "request body is too large".to_string(),
        });
    }

    let request: VerifyEnvelopeRequest = if request_cbor {
        from_slice(body).map_err(|err| HttpError {
            status_code: 400,
            message: format!("failed to decode request body: {err}"),
        })?
    } else {
        serde_json::from_slice(body).map_err(|err| HttpError {
            status_code: 400,
            message: format!("failed to decode request body: {err}"),
        })?
    };

    let principal = crate::api::verify_envelope(
        request.signed_envelope,
        request.expect_target,
        request.expect_digest,
    )
    .map_err(|err| HttpError {
        status_code: 401,
        message: format!("failed to verify envelope: {err}"),
    })?;

    let response = VerifyEnvelopeResponse { result: principal };
    if response_cbor {
        cbor_into_vec(&response).map_err(|err| HttpError {
            status_code: 500,
            message: format!("failed to encode response body: {err}"),
        })
    } else {
        serde_json::to_vec(&response).map_err(|err| HttpError {
            status_code: 500,
            message: format!("failed to encode response body: {err}"),
        })
    }
}

fn basic_headers() -> Vec<HeaderField> {
    vec![("x-content-type-options".to_string(), "nosniff".to_string())]
}

fn error_response(
    status_code: u16,
    message: String,
    mut headers: Vec<HeaderField>,
) -> HttpResponse {
    headers.push(("content-type".to_string(), "text/plain".to_string()));
    headers.push(("content-length".to_string(), message.len().to_string()));
    HttpResponse {
        status_code,
        headers,
        body: message.into_bytes().into(),
        upgrade: None,
    }
}

fn header_contains(headers: &[HeaderField], expected_name: &str, expected_value: &str) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case(expected_name)
            && value
                .as_bytes()
                .windows(expected_value.len())
                .any(|part| part.eq_ignore_ascii_case(expected_value.as_bytes()))
    })
}

#[derive(Deserialize)]
struct VerifyEnvelopeRequest {
    signed_envelope: ByteBufB64,
    expect_target: Option<Principal>,
    expect_digest: Option<ByteArrayB64<32>>,
}

#[derive(Serialize)]
struct VerifyEnvelopeResponse {
    result: Principal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_negotiation_is_case_insensitive() {
        let headers = vec![(
            "Content-Type".to_string(),
            "Application/CBOR; charset=binary".to_string(),
        )];
        assert!(header_contains(&headers, "content-type", CBOR));
        assert!(!header_contains(&headers, "accept", CBOR));
    }
}
