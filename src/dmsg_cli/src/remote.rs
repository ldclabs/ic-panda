//! The CLI's connection to its daemon: discovery file, owner credential and
//! JSON-RPC over loopback HTTP.

use crate::home::Home;
use serde_json::{json, Value};

/// A failed call: a stable code and a message.
#[derive(Debug)]
pub struct Failure {
    /// `UNAVAILABLE`, `AUTH_REQUIRED`, `LOCKED` and the other daemon codes.
    pub code: String,
    /// Human-readable detail.
    pub message: String,
}

impl Failure {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// A connected daemon.
pub struct Remote {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl Remote {
    /// Find the home's daemon.
    pub fn connect(home: &Home) -> Result<Self, Failure> {
        let discovery = home.discovery().ok_or_else(|| {
            Failure::new(
                "DAEMON_STOPPED",
                "the dmsg daemon is not running; start it with `dmsg daemon`",
            )
        })?;
        let token = home
            .owner_token(false)
            .map_err(|e| Failure::new("AUTH_REQUIRED", e.to_string()))?;
        Ok(Self {
            base: format!("http://127.0.0.1:{}", discovery.port),
            token,
            http: reqwest::Client::builder()
                .no_proxy()
                .build()
                .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?,
        })
    }

    /// `GET /daemon/status`.
    pub async fn status(&self) -> Result<Value, Failure> {
        let response = self
            .http
            .get(format!("{}/daemon/status", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| Failure::new("DAEMON_STOPPED", e.to_string()))?;
        if !response.status().is_success() {
            return Err(Failure::new("AUTH_REQUIRED", response.status().to_string()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))
    }

    /// Call one method.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Failure> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let response = self
            .http
            .post(format!("{}/app/v1", self.base))
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| Failure::new("DAEMON_STOPPED", e.to_string()))?;
        if !response.status().is_success() {
            return Err(Failure::new("AUTH_REQUIRED", response.status().to_string()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?;
        let mut reply: Value = serde_json::from_slice(&bytes)
            .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?;
        if let Some(error) = reply.get("error") {
            let code = error["data"]["code"].as_str().unwrap_or("UNAVAILABLE");
            return Err(Failure::new(
                code,
                error["message"].as_str().unwrap_or_default(),
            ));
        }
        Ok(reply["result"].take())
    }
}
