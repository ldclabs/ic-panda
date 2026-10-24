//! The fixed GitHub.com Issues write adapter. One send, no redirects or retries.

use super::{Target, failure};
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use zeroize::Zeroizing;

/// The complete set of accepted issue arguments.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IssueInput {
    /// The full title, shown without Markdown interpretation.
    pub title: String,
    /// Full optional body, shown before approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl IssueInput {
    /// Check sizes before creating an approval.
    pub fn validate(&self) -> Result<()> {
        ensure_valid(
            !self.title.trim().is_empty()
                && self.title.chars().count() <= 256
                && !self.title.chars().any(char::is_control)
                && self
                    .body
                    .as_ref()
                    .is_none_or(|b| b.len() <= 32 * 1024 && !b.contains('\0')),
            "issue arguments",
        )
    }
}

/// Only these business fields can reach MCP.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueResult {
    /// Created issue number.
    pub number: u64,
    /// A reconstructed, checked GitHub link, never arbitrary upstream content.
    pub html_url: String,
}

/// Validate path segments without accepting URL syntax or dot traversal.
pub fn validate_target(target: &Target) -> Result<()> {
    let segment = |s: &str, max: usize| {
        !s.is_empty()
            && s.len() <= max
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    ensure_valid(
        target.api_origin == "https://api.github.com"
            && segment(&target.owner, 39)
            && segment(&target.repo, 100),
        "GitHub target",
    )
}

/// Make the production transport; not configurable through RPC or environment.
pub(crate) fn transport() -> Result<reqwest::Client> {
    transport_builder()
        .build()
        .map_err(|_| Error::Unavailable("GitHub transport".into()))
}

pub(crate) fn transport_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
}

/// Assemble the request, ensuring the header is marked sensitive to Debug.
pub(crate) fn request(
    http: &reqwest::Client,
    target: &Target,
    input: &IssueInput,
    token: &str,
) -> Result<reqwest::Request> {
    validate_target(target)?;
    input.validate()?;
    ensure_valid(
        !token.is_empty()
            && token.len() <= 8192
            && token.bytes().all(|b| (0x21..=0x7e).contains(&b)),
        "credential format",
    )?;
    let header = Zeroizing::new(format!("Bearer {token}"));
    let mut authorization = reqwest::header::HeaderValue::from_str(&header)
        .map_err(|_| failure("SECRET_REFERENCE_INVALID"))?;
    authorization.set_sensitive(true);
    http.post(format!(
        "https://api.github.com/repos/{}/{}/issues",
        target.owner, target.repo
    ))
    .header(reqwest::header::AUTHORIZATION, authorization)
    .header(reqwest::header::ACCEPT, "application/vnd.github+json")
    .header(reqwest::header::USER_AGENT, "dMsg-secrets/1")
    .header("X-GitHub-Api-Version", "2026-03-10")
    .json(input)
    .build()
    .map_err(|_| failure("SECRET_REFERENCE_INVALID"))
}

pub(crate) fn response(
    status: u16,
    bytes: &[u8],
    target: &Target,
    token: &str,
) -> Result<IssueResult> {
    // A redirect, server failure, malformed success or missing response can
    // never prove that a POST did not create an issue.
    if matches!(status, 400 | 401 | 403 | 404 | 405 | 410 | 422 | 429) {
        return Err(Error::Forbidden);
    }
    ensure(status == 201, Error::ExecutionUnknown)?;
    ensure(
        !bytes
            .windows(token.len().max(1))
            .any(|w| w == token.as_bytes()),
        failure("SECRET_OUTPUT_BLOCKED"),
    )?;
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| Error::ExecutionUnknown)?;
    let number = value["number"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or(Error::ExecutionUnknown)?;
    let html_url = format!(
        "https://github.com/{}/{}/issues/{number}",
        target.owner, target.repo
    );
    ensure(
        value["html_url"].as_str() == Some(&html_url),
        Error::ExecutionUnknown,
    )?;
    ensure(!html_url.contains(token), failure("SECRET_OUTPUT_BLOCKED"))?;
    Ok(IssueResult { number, html_url })
}

pub(crate) async fn send(
    http: reqwest::Client,
    request: reqwest::Request,
    target: &Target,
    token: &str,
) -> Result<IssueResult> {
    let mut reply = http
        .execute(request)
        .await
        .map_err(|_| Error::ExecutionUnknown)?;
    let status = reply.status().as_u16();
    if matches!(status, 400 | 401 | 403 | 404 | 405 | 410 | 422 | 429) {
        return Err(Error::Forbidden);
    }
    ensure(
        reply.content_length().is_none_or(|n| n <= 1024 * 1024),
        Error::ExecutionUnknown,
    )?;
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = reply.chunk().await.map_err(|_| Error::ExecutionUnknown)? {
        ensure(
            bytes.len() + chunk.len() <= 1024 * 1024,
            Error::ExecutionUnknown,
        )?;
        bytes.extend_from_slice(&chunk);
    }
    response(status, &bytes, target, token)
}
