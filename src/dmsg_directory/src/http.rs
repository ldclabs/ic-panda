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

fn document_response(document: &[u8]) -> HttpResponse<'static> {
    response(
        StatusCode::OK,
        "application/json",
        "public, max-age=30",
        document.to_vec(),
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
) -> HttpCertificationTreeEntry<'static> {
    let certification =
        HttpCertification::response_only(&CEL, response, None).expect("certifiable response");
    HttpCertificationTreeEntry::new(path, certification)
}

fn publish_root(tree: &HttpCertificationTree) {
    #[cfg(target_arch = "wasm32")]
    ic_cdk::api::certified_data_set(tree.root_hash());
    #[cfg(not(target_arch = "wasm32"))]
    let _ = tree;
}

/// Replace the certified response of one account's document.
pub(crate) fn certify_document(id: &AccountId, document: &[u8]) {
    let path = document_path(id);
    let entry = entry(path.clone(), &document_response(document));
    TREE.with_borrow_mut(|tree| {
        tree.delete_by_path(&path);
        tree.insert(&entry);
        publish_root(tree);
    });
}

/// Certify the fallback 404, the domain list and every stored document once.
pub(crate) fn rebuild(
    custom_domains: &[String],
    documents: Vec<(AccountId, serde_bytes::ByteBuf)>,
) {
    TREE.with_borrow_mut(|tree| {
        tree.clear();
        for (path, response) in [not_found(), domains(custom_domains)] {
            tree.insert(&entry(path, &response));
        }
        for (id, document) in documents {
            tree.insert(&entry(document_path(&id), &document_response(&document)));
        }
        publish_root(tree);
    });
}

/// Select the stored response for a request path and attach its certificate.
pub(crate) fn serve(
    request: &HttpRequest<'static>,
    custom_domains: &[String],
    load: impl Fn(&AccountId) -> Option<serde_bytes::ByteBuf>,
) -> HttpResponse<'static> {
    let path = request.get_path().unwrap_or_default();
    let document = path
        .strip_prefix(ACCOUNT_PREFIX)
        .and_then(|text| text.parse::<AccountId>().ok())
        .filter(|id| path == format!("{ACCOUNT_PREFIX}{id}"))
        .and_then(|id| load(&id).map(|document| (id, document)));
    let (expr_path, mut response) = match document {
        Some((id, document)) => (document_path(&id), document_response(&document)),
        None if path == IC_DOMAINS => domains(custom_domains),
        None => not_found(),
    };
    let certified = entry(expr_path.clone(), &response);
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
