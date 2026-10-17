//! The CLI's connection to its daemon: discovery file, owner credential and
//! JSON-RPC over loopback HTTP.

use crate::home::{Discovery, Home};
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
    /// Find the home's daemon. A stopped daemon comes first: before its
    /// first start there is no owner credential either.
    pub fn connect(home: &Home) -> Result<Self, Failure> {
        let discovery = Self::discover(home)?;
        let settings = home
            .settings()
            .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
        let token = home
            .owner_token(&settings, false)
            .map_err(|e| Failure::new("AUTH_REQUIRED", e.to_string()))?;
        Self::with_token(&discovery, token)
    }

    /// Find the home's daemon as a hosted agent, with that agent's
    /// credential instead of the owner's.
    pub fn agent(home: &Home, credential: &str) -> Result<Self, Failure> {
        Self::with_token(&Self::discover(home)?, credential.into())
    }

    fn discover(home: &Home) -> Result<Discovery, Failure> {
        home.discovery().ok_or_else(|| {
            Failure::new(
                "DAEMON_STOPPED",
                "the dmsg daemon is not running; start it with `dmsg start`",
            )
        })
    }

    fn with_token(discovery: &Discovery, token: String) -> Result<Self, Failure> {
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
        let request = self.http.get(format!("{}/daemon/status", self.base));
        self.send(request).await
    }

    /// Call one method.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Failure> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let request = self
            .http
            .post(format!("{}/app/v1", self.base))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        let mut reply = self.send(request).await?;
        if let Some(error) = reply.get("error") {
            let code = error["data"]["code"].as_str().unwrap_or("UNAVAILABLE");
            return Err(Failure::new(
                code,
                error["message"].as_str().unwrap_or_default(),
            ));
        }
        Ok(reply["result"].take())
    }

    /// Send a request with the credential and read the JSON reply. Only
    /// a refused credential is `AUTH_REQUIRED`.
    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value, Failure> {
        let response = request
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| Failure::new("DAEMON_STOPPED", e.to_string()))?;
        match response.status() {
            status if status.is_success() => {}
            reqwest::StatusCode::UNAUTHORIZED => {
                return Err(Failure::new(
                    "AUTH_REQUIRED",
                    "the daemon refused the credential",
                ))
            }
            status => return Err(Failure::new("UNAVAILABLE", status.to_string())),
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))
    }
}
