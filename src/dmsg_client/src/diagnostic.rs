//! Privacy-safe failure diagnostics. Only allowlisted categories and HTTP
//! status numbers cross the logging boundary; error messages are never logs.

use dmsg_types::Error;
use std::error::Error as StdError;

/// The only `log` target the daemon records from libraries.
pub const TARGET: &str = "dmsg::diagnostic";

const SOURCES: &[&str] = &[
    "relay", "ic", "download", "storage", "hints", "mount", "client", "unknown",
];
const REASONS: &[&str] = &[
    "timeout",
    "connect",
    "connection_closed",
    "connection_reset",
    "connection_refused",
    "dns",
    "tls",
    "request",
    "body",
    "decode",
    "http",
    "remote_error",
    "io",
    "permission_denied",
    "not_found",
    "disk_full",
    "read_only",
    "busy",
    "corrupt",
    "constraint",
    "database",
    "rejected",
    "certificate",
    "certificate_stale",
    "response_too_large",
    "protocol",
    "heartbeat_timeout",
    "invalid_endpoint",
    "unavailable",
    "result_unknown",
    "integrity",
    "invalid_input",
    "locked",
    "refused",
];

/// Fixed categories describing a failure, with no caller or resource data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// A known subsystem, never an origin or file path.
    pub source: &'static str,
    /// An allowlisted reason, never an upstream message.
    pub reason: &'static str,
    /// An HTTP status between 100 and 599, when available.
    pub http_status: Option<u16>,
}

impl Diagnostic {
    /// Unknown labels collapse to fixed fallbacks.
    pub fn new(source: &str, reason: &str) -> Self {
        Self {
            source: SOURCES
                .iter()
                .copied()
                .find(|s| *s == source)
                .unwrap_or("unknown"),
            reason: REASONS
                .iter()
                .copied()
                .find(|r| *r == reason)
                .unwrap_or("unavailable"),
            http_status: None,
        }
    }

    /// An HTTP failure without its URL, headers or response body.
    pub fn http(source: &str, status: u16) -> Self {
        Self {
            http_status: (100..=599).contains(&status).then_some(status),
            ..Self::new(source, "http")
        }
    }

    /// Classify an I/O error without formatting its message.
    pub fn io(source: &str, error: &std::io::Error) -> Self {
        use std::io::ErrorKind;
        Self::new(
            source,
            match error.kind() {
                ErrorKind::TimedOut => "timeout",
                ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe => "connection_closed",
                ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => "connection_reset",
                ErrorKind::ConnectionRefused => "connection_refused",
                ErrorKind::PermissionDenied => "permission_denied",
                ErrorKind::NotFound => "not_found",
                ErrorKind::StorageFull => "disk_full",
                ErrorKind::ReadOnlyFilesystem => "read_only",
                _ => "io",
            },
        )
    }

    /// Keep transport causes before the protocol maps a lost mutation reply
    /// to `ExecutionUnknown`. The chain is inspected, never written out.
    pub fn request(source: &str, error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            return Self::new(source, "timeout");
        }
        if let Some(status) = error.status() {
            return Self::http(source, status.as_u16());
        }
        let mut cause = error.source();
        while let Some(error) = cause {
            if let Some(io) = error.downcast_ref::<std::io::Error>() {
                let diagnostic = Self::io(source, io);
                if diagnostic.reason != "io" {
                    return diagnostic;
                }
            }
            // Some TLS/DNS adapters erase their typed cause. These checks
            // select fixed labels; no substring is ever copied to output.
            let text = error.to_string().to_ascii_lowercase();
            if text.contains("certificate") || text.contains("tls") {
                return Self::new(source, "tls");
            }
            if text.contains("dns") || text.contains("failed to lookup") {
                return Self::new(source, "dns");
            }
            cause = error.source();
        }
        Self::new(
            source,
            if error.is_connect() {
                "connect"
            } else if error.is_body() {
                "body"
            } else if error.is_decode() {
                "decode"
            } else {
                "request"
            },
        )
    }

    /// Classify a returned application error. Detailed network failures are
    /// emitted at their source, before error-code conversion loses the cause.
    pub fn error(error: &Error) -> Self {
        match error {
            Error::Unavailable(message) => {
                if let Some(message) = message.strip_prefix("network: ") {
                    let (source, reason) = message
                        .split_once(' ')
                        .unwrap_or(("unknown", "unavailable"));
                    return Self::new(source, reason);
                }
                let source = ["relay", "ic", "storage", "mount"]
                    .into_iter()
                    .find(|s| {
                        message
                            .strip_prefix(s)
                            .is_some_and(|rest| rest.starts_with(':') || rest.starts_with(' '))
                    })
                    .unwrap_or("unknown");
                Self::new(
                    source,
                    if message == "corrupt record" {
                        "corrupt"
                    } else {
                        "unavailable"
                    },
                )
            }
            Error::ExecutionUnknown => Self::new("unknown", "result_unknown"),
            Error::IntegrityFailed => Self::new("client", "integrity"),
            Error::InvalidInput(_) => Self::new("client", "invalid_input"),
            Error::Locked => Self::new("client", "locked"),
            _ => Self::new("client", "refused"),
        }
    }

    /// Emit only this fixed schema. Other library log targets are ignored
    /// by the daemon, even when RUST_LOG requests verbose network logging.
    pub fn emit(self) {
        log::warn!(target: TARGET, "{} {} {}", self.source, self.reason, self.http_status.unwrap_or(0));
    }

    /// Strictly decode our log record, rejecting arbitrary messages and
    /// extra fields. Used again at the daemon's file-writing boundary.
    pub fn decode(record: &str) -> Option<Self> {
        let parts: Vec<_> = record.split_ascii_whitespace().collect();
        if parts.len() != 3 || !SOURCES.contains(&parts[0]) || !REASONS.contains(&parts[1]) {
            return None;
        }
        let status: u16 = parts[2].parse().ok()?;
        if status != 0 && !(100..=599).contains(&status) {
            return None;
        }
        Some(Self {
            http_status: (status != 0).then_some(status),
            ..Self::new(parts[0], parts[1])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn request_diagnostics_distinguish_timeout_and_refused_connection_without_urls() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let url = format!(
            "http://{}/PRIVATE?token=PRIVATE",
            listener.local_addr().unwrap()
        );
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_millis(50))
            .build()
            .unwrap();
        let timeout = client.get(&url).send().await.unwrap_err();
        assert_eq!(
            Diagnostic::request("relay", &timeout),
            Diagnostic::new("relay", "timeout")
        );
        drop(listener);
        let refused = client.get(&url).send().await.unwrap_err();
        let diagnostic = Diagnostic::request("relay", &refused);
        assert!(matches!(
            diagnostic.reason,
            "connection_refused" | "connect"
        ));
        assert!(!format!("{diagnostic:?}").contains("PRIVATE"));
    }

    #[test]
    fn errors_and_log_records_cannot_smuggle_private_text() {
        for private in [
            "https://host/account/SECRET?token=SECRET",
            "/Users/SECRET/file",
            "Bearer SECRET",
            "SECRET\nforged=value",
        ] {
            let error = Error::Unavailable(format!("relay: {private}"));
            assert_eq!(
                Diagnostic::error(&error),
                Diagnostic::new("relay", "unavailable")
            );
            assert_eq!(
                Diagnostic::decode(&format!("relay timeout 503 {private}")),
                None
            );
            assert_eq!(Diagnostic::decode(&format!("relay {private} 503")), None);
            assert_eq!(
                Diagnostic::new(private, private),
                Diagnostic::new("unknown", "unavailable")
            );
            let io = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, private);
            assert_eq!(
                Diagnostic::io("hints", &io),
                Diagnostic::new("hints", "connection_closed")
            );
        }
        assert_eq!(
            Diagnostic::decode("relay http 503"),
            Some(Diagnostic::http("relay", 503))
        );
        assert_eq!(Diagnostic::decode("relay http 65535"), None);
    }
}
