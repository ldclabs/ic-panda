//! ICP-certified HTTP for principal documents.
//!
//! Every response is certified as response-only with all headers, so an HTTP
//! gateway rejects any document, 404 or domain list this canister did not
//! commit. The heap tree holds hashes only and is rebuilt after upgrade.
use dmsg_types::*;
use ic_http_certification::{
    utils::add_v2_certificate_header, DefaultCelBuilder, DefaultResponseCertification,
    DefaultResponseOnlyCelExpression, HttpCertification, HttpCertificationPath,
    HttpCertificationTree, HttpCertificationTreeEntry, HttpRequest, HttpResponse, StatusCode,
    CERTIFICATE_EXPRESSION_HEADER_NAME,
};
use std::{cell::RefCell, sync::LazyLock};

const ACCOUNT_PREFIX: &str = "/";
const IC_DOMAINS: &str = "/.well-known/ic-domains";
const NOT_FOUND: &[u8] =
    br#"{"error":{"code":"not_found","message":"principal document not found"}}"#;

static CEL: LazyLock<DefaultResponseOnlyCelExpression<'static>> = LazyLock::new(|| {
    DefaultCelBuilder::response_only_certification()
        .with_response_certification(DefaultResponseCertification::response_header_exclusions(
            vec![],
        ))
        .build()
});

thread_local! {
    static TREE: RefCell<HttpCertificationTree> = RefCell::new(HttpCertificationTree::default());
}

fn response(
    status: StatusCode,
    content_type: &str,
    cache: &str,
    body: Vec<u8>,
) -> HttpResponse<'static> {
    HttpResponse::builder()
        .with_status_code(status)
        .with_headers(vec![
            ("content-type".into(), content_type.into()),
            ("cache-control".into(), cache.into()),
            ("access-control-allow-origin".into(), "*".into()),
            ("x-content-type-options".into(), "nosniff".into()),
            (CERTIFICATE_EXPRESSION_HEADER_NAME.into(), CEL.to_string()),
        ])
        .with_body(body)
        .build()
}

fn document_path(id: &AccountId) -> HttpCertificationPath<'static> {
    HttpCertificationPath::exact(format!("{ACCOUNT_PREFIX}{id}"))
}

fn document_response(document: Vec<u8>) -> HttpResponse<'static> {
    response(
        StatusCode::OK,
        "application/json",
        "public, max-age=30",
        document,
    )
}

fn not_found() -> (HttpCertificationPath<'static>, HttpResponse<'static>) {
    (
        HttpCertificationPath::wildcard(""),
        response(
            StatusCode::NOT_FOUND,
            "application/json",
            "no-store",
            NOT_FOUND.to_vec(),
        ),
    )
}

fn domains(custom_domains: &[String]) -> (HttpCertificationPath<'static>, HttpResponse<'static>) {
    (
        HttpCertificationPath::exact(IC_DOMAINS),
        response(
            StatusCode::OK,
            "text/plain",
            "public, max-age=300",
            custom_domains.join("\n").into_bytes(),
        ),
    )
}

fn entry(
    path: HttpCertificationPath<'static>,
    response: &HttpResponse<'static>,
    document_digest: Option<Hash>,
) -> HttpCertificationTreeEntry<'static> {
    let certification =
        HttpCertification::response_only(&CEL, response, document_digest.map(Hash::into_array))
            .expect("certifiable response");
    HttpCertificationTreeEntry::new(path, certification)
}

fn publish_root(tree: &HttpCertificationTree) {
    #[cfg(target_arch = "wasm32")]
    ic_cdk::api::certified_data_set(tree.root_hash());
    #[cfg(not(target_arch = "wasm32"))]
    let _ = tree;
}

/// Replace the certified response of one account's document.
pub(crate) fn certify_document(id: &AccountId, document_digest: Hash) {
    let path = document_path(id);
    let entry = entry(
        path.clone(),
        &document_response(vec![]),
        Some(document_digest),
    );
    TREE.with_borrow_mut(|tree| {
        tree.delete_by_path(&path);
        tree.insert(&entry);
        publish_root(tree);
    });
}

/// Certify the fallback 404, domain list and stored document digests in one pass.
/// The iterator releases each stable record before reading the next one.
pub(crate) fn rebuild(
    custom_domains: &[String],
    documents: impl IntoIterator<Item = (AccountId, Hash)>,
) {
    TREE.with_borrow_mut(|tree| {
        tree.clear();
        for (path, response) in [not_found(), domains(custom_domains)] {
            tree.insert(&entry(path, &response, None));
        }
        for (id, document_digest) in documents {
            tree.insert(&entry(
                document_path(&id),
                &document_response(vec![]),
                Some(document_digest),
            ));
        }
        publish_root(tree);
    });
}

/// Match the certification library's path segments: internal empty segments
/// are ignored, but a trailing slash remains a distinct path.
fn routing_path(path: &str) -> String {
    let mut normalized = String::new();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        normalized.push('/');
        normalized.push_str(segment);
    }
    if path.ends_with('/') {
        normalized.push('/');
    }
    normalized
}

/// Select the stored response for a request path and attach its certificate.
pub(crate) fn serve(
    request: &HttpRequest<'static>,
    custom_domains: &[String],
    load: impl Fn(&AccountId) -> Option<(serde_bytes::ByteBuf, Hash)>,
) -> HttpResponse<'static> {
    let path = request.get_path().unwrap_or_default();
    let route = routing_path(&path);
    let document = route
        .strip_prefix(ACCOUNT_PREFIX)
        .and_then(|text| text.parse::<AccountId>().ok())
        .filter(|id| route == format!("{ACCOUNT_PREFIX}{id}"))
        .and_then(|id| load(&id).map(|document| (id, document)));
    let (expr_path, mut response, document_digest) = match document {
        Some((id, (document, digest))) => (
            document_path(&id),
            document_response(document.into_vec()),
            Some(digest),
        ),
        None => {
            let (path, response) = if route == IC_DOMAINS {
                domains(custom_domains)
            } else {
                not_found()
            };
            (path, response, None)
        }
    };
    let certified = entry(expr_path.clone(), &response, document_digest);
    let witness = TREE.with_borrow(|tree| tree.witness(&certified, &path));
    if let (Ok(witness), Some(certificate)) = (witness, ic_cdk::api::data_certificate()) {
        add_v2_certificate_header(
            &certificate,
            &mut response,
            &witness,
            &expr_path.to_expr_path(),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_matches_certification_segments_and_keeps_trailing_slash() {
        for (path, expected) in [
            ("", ""),
            ("/", "/"),
            ("///", "/"),
            ("/account", "/account"),
            ("//account", "/account"),
            ("//account//", "/account/"),
            ("/.well-known//ic-domains", IC_DOMAINS),
        ] {
            assert_eq!(routing_path(path), expected);
            assert_eq!(
                HttpCertificationPath::exact(path).to_expr_path(),
                HttpCertificationPath::exact(expected).to_expr_path(),
            );
        }
    }

    #[test]
    fn persisted_body_hash_produces_the_same_certification() {
        let id = AccountId([9; 12]);
        for body in [
            vec![],
            br#"{"controllers":[]}"#.to_vec(),
            vec![b'a'; 65_536],
        ] {
            let digest = dmsg_protocol::sha256(&body);
            let mut original = HttpCertificationTree::default();
            original.insert(&entry(document_path(&id), &document_response(body), None));
            let mut prehashed = HttpCertificationTree::default();
            prehashed.insert(&entry(
                document_path(&id),
                &document_response(vec![]),
                Some(digest),
            ));
            assert_eq!(original.root_hash(), prehashed.root_hash());
        }
    }
}
