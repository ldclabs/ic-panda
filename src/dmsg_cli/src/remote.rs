//! The CLI's connection to its daemon: discovery file, owner credential and
//! JSON-RPC over loopback HTTP.

use crate::home::{Discovery, Home};
use serde_json::{Value, json};

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
    /// The daemon's loopback origin; `None` until an agent session finds it.
    base: std::sync::Mutex<Option<String>>,
    /// For an agent session, the home to find the daemon in again: a
    /// restarted daemon listens on another port, and one that is not
    /// running yet may start while the session lasts.
    home: Option<Home>,
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
        Self::with_token(Some(origin(&discovery)), None, token)
    }

    /// The home's daemon for a hosted agent's long-lived session, with that
    /// agent's credential instead of the owner's. It is found at the first
    /// call, so the session starts while no daemon runs, and found again
    /// when it stops answering, so a restart does not end the session.
    pub fn agent(home: &Home, credential: &str) -> Result<Self, Failure> {
        let base = home.discovery().map(|d| origin(&d));
        Self::with_token(base, Some(home.clone()), credential.into())
    }

    fn discover(home: &Home) -> Result<Discovery, Failure> {
        home.discovery().ok_or_else(stopped)
    }

    fn with_token(
        base: Option<String>,
        home: Option<Home>,
        token: String,
    ) -> Result<Self, Failure> {
        Ok(Self {
            base: std::sync::Mutex::new(base),
            home,
            token,
            http: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .build()
                .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?,
        })
    }

    /// The daemon's origin, looked up for an agent session that has none.
    fn base(&self) -> Result<String, Failure> {
        let mut base = self.base.lock().expect("daemon origin");
        if base.is_none() {
            *base = self
                .home
                .as_ref()
                .and_then(Home::discovery)
                .map(|d| origin(&d));
        }
        base.clone().ok_or_else(stopped)
    }

    /// Look the daemon up again after it stopped answering: whether an
    /// agent session now finds it somewhere else.
    fn moved(&self) -> bool {
        let Some(home) = &self.home else {
            return false;
        };
        let found = home.discovery().map(|d| origin(&d));
        let mut base = self.base.lock().expect("daemon origin");
        let moved = found.is_some() && found != *base;
        *base = found;
        moved
    }

    /// `GET /daemon/status`.
    pub async fn status(&self) -> Result<Value, Failure> {
        let request = self.http.get(format!("{}/daemon/status", self.base()?));
        self.send(request).await
    }

    /// Call one method; an agent session tries once more where a restarted
    /// daemon now listens.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Failure> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let mut reply = match self.post(&body).await {
            Err(failure) if failure.code == "DAEMON_STOPPED" && self.moved() => {
                self.post(&body).await
            }
            reply => reply,
        }?;
        if let Some(error) = reply.get("error") {
            let code = error["data"]["code"].as_str().unwrap_or("UNAVAILABLE");
            return Err(Failure::new(
                code,
                error["message"].as_str().unwrap_or_default(),
            ));
        }
        Ok(reply["result"].take())
    }

    async fn post(&self, body: &Value) -> Result<Value, Failure> {
        let request = self
            .http
            .post(format!("{}/app/v1", self.base()?))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        self.send(request).await
    }

    /// Send a request with the credential and read the JSON reply. Only
    /// a refused credential is `AUTH_REQUIRED`, and only a refused
    /// connection `DAEMON_STOPPED`.
    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value, Failure> {
        // Refused before the request left: nothing ran, and a retry may
        // find a restarted daemon. A request lost on the way may have run.
        let response =
            request
                .bearer_auth(&self.token)
                .send()
                .await
                .map_err(|e| match e.is_connect() {
                    true => Failure::new("DAEMON_STOPPED", e.to_string()),
                    false => Failure::new("UNAVAILABLE", e.to_string()),
                })?;
        match response.status() {
            status if status.is_success() => {}
            reqwest::StatusCode::UNAUTHORIZED => {
                return Err(Failure::new(
                    "AUTH_REQUIRED",
                    "the daemon refused the credential",
                ));
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

fn origin(discovery: &Discovery) -> String {
    format!("http://127.0.0.1:{}", discovery.port)
}

fn stopped() -> Failure {
    Failure::new(
        "DAEMON_STOPPED",
        "the dmsg daemon is not running; start it with `dmsg start` or open dMsg",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A daemon stand-in on a new port that answers every call with
    /// `result` and closes each connection, as a stopped process does.
    async fn answering(result: &'static str) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buf).await.unwrap();
                    request.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&request).to_lowercase();
                    let Some(end) = text.find("\r\n\r\n") else {
                        continue;
                    };
                    let length = text[..end]
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .map_or(0, |v| v.trim().parse::<usize>().unwrap());
                    if n == 0 || request.len() >= end + 4 + length {
                        break;
                    }
                }
                let body = json!({"jsonrpc": "2.0", "id": 1, "result": result}).to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (port, task)
    }

    fn publish(home: &Home, port: u16) {
        home.publish(&Discovery {
            port,
            pid: 0,
            instance: port.to_string(),
        })
        .unwrap();
    }

    #[tokio::test]
    async fn an_agent_session_outlives_its_daemon() {
        let dir = dmsg_client::testing::TempDir::new();
        let home = Home {
            dir: dir.0.join("home"),
        };
        std::fs::create_dir_all(&home.dir).unwrap();
        // Before any daemon: the session starts, and its calls say why
        // they fail.
        let session = Remote::agent(&home, "agent").unwrap();
        let stopped = session.call("mcp/tools", Value::Null).await.unwrap_err();
        assert_eq!(stopped.code, "DAEMON_STOPPED");

        let (first, task) = answering("first").await;
        publish(&home, first);
        assert_eq!(
            session.call("mcp/tools", Value::Null).await.unwrap(),
            "first"
        );

        // The daemon restarts on another port: the next call finds it.
        task.abort();
        let _ = task.await;
        let (second, _task) = answering("second").await;
        publish(&home, second);
        assert_eq!(
            session.call("mcp/tools", Value::Null).await.unwrap(),
            "second"
        );
    }
}
