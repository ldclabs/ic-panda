use cbor2::from_slice;
use ic_auth_types::{cbor_into_vec, ByteBufB64, BytesB64};
use ic_http_certification::{
    utils::add_skip_certification_header, HeaderField, HttpRequest, HttpResponse,
    HttpUpdateRequest, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::store;

const CBOR: &str = "application/cbor";
const JSON: &str = "application/json";
const MAX_HTTP_REQUEST_BODY_BYTES: usize = 512 * 1_024;
const MAX_PUBLIC_KEY_QUERY_BYTES: usize = 256;

#[derive(Debug)]
struct HttpError {
    status_code: StatusCode,
    message: String,
}

impl HttpError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

#[ic_cdk::query(hidden = true)]
fn http_request(request: HttpRequest<'static>) -> HttpResponse<'static> {
    if request.method().as_str() == "POST" {
        return HttpResponse::builder()
            .with_body(b"Upgrade".as_slice())
            .with_upgrade(true)
            .build();
    }

    let request_cbor = header_contains(request.headers(), "content-type", CBOR);
    let response_cbor = request_cbor || header_contains(request.headers(), "accept", CBOR);
    let origin = header_value(request.headers(), "origin");
    let path = match request.get_path() {
        Ok(path) => path,
        Err(err) => {
            return certify_response(error_response(HttpError::bad_request(err.to_string())))
        }
    };

    let result = match (request.method().as_str(), path.as_str()) {
        ("HEAD", _) => Ok(Vec::new()),
        ("GET", "/delegation") => request
            .get_query()
            .map_err(|err| HttpError::bad_request(err.to_string()))
            .and_then(|query| get_delegation(query.as_deref(), origin, response_cbor)),
        (method, path) => Err(HttpError {
            status_code: StatusCode::NOT_FOUND,
            message: format!("method {method}, path: {path}"),
        }),
    };

    certify_response(result_response(result, response_cbor))
}

#[ic_cdk::update(hidden = true)]
fn http_request_update(request: HttpUpdateRequest<'static>) -> HttpResponse<'static> {
    let request_cbor = header_contains(request.headers(), "content-type", CBOR);
    let response_cbor = request_cbor || header_contains(request.headers(), "accept", CBOR);
    let origin = header_value(request.headers(), "origin").unwrap_or_default();
    let path = match request.get_path() {
        Ok(path) => path,
        Err(err) => return error_response(HttpError::bad_request(err.to_string())),
    };

    let result = match (request.method().as_str(), path.as_str()) {
        ("POST", "/delegation") => {
            put_delegation(request.body(), origin, request_cbor, response_cbor)
        }
        (method, path) => Err(HttpError {
            status_code: StatusCode::NOT_FOUND,
            message: format!("method {method}, path: {path}"),
        }),
    };

    result_response(result, response_cbor)
}

#[derive(Deserialize)]
struct Request {
    payload: ByteBufB64,
}

#[derive(Serialize)]
struct Response<'a> {
    result: BytesB64<'a>,
}

fn get_delegation(
    query: Option<&str>,
    origin: Option<&str>,
    response_cbor: bool,
) -> Result<Vec<u8>, HttpError> {
    let pubkey = parse_public_key(query)?;
    store::state::with_delegation(pubkey.as_slice(), origin, |payload| {
        encode_response(payload, response_cbor)
    })
    .ok_or_else(|| HttpError {
        status_code: StatusCode::NOT_FOUND,
        message: format!("no delegation found for pubkey: {pubkey}"),
    })?
}

fn put_delegation(
    body: &[u8],
    origin: &str,
    request_cbor: bool,
    response_cbor: bool,
) -> Result<Vec<u8>, HttpError> {
    if body.len() > MAX_HTTP_REQUEST_BODY_BYTES {
        return Err(HttpError {
            status_code: StatusCode::PAYLOAD_TOO_LARGE,
            message: "request body is too large".to_string(),
        });
    }

    let request: Request = if request_cbor {
        from_slice(body).map_err(|err| {
            HttpError::bad_request(format!("failed to decode CBOR request: {err}"))
        })?
    } else {
        serde_json::from_slice(body).map_err(|err| {
            HttpError::bad_request(format!("failed to decode JSON request: {err}"))
        })?
    };

    let pubkey = store::state::put_delegation(request.payload.into_vec(), origin)
        .map_err(HttpError::bad_request)?;
    encode_response(&pubkey, response_cbor)
}

fn parse_public_key(query: Option<&str>) -> Result<ByteBufB64, HttpError> {
    let query = query.ok_or_else(|| HttpError::bad_request("missing query parameter"))?;
    if query.len() > MAX_PUBLIC_KEY_QUERY_BYTES {
        return Err(HttpError::bad_request("query parameter is too long"));
    }

    let value = query
        .split('&')
        .find_map(|pair| match pair.split_once('=') {
            Some(("pubkey", value)) => Some(value),
            _ => None,
        })
        .ok_or_else(|| HttpError::bad_request("missing pubkey query parameter"))?;

    let value = urlencoding::decode(value)
        .map_err(|err| HttpError::bad_request(format!("invalid pubkey encoding: {err}")))?;
    let pubkey = ByteBufB64::from_str(value.as_ref())
        .map_err(|err| HttpError::bad_request(format!("invalid pubkey: {err}")))?;
    if pubkey.is_empty() || pubkey.len() > store::MAX_PUBLIC_KEY_BYTES {
        return Err(HttpError::bad_request("invalid public key length"));
    }
    Ok(pubkey)
}

fn encode_response(result: &[u8], cbor: bool) -> Result<Vec<u8>, HttpError> {
    let response = Response {
        result: BytesB64::from_slice(result),
    };
    if cbor {
        cbor_into_vec(&response)
            .map_err(|err| HttpError::internal(format!("failed to encode CBOR response: {err}")))
    } else {
        serde_json::to_vec(&response)
            .map_err(|err| HttpError::internal(format!("failed to encode JSON response: {err}")))
    }
}

fn result_response(
    result: Result<Vec<u8>, HttpError>,
    response_cbor: bool,
) -> HttpResponse<'static> {
    match result {
        Ok(body) => {
            let headers = vec![
                ("x-content-type-options".to_string(), "nosniff".to_string()),
                (
                    "content-type".to_string(),
                    if response_cbor { CBOR } else { JSON }.to_string(),
                ),
                ("content-length".to_string(), body.len().to_string()),
            ];
            HttpResponse::builder()
                .with_headers(headers)
                .with_body(body)
                .build()
        }
        Err(err) => error_response(err),
    }
}

fn error_response(error: HttpError) -> HttpResponse<'static> {
    let body = error.message.into_bytes();
    let headers = vec![
        ("x-content-type-options".to_string(), "nosniff".to_string()),
        ("content-type".to_string(), "text/plain".to_string()),
        ("content-length".to_string(), body.len().to_string()),
    ];
    HttpResponse::builder()
        .with_status_code(error.status_code)
        .with_headers(headers)
        .with_body(body)
        .build()
}

fn certify_response(mut response: HttpResponse<'static>) -> HttpResponse<'static> {
    // The data certificate only exists in a non-replicated query. A replicated call of this
    // method is certified by consensus, so it must not be turned into a trap.
    if let Some(certificate) = ic_cdk::api::data_certificate() {
        add_skip_certification_header(certificate, &mut response);
    }
    response
}

fn header_value<'a>(headers: &'a [HeaderField], expected_name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(expected_name))
        .map(|(_, value)| value.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_url_encoded_public_key() {
        let pubkey = parse_public_key(Some("pubkey=AQID%2BA%3D%3D")).unwrap();
        assert_eq!(pubkey.as_slice(), &[1, 2, 3, 248]);
    }

    #[test]
    fn parses_a_public_key_that_is_not_the_first_query_parameter() {
        let pubkey = parse_public_key(Some("ts=1&pubkey=AQID%2BA%3D%3D")).unwrap();
        assert_eq!(pubkey.as_slice(), &[1, 2, 3, 248]);

        let error = parse_public_key(Some("ts=1")).unwrap_err();
        assert_eq!(error.message, "missing pubkey query parameter");
    }

    #[test]
    fn rejects_an_oversized_query_before_base64_decoding() {
        let query = format!("pubkey={}", "A".repeat(MAX_PUBLIC_KEY_QUERY_BYTES));
        let error = parse_public_key(Some(&query)).unwrap_err();
        assert_eq!(error.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(error.message, "query parameter is too long");
    }

    #[test]
    fn content_negotiation_is_case_insensitive() {
        let headers = vec![(
            "Content-Type".to_string(),
            "Application/CBOR; charset=binary".to_string(),
        )];
        assert!(header_contains(&headers, "content-type", CBOR));
        assert!(!header_contains(&headers, "accept", CBOR));
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let headers = vec![("Origin".to_string(), "https://example.com".to_string())];
        assert_eq!(
            header_value(&headers, "origin"),
            Some("https://example.com")
        );
    }
}
