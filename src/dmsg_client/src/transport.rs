//! Bounded recovery for safe reads. Mutations are sent once; their owners
//! reconcile uncertain results using the original operation journal.

use crate::diagnostic::Diagnostic;
use bytes::Bytes;
use futures_timer::Delay;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub attempts: usize,
    pub attempt: Duration,
    pub total: Duration,
    pub backoff: Duration,
}

pub(crate) const READ: Policy = Policy {
    attempts: 3,
    attempt: Duration::from_secs(5),
    total: Duration::from_secs(12),
    backoff: Duration::from_millis(250),
};
pub(crate) const WRITE: Policy = Policy {
    attempts: 1,
    attempt: Duration::from_secs(30),
    total: Duration::from_secs(30),
    backoff: Duration::ZERO,
};

#[derive(Debug)]
pub(crate) enum Failure {
    Request(reqwest::Error),
    TooLarge,
}

pub(crate) fn transient_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

pub(crate) fn transient(error: &reqwest::Error) -> bool {
    matches!(
        Diagnostic::request("unknown", error).reason,
        "timeout"
            | "connect"
            | "connection_closed"
            | "connection_reset"
            | "connection_refused"
            | "dns"
            | "request"
            | "body"
    ) && !error.is_builder()
}

pub(crate) fn unavailable(source: &str, reason: &str) -> dmsg_types::Error {
    let safe = Diagnostic::new(source, reason);
    dmsg_types::Error::Unavailable(format!("network: {} {}", safe.source, safe.reason))
}

/// Disable reqwest's own protocol retries so this is the only retry budget.
pub(crate) fn client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(3))
        .timeout(WRITE.total)
        .build()
}

/// Reads include the body in their timeout and retry budget. Byte limits
/// and certificate/protocol failures never trigger a retry.
pub(crate) async fn send(
    client: &reqwest::Client,
    request: reqwest::Request,
    source: &str,
    policy: Policy,
    limit: usize,
) -> Result<http::Response<Bytes>, Failure> {
    let started = Instant::now();
    for attempt in 0..policy.attempts {
        let mut request = request.try_clone().expect("buffered HTTP request");
        *request.timeout_mut() = Some(
            policy
                .attempt
                .min(policy.total.saturating_sub(started.elapsed())),
        );
        let result = exchange(client, request, limit).await;
        let retry = match &result {
            Err(Failure::Request(error)) => {
                Diagnostic::request(source, error).emit();
                transient(error)
            }
            Err(Failure::TooLarge) => {
                Diagnostic::new(source, "response_too_large").emit();
                false
            }
            Ok(response) if !response.status().is_success() => {
                Diagnostic::http(source, response.status().as_u16()).emit();
                transient_status(response.status().as_u16())
            }
            Ok(_) => false,
        };
        if !retry || attempt + 1 == policy.attempts {
            return result;
        }
        let jitter = if policy.backoff.is_zero() {
            Duration::ZERO
        } else {
            Duration::from_millis(u64::from(crate::crypto::random_id()[0]) / 2)
        };
        let pause = policy.backoff * (1 << attempt) + jitter;
        if started.elapsed() + pause >= policy.total {
            return result;
        }
        Delay::new(pause).await;
    }
    unreachable!("a transport policy always attempts at least once")
}

async fn exchange(
    client: &reqwest::Client,
    request: reqwest::Request,
    limit: usize,
) -> Result<http::Response<Bytes>, Failure> {
    let mut response = client.execute(request).await.map_err(Failure::Request)?;
    let mut result = http::Response::new(Bytes::new());
    *result.status_mut() = response.status();
    *result.headers_mut() = response.headers().clone();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(Failure::Request)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(Failure::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    *result.body_mut() = Bytes::from(bytes);
    Ok(result)
}

/// IC queries and certificate/status reads are safe POSTs. Ingress calls
/// are deliberately excluded, irrespective of their HTTP failure code.
pub(crate) fn ic_read(method: &http::Method, path: &str) -> bool {
    method == http::Method::GET
        || (method == http::Method::POST
            && path.starts_with("/api/")
            && (path.ends_with("/query") || path.ends_with("/read_state")))
}

#[derive(Debug)]
pub(crate) struct IcTransport(pub reqwest::Client);

#[async_trait::async_trait]
impl ic_agent::agent::HttpService for IcTransport {
    async fn call<'a>(
        &'a self,
        build: &'a (dyn Fn() -> Result<http::Request<Bytes>, ic_agent::AgentError> + Send + Sync),
        _retries: usize,
        limit: Option<usize>,
    ) -> Result<http::Response<Bytes>, ic_agent::AgentError> {
        use ic_agent::{AgentError, agent::agent_error::TransportError};
        let request = build()?;
        let policy = if ic_read(request.method(), request.uri().path()) {
            READ
        } else {
            WRITE
        };
        let (parts, body) = request.into_parts();
        let request = http::Request::from_parts(parts, reqwest::Body::from(body))
            .try_into()
            .map_err(|e: reqwest::Error| AgentError::TransportError(TransportError::Reqwest(e)))?;
        send(
            &self.0,
            request,
            "ic",
            policy,
            limit.unwrap_or(4 * 1024 * 1024),
        )
        .await
        .map_err(|e| match e {
            Failure::Request(e) => AgentError::TransportError(TransportError::Reqwest(e)),
            Failure::TooLarge => AgentError::ResponseSizeExceededLimit(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn server(
        replies: Vec<Option<&'static str>>,
    ) -> (String, tokio::task::JoinHandle<Vec<Vec<u8>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for reply in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0; 4096];
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let size = headers
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .map_or(0, |n| n.trim().parse::<usize>().unwrap());
                        if bytes.len() >= end + 4 + size {
                            break;
                        }
                    }
                }
                requests.push(bytes);
                if let Some(reply) = reply {
                    socket.write_all(reply.as_bytes()).await.unwrap();
                }
            }
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err(),
                "unexpected extra submission"
            );
            requests
        });
        (origin, task)
    }

    const OK: &str = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";
    const BUSY: &str = "HTTP/1.1 503 Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";

    #[tokio::test]
    async fn safe_reads_recover_and_ingress_is_sent_once_even_on_503_or_lost_reply() {
        use ic_agent::agent::HttpService;
        for (path, replies) in [
            (
                "/api/v3/canister/aaaaa-aa/query",
                vec![Some(BUSY), None, Some(OK)],
            ),
            ("/api/v3/canister/aaaaa-aa/read_state", vec![None, Some(OK)]),
            ("/api/v4/canister/aaaaa-aa/call", vec![Some(BUSY)]),
            ("/api/v4/canister/aaaaa-aa/call", vec![None]),
        ] {
            let expected = replies.len();
            let (origin, server) = server(replies).await;
            let transport = IcTransport(client().unwrap());
            let build = || {
                Ok(http::Request::post(format!("{origin}{path}"))
                    .body(Bytes::from_static(b"same signed command"))
                    .unwrap())
            };
            let result = transport.call(&build, 99, Some(1024)).await;
            if path.ends_with("/query") || path.ends_with("/read_state") {
                assert_eq!(result.unwrap().body(), "ok");
            }
            let requests = server.await.unwrap();
            assert_eq!(requests.len(), expected);
            for pair in requests.windows(2) {
                assert_eq!(
                    pair[0], pair[1],
                    "a retry must not change the signed request"
                );
            }
        }
    }

    #[tokio::test]
    async fn permanent_http_and_oversize_responses_are_not_retried() {
        for (reply, limit) in [
            (
                "HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                10,
            ),
            (OK, 1),
        ] {
            let (origin, server) = server(vec![Some(reply)]).await;
            let client = client().unwrap();
            let result = send(
                &client,
                client.get(origin).build().unwrap(),
                "relay",
                READ,
                limit,
            )
            .await;
            if limit == 1 {
                assert!(matches!(result, Err(Failure::TooLarge)));
            } else {
                assert_eq!(result.unwrap().status(), 403);
            }
            assert_eq!(server.await.unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn response_body_stalls_obey_the_total_budget() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut bytes = [0; 4096];
                    assert!(socket.read(&mut bytes).await.unwrap() > 0);
                    let _ = socket
                        .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\na")
                        .await;
                    tokio::time::sleep(Duration::from_millis(500)).await;
                });
            }
        });
        let client = client().unwrap();
        let policy = Policy {
            attempts: 3,
            attempt: Duration::from_millis(50),
            total: Duration::from_millis(120),
            backoff: Duration::ZERO,
        };
        let started = Instant::now();
        let result = send(
            &client,
            client.get(origin).build().unwrap(),
            "relay",
            policy,
            10,
        )
        .await;
        assert!(matches!(result, Err(Failure::Request(e)) if e.is_timeout()));
        assert!(started.elapsed() < Duration::from_secs(1));
        server.abort();
    }

    #[tokio::test]
    async fn an_ic_update_with_no_reply_remains_unknown_and_is_not_resubmitted() {
        let (origin, server) = server(vec![None]).await;
        let mut config = crate::testing::config(false);
        config.ic_host = origin;
        let ic = crate::ic::Ic::anonymous(&config).await.unwrap();
        let result: dmsg_types::Result<()> = ic
            .update(candid::Principal::anonymous(), "mutate", ())
            .await;
        assert_eq!(result, Err(dmsg_types::Error::ExecutionUnknown));
        assert_eq!(server.await.unwrap().len(), 1);
    }
}
