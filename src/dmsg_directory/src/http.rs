//! ICP-certified HTTP for principal documents.
//!
//! Every response is certified as response-only with all headers, so an HTTP
//! gateway rejects any document, 404 or domain list this canister did not
//! commit. Under `http_expr`, the first segment of each certified expression
//! path is a key of a stable-memory certified map holding that segment's
//! subtree hash: one per account document, `.well-known` for the domain list
//! and `<*>` for the fallback 404. Only hashes are stored, so an upgrade
//! republishes the root without visiting the documents.
use crate::store::{memory, Memory};
use dmsg_runtime::cert_map::CertMap;
use dmsg_types::*;
use ic_certification::{labeled, labeled_hash, HashTree, SubtreeLookupResult};
use ic_http_certification::{
    utils::{add_v2_certificate_header, more_specific_wildcards_for},
    DefaultCelBuilder, DefaultResponseCertification, DefaultResponseOnlyCelExpression,
    HttpCertification, HttpCertificationPath, HttpCertificationTree, HttpCertificationTreeEntry,
    HttpRequest, HttpResponse, StatusCode, CERTIFICATE_EXPRESSION_HEADER_NAME,
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

const PATH_PREFIX: &[u8] = b"http_expr";

thread_local! {
    static TREE: RefCell<CertMap<Memory>> = RefCell::new(CertMap::new(memory(2), memory(3)));
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

// The first segment of a certified expression path: its key under `http_expr`.
fn segment(path: &HttpCertificationPath) -> Vec<u8> {
    path.to_expr_path().swap_remove(1).into_bytes()
}

// The subtree a single entry certifies under its first segment.
fn subtree(entry: &HttpCertificationTreeEntry, segment: &[u8]) -> HashTree {
    let mut tree = HttpCertificationTree::default();
    tree.insert(entry);
    match tree.as_hash_tree().lookup_subtree([PATH_PREFIX, segment]) {
        SubtreeLookupResult::Found(child) => child,
        _ => unreachable!("a single-entry tree holds its segment"),
    }
}

fn certify(
    path: HttpCertificationPath<'static>,
    response: &HttpResponse<'static>,
    document_digest: Option<Hash>,
) {
    let key = segment(&path);
    let hash = subtree(&entry(path, response, document_digest), &key).digest();
    TREE.with_borrow_mut(|tree| tree.set(key, hash));
    publish();
}

pub(crate) fn publish() {
    let root = TREE.with_borrow(|tree| labeled_hash(PATH_PREFIX, &tree.root_hash()));
    #[cfg(target_arch = "wasm32")]
    ic_cdk::api::certified_data_set(root);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = root;
}

/// Replace the certified response of one account's document.
pub(crate) fn certify_document(id: &AccountId, document_digest: Hash) {
    certify(
        document_path(id),
        &document_response(vec![]),
        Some(document_digest),
    );
}

/// Replace the certified domain list.
pub(crate) fn certify_domains(custom_domains: &[String]) {
    let (path, response) = domains(custom_domains);
    certify(path, &response, None);
}

/// Certify the fallback 404 and the domain list, which follow this code
/// rather than stored records; documents keep their certified hashes.
pub(crate) fn certify_fixed(custom_domains: &[String]) {
    let (path, response) = not_found();
    certify(path, &response, None);
    certify_domains(custom_domains);
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
    let witness = witness(
        &path,
        &expr_path,
        document_digest.is_none() && route != IC_DOMAINS,
        custom_domains,
        &load,
    );
    if let Some(certificate) = ic_cdk::api::data_certificate() {
        add_v2_certificate_header(
            &certificate,
            &mut response,
            &witness,
            &expr_path.to_expr_path(),
        );
    }
    response
}

/// Witness for a response certified at `expr_path`. The fallback must also
/// prove that neither the exact request path nor a more specific wildcard
/// exists, so the first segment of each of those paths is revealed too.
fn witness(
    path: &str,
    expr_path: &HttpCertificationPath,
    fallback_response: bool,
    custom_domains: &[String],
    load: &impl Fn(&AccountId) -> Option<(serde_bytes::ByteBuf, Hash)>,
) -> HashTree {
    let mut keys = vec![segment(expr_path)];
    if fallback_response {
        let bytes =
            |p: Vec<String>| -> Vec<Vec<u8>> { p.into_iter().map(String::into_bytes).collect() };
        let request = bytes(HttpCertificationPath::exact(path.to_string()).to_expr_path());
        keys.push(request[1].clone());
        for more in more_specific_wildcards_for(&request, &bytes(expr_path.to_expr_path())) {
            keys.extend(more.into_iter().next());
        }
    }
    let (fallback, fallback_response) = not_found();
    let (listed, listed_response) = domains(custom_domains);
    let child = |key: &[u8]| {
        if key == segment(&fallback).as_slice() {
            return subtree(&entry(fallback.clone(), &fallback_response, None), key);
        }
        if key == segment(&listed).as_slice() {
            return subtree(&entry(listed.clone(), &listed_response, None), key);
        }
        let id = std::str::from_utf8(key)
            .ok()
            .and_then(|text| text.parse::<AccountId>().ok())
            .expect("certified account segment");
        let (_, digest) = load(&id).expect("certified document");
        subtree(
            &entry(document_path(&id), &document_response(vec![]), Some(digest)),
            key,
        )
    };
    let keys: Vec<&[u8]> = keys.iter().map(Vec::as_slice).collect();
    labeled(
        PATH_PREFIX,
        TREE.with_borrow(|tree| tree.witness(&keys, child)),
    )
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
    fn fallback_witness_proves_more_specific_paths_absent() {
        use ic_certification::hash_tree::SubtreeLookupResult::*;
        let domains = vec!["id.dmsg.test".to_string()];
        certify_fixed(&domains);
        let id = AccountId([9; 12]);
        let digest = dmsg_protocol::sha256(b"{}");
        certify_document(&id, digest);
        let load =
            |found: &AccountId| (*found == id).then(|| (serde_bytes::ByteBuf::new(), digest));
        let root = TREE.with_borrow(|tree| labeled_hash(PATH_PREFIX, &tree.root_hash()));
        let (fallback, _) = not_found();
        for path in [
            "/unknown",
            "/",
            "/a/b",
            &format!("/{id}/"),
            "/.well-known/x",
            "/<*>",
        ] {
            let tree = witness(path, &fallback, true, &domains, &load);
            assert_eq!(tree.digest(), root, "{path}");
            let request = HttpCertificationPath::exact(path.to_string()).to_expr_path();
            let lookup =
                |parts: Vec<String>| tree.lookup_subtree(parts.iter().map(|p| p.as_bytes()));
            assert!(matches!(lookup(request.clone()), Absent), "{path} exact");
            let bytes = |p: &[String]| -> Vec<Vec<u8>> {
                p.iter().map(|s| s.as_bytes().to_vec()).collect()
            };
            let wildcards =
                more_specific_wildcards_for(&bytes(&request), &bytes(&fallback.to_expr_path()));
            assert!(!wildcards.is_empty());
            for more in wildcards {
                let more: Vec<String> = std::iter::once("http_expr".to_string())
                    .chain(more.into_iter().map(|p| String::from_utf8(p).unwrap()))
                    .collect();
                assert!(matches!(lookup(more.clone()), Absent), "{path} {more:?}");
            }
            assert!(
                matches!(lookup(fallback.to_expr_path()), Found(_)),
                "{path} fallback"
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
