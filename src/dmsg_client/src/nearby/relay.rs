//! Ephemeral blind relay (`dmsg-nearby-relay/1`) creation, cancellation and
//! binary-frame WebSocket transport (`tokio_tungstenite`).

use super::offer::bearer_token;
use super::wire::FrameTransport;
use super::{RELAY_JOIN_TTL_MS, RELAY_MAX_BYTES};
use crate::codec::sha256;
use crate::crypto::random_id;
use crate::hold::Hold;
use crate::now_ms;
use dmsg_types::{Error, Hash, Result, ensure};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_bytes::Bytes;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use zeroize::Zeroizing;

const CREATE_ACTION: &str = "dmsg/nearby-relay/create/v1";
const CANCEL_ACTION: &str = "dmsg/nearby-relay/cancel/v1";

/// An active ephemeral relay room created by a logged-in receiver.
pub struct RelayRoom {
    /// Ephemeral 16-byte room identifier.
    pub rid: [u8; 16],
    /// One-time 16-byte receiver WebSocket bearer token.
    pub recv_token: Zeroizing<[u8; 16]>,
    /// One-time 16-byte sender WebSocket bearer token (embedded in `NearbyOffer`).
    pub send_token: Zeroizing<[u8; 16]>,
    /// Join deadline in Unix milliseconds.
    pub join_expires_at: u64,
    /// Server-reported transfer cap in bytes (at most `RELAY_MAX_BYTES`).
    pub max_bytes: u64,
    /// Client's configured relay origin (e.g. `https://api.dmsg.net`).
    pub origin: String,
    /// Deployment environment (`local`, `staging`, `production`).
    pub env: String,
}

fn random_16() -> [u8; 16] {
    let mut out = [0u8; 16];
    getrandom::fill(&mut out).expect("operating system random source");
    out
}

/// WebSocket URL for `rid` on the client's configured `origin`.
pub fn ws_url(origin: &str, rid: &[u8; 16]) -> String {
    let base = origin
        .trim_end_matches('/')
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    format!("{base}/v1/nearby-relays/{}/ws", hex::encode(rid))
}

/// Create a short-lived blind relay room on the account's configured relay.
///
/// Standalone and offline workspaces skip relay creation and return `Error::Unavailable`.
pub async fn create_room<H: Hold>(h: &H) -> Result<RelayRoom> {
    let rid = random_16();
    let recv_token = Zeroizing::new(random_16());
    let send_token = Zeroizing::new(random_16());
    let recv_hash = sha256(recv_token.as_slice());
    let send_hash = sha256(send_token.as_slice());
    let join_expires_at = now_ms() + RELAY_JOIN_TTL_MS;
    let request_id = Hash::new(random_id());

    #[derive(Serialize)]
    struct CreatePayload<'a> {
        rid: &'a Bytes,
        recv_token_hash: &'a Hash,
        send_token_hash: &'a Hash,
        join_expires_at: u64,
    }

    let (relay, (origin, env, prepared)) = h
        .signed(|c| {
            if c.offline_mode() {
                return Err(Error::Unavailable(
                    "nearby relay unavailable offline".into(),
                ));
            }
            let account = c.account()?.id.to_string();
            let path = format!("/v1/accounts/{account}/nearby-relays");
            let origin = c.config.relay.clone();
            let env = c.workspace.meta.environment.clone();
            let payload = CreatePayload {
                rid: Bytes::new(&rid),
                recv_token_hash: &recv_hash,
                send_token_hash: &send_hash,
                join_expires_at,
            };
            let req = c.signed_post(&path, CREATE_ACTION, &payload, &request_id)?;
            Ok((origin, env, req))
        })
        .await?;

    let reply = relay.send(prepared).await?;
    let max_bytes = reply["max_bytes"]
        .as_u64()
        .unwrap_or(RELAY_MAX_BYTES)
        .min(RELAY_MAX_BYTES);
    Ok(RelayRoom {
        rid,
        recv_token,
        send_token,
        join_expires_at,
        max_bytes,
        origin,
        env,
    })
}

/// Best-effort cancel of a previously created relay room.
pub async fn cancel_room<H: Hold>(h: &H, rid: [u8; 16]) -> Result<()> {
    let request_id = Hash::new(random_id());

    #[derive(Serialize)]
    struct CancelPayload<'a> {
        rid: &'a Bytes,
    }

    let (relay, prepared) = h
        .signed(|c| {
            if c.offline_mode() {
                return Err(Error::Unavailable(
                    "nearby relay unavailable offline".into(),
                ));
            }
            let account = c.account()?.id.to_string();
            let path = format!(
                "/v1/accounts/{account}/nearby-relays/{}/cancel",
                hex::encode(rid)
            );
            let payload = CancelPayload {
                rid: Bytes::new(&rid),
            };
            let req = c.signed_post(&path, CANCEL_ACTION, &payload, &request_id)?;
            Ok(req)
        })
        .await?;

    let _ = relay.send(prepared).await?;
    Ok(())
}

/// WebSocket binary-frame transport over `GET /v1/nearby-relays/:rid/ws`.
pub struct WsFrameTransport {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl WsFrameTransport {
    /// Connect to `ws_url(origin, rid)` authenticating via `Authorization: Bearer <token>`.
    pub async fn connect(origin: &str, rid: &[u8; 16], token: &[u8; 16]) -> Result<Self> {
        let url = ws_url(origin, rid);
        let mut request = url
            .into_client_request()
            .map_err(|_| Error::InvalidInput("nearby relay url".into()))?;
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", bearer_token(token))
                .parse()
                .map_err(|_| Error::InvalidInput("nearby relay bearer".into()))?,
        );
        let (socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| Error::Unavailable(format!("nearby relay connect: {e}")))?;
        Ok(Self { socket })
    }
}

impl FrameTransport for WsFrameTransport {
    async fn send_frame(&mut self, bytes: &[u8]) -> Result<()> {
        self.socket
            .send(Message::Binary(bytes.to_vec().into()))
            .await
            .map_err(|e| Error::Unavailable(format!("nearby relay send: {e}")))
    }

    async fn recv_frame(&mut self, max: usize) -> Result<Vec<u8>> {
        loop {
            let Some(msg) = self.socket.next().await else {
                return Err(Error::Unavailable("nearby relay closed".into()));
            };
            let msg = msg.map_err(|e| Error::Unavailable(format!("nearby relay recv: {e}")))?;
            match msg {
                Message::Binary(data) => {
                    ensure(!data.is_empty() && data.len() <= max, Error::QuotaExceeded)?;
                    return Ok(data.to_vec());
                }
                Message::Ping(payload) => {
                    let _ = self.socket.send(Message::Pong(payload)).await;
                }
                Message::Pong(_) => continue,
                Message::Close(_) => {
                    return Err(Error::Unavailable("nearby relay closed".into()));
                }
                _ => return Err(Error::IntegrityFailed),
            }
        }
    }
}
