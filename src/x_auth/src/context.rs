use axum::{
    body::Body,
    http::{header, HeaderMap, Request},
    middleware::Next,
    response::Response,
};
use serde_json::Value;
use std::{collections::BTreeMap, net::IpAddr, sync::Arc, time::Instant};
use tokio::sync::RwLock;
use tower_governor::key_extractor::{KeyExtractor, SmartIpKeyExtractor};

pub use structured_logger::unix_ms;

pub static IP_KEY_EXTRACTOR: SmartIpKeyExtractor = SmartIpKeyExtractor {};

pub struct ReqContext {
    #[allow(dead_code)]
    pub rid: String, // from x-request-id header
    pub unix_ms: u64,
    pub start: Instant,
    pub ip: Option<IpAddr>,
    pub kv: RwLock<BTreeMap<String, Value>>,
}

impl ReqContext {
    pub fn new(rid: &str, ip: Option<IpAddr>) -> Self {
        Self {
            rid: rid.to_string(),
            unix_ms: unix_ms(),
            start: Instant::now(),
            ip,
            kv: RwLock::new(BTreeMap::new()),
        }
    }

    pub async fn set(&self, key: &str, value: Value) {
        let mut kv = self.kv.write().await;
        kv.insert(key.to_string(), value);
    }

    pub async fn set_kvs(&self, kvs: Vec<(&str, Value)>) {
        let mut kv = self.kv.write().await;
        for item in kvs {
            kv.insert(item.0.to_string(), item.1);
        }
    }
}

pub async fn middleware(mut req: Request<Body>, next: Next) -> Response {
    let method = req.method().to_string();
    let uri = req.uri().to_string();
    let rid = extract_header(req.headers(), "x-request-id", || "".to_string());
    let ip = IP_KEY_EXTRACTOR.extract(&req).ok();

    let ctx = Arc::new(ReqContext::new(&rid, ip));
    req.extensions_mut().insert(ctx.clone());

    let res = next.run(req).await;
    if method != "HEAD" {
        let kv = ctx.kv.read().await;
        let status = res.status().as_u16();
        let headers = res.headers();
        let ct = headers
            .get(header::CONTENT_TYPE)
            .map_or("", |v| v.to_str().unwrap_or_default());
        let ce = headers
            .get(header::CONTENT_ENCODING)
            .map_or("", |v| v.to_str().unwrap_or_default());
        log::info!(target: "api",
            method = method,
            uri = uri,
            ip = ctx.ip.map(|ip| ip.to_string()).unwrap_or_default(),
            rid = rid,
            status = status,
            start = ctx.unix_ms,
            elapsed = ctx.start.elapsed().as_millis() as u64,
            ctype = ct,
            encoding = ce,
            kv:serde = *kv;
            "",
        );
    }

    res
}

pub fn extract_header(hm: &HeaderMap, key: &str, or: impl FnOnce() -> String) -> String {
    match hm.get(key) {
        None => or(),
        Some(v) => match v.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => or(),
        },
    }
}
