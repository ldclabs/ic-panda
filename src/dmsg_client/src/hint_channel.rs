//! One connection of the relay's hint channel (connect-v1 §7.5): a WebSocket
//! authenticated by a bearer token that receives only content-free hints.

use crate::diagnostic::Diagnostic;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::{Instant, Interval};
use tokio_tungstenite::tungstenite::{self, Message, client::IntoClientRequest};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// The client heartbeat; the relay answers each `ping` with `pong`.
const PING: Duration = Duration::from_secs(25);
/// Silence after which the connection counts as lost.
const SILENCE: Duration = Duration::from_secs(60);

/// What moved, never its content.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    /// The account the hint is for.
    pub account: String,
    /// `connect`, `mcp-event`, `channel`, `inbox`, `notification`, `content` or `read`.
    pub kind: String,
    /// The channel of a `channel` or `read` hint.
    #[serde(default)]
    pub resource: Option<String>,
    /// A `channel` hint's `message` or `control`.
    #[serde(default)]
    pub event: Option<String>,
    /// How far a `content` hint's account content moved.
    #[serde(default)]
    pub cursor: Option<u64>,
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// An open hint channel.
pub struct Connection {
    sink: SplitSink<Socket, Message>,
    stream: SplitStream<Socket>,
    ping: Interval,
    heard: Instant,
}

/// The hint channel of `account` on the relay at `origin`.
pub fn url(origin: &str, account: &str) -> String {
    let origin = origin
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    format!("{origin}/v1/hints/ws?account={account}")
}

impl Connection {
    /// Connect with a registered token.
    ///
    /// # Errors
    /// A failure to connect, as a privacy-safe category.
    pub async fn open(url: &str, token: &str) -> Result<Self, Diagnostic> {
        let mut request = url.into_client_request().map_err(failure)?;
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {token}")
                .parse()
                .map_err(|_| Diagnostic::new("hints", "invalid_endpoint"))?,
        );
        let (socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(failure)?;
        let (sink, stream) = socket.split();
        Ok(Self {
            sink,
            stream,
            ping: tokio::time::interval(PING),
            heard: Instant::now(),
        })
    }

    /// The next hint; `None` once the relay closed the connection.
    ///
    /// # Errors
    /// A lost connection, as a privacy-safe category.
    pub async fn next(&mut self) -> Result<Option<Hint>, Diagnostic> {
        loop {
            tokio::select! {
                _ = self.ping.tick() => {
                    if self.heard.elapsed() > SILENCE {
                        return Err(Diagnostic::new("hints", "heartbeat_timeout"));
                    }
                    self.sink.send(Message::Text("ping".into())).await.map_err(failure)?;
                }
                message = self.stream.next() => {
                    let Some(message) = message else { return Ok(None) };
                    let message = message.map_err(failure)?;
                    self.heard = Instant::now();
                    let Message::Text(text) = message else { continue };
                    if let Some(hint) = parse(&text) {
                        return Ok(Some(hint));
                    }
                }
            }
        }
    }
}

/// A hint frame; a `pong` or anything else is no hint.
fn parse(text: &str) -> Option<Hint> {
    #[derive(Deserialize)]
    struct Frame {
        r#type: String,
        #[serde(flatten)]
        hint: Hint,
    }
    serde_json::from_str::<Frame>(text)
        .ok()
        .filter(|frame| frame.r#type == "hint")
        .map(|frame| frame.hint)
}

/// WebSocket errors may contain a handshake body, URL or close-frame text.
fn failure(error: tungstenite::Error) -> Diagnostic {
    use tungstenite::Error;
    match error {
        Error::Io(error) => Diagnostic::io("hints", &error),
        Error::Tls(_) => Diagnostic::new("hints", "tls"),
        Error::Http(response) => Diagnostic::http("hints", response.status().as_u16()),
        Error::ConnectionClosed | Error::AlreadyClosed => {
            Diagnostic::new("hints", "connection_closed")
        }
        Error::Url(_) => Diagnostic::new("hints", "invalid_endpoint"),
        _ => Diagnostic::new("hints", "protocol"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_hint_frames_are_hints_and_extra_fields_stay_optional() {
        assert_eq!(
            parse(r#"{"type":"hint","account":"a","kind":"content","cursor":7}"#),
            Some(Hint {
                account: "a".into(),
                kind: "content".into(),
                resource: None,
                event: None,
                cursor: Some(7),
            })
        );
        let channel = parse(
            r#"{"type":"hint","account":"a","kind":"channel","resource":"c","event":"message"}"#,
        )
        .unwrap();
        assert_eq!(channel.resource.as_deref(), Some("c"));
        assert_eq!(channel.event.as_deref(), Some("message"));
        assert_eq!(parse("pong"), None);
        assert_eq!(parse(r#"{"type":"other","account":"a","kind":"x"}"#), None);
    }

    #[test]
    fn the_url_follows_the_relay_scheme() {
        assert_eq!(
            url("https://relay.test", "a"),
            "wss://relay.test/v1/hints/ws?account=a"
        );
        assert_eq!(
            url("http://127.0.0.1:1", "a"),
            "ws://127.0.0.1:1/v1/hints/ws?account=a"
        );
    }

    #[tokio::test]
    async fn a_connection_delivers_hints_and_ends_when_the_relay_closes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
            for frame in [
                "pong",
                r#"{"type":"hint","account":"a","kind":"content","cursor":7}"#,
            ] {
                socket.send(Message::Text(frame.into())).await.unwrap();
            }
            socket.close(None).await.unwrap();
        });
        let url = format!("ws://{address}/");
        let mut connection = Connection::open(&url, "token").await.unwrap();
        let hint = connection.next().await.unwrap().unwrap();
        assert_eq!((hint.kind.as_str(), hint.cursor), ("content", Some(7)));
        assert!(matches!(connection.next().await, Ok(None) | Err(_)));
    }

    #[tokio::test]
    async fn a_tls_endpoint_has_a_usable_crypto_provider() {
        // Accepts and never answers: reaching the handshake is enough to
        // know that building the TLS configuration does not panic.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("wss://localhost:{}/", listener.local_addr().unwrap().port());
        let open = Connection::open(&url, "token");
        let result = tokio::time::timeout(Duration::from_millis(300), open).await;
        assert!(result.is_err() || result.is_ok_and(|r| r.is_err()));
    }
}
