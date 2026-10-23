//! MCP Events: the daemon fetches what the relay holds for
//! this device's callback endpoints, on the relay's `mcp-event` hint and
//! every minute while unlocked, and hands it to agents as the
//! `dmsg.relay.delivery` events of `dmsg mcp` or through the
//! `dmsg_events_*` tools. Locked, nothing is fetched: deliveries wait on
//! the relay for seven days.
//!
//! Generic upstream bindings only get `refresh_due` reminders. The separate
//! Mail watch manager owns Mail subscription creation and renewal.

use crate::daemon::Daemon;
use crate::params::text;
use dmsg_client::agents::AgentEntry;
use dmsg_client::mcp_events::{Filter, jwks_keys};
use dmsg_client::now_ms;
use dmsg_types::{Error, Hash, Result};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// The owner's methods this module serves.
pub(crate) const METHODS: [&str; 8] = [
    "mcp-events/create",
    "mcp-events/list",
    "mcp-events/rotate",
    "mcp-events/delete",
    "mcp-events/bind",
    "mcp-events/poll",
    "mcp-events/ack",
    "mcp-events/status",
];

/// Fetch this often while unlocked; a hint or a waiting reader fetches sooner.
const FETCH_MS: u64 = 60_000;
/// Fetch passes are at least this far apart.
const SPACING_MS: u64 = 2_000;
/// A reader waits at most this long for a delivery.
pub(crate) const MAX_WAIT_MS: u64 = 25_000;
/// An upstream's published `v1a` keys are read again after this long.
const JWKS_TTL_MS: u64 = 3_600_000;
const MAX_JWKS: usize = 64 * 1024;

/// The fetch loop's signals, its last report and the upstream keys.
pub(crate) struct McpEvents {
    wake: Notify,
    pub(crate) arrived: Notify,
    http: reqwest::Client,
    state: std::sync::Mutex<State>,
}

#[derive(Default)]
struct State {
    last: Option<Value>,
    rejected: u64,
    jwks: BTreeMap<String, (u64, Vec<Hash>)>,
}

impl McpEvents {
    /// `roots` replace the system roots in a local deployment, as for the
    /// agent connectors.
    pub(crate) fn new(roots: &[Vec<u8>]) -> Result<Self> {
        let roots = roots
            .iter()
            .map(|pem| {
                reqwest::Certificate::from_pem(pem).map_err(|_| dmsg_types::invalid("agent_ca"))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none());
        if !roots.is_empty() {
            http = http.tls_certs_only(roots);
        }
        Ok(Self {
            wake: Notify::new(),
            arrived: Notify::new(),
            http: http
                .build()
                .map_err(|e| Error::Unavailable(format!("http: {e}")))?,
            state: std::sync::Mutex::new(State::default()),
        })
    }

    /// Fetch soon, such as on a relay hint.
    pub(crate) fn wake(&self) {
        self.wake.notify_one();
    }

    /// The keys an origin publishes at `/.well-known/mcp-webhook-jwks.json`;
    /// none when it publishes none or cannot be read.
    async fn keys(&self, origin: &str, now: u64) -> Vec<Hash> {
        if let Some((at, keys)) = self.state.lock().expect("events state").jwks.get(origin)
            && now < at + JWKS_TTL_MS
        {
            return keys.clone();
        }
        let keys = match self
            .http
            .get(format!("{origin}/.well-known/mcp-webhook-jwks.json"))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => match response.bytes().await {
                Ok(bytes) if bytes.len() <= MAX_JWKS => serde_json::from_slice(&bytes)
                    .map(|jwks: Value| jwks_keys(&jwks))
                    .unwrap_or_default(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        self.state
            .lock()
            .expect("events state")
            .jwks
            .insert(origin.to_string(), (now, keys.clone()));
        keys
    }
}

/// Fetch for as long as the daemon runs.
pub(crate) async fn run(daemon: Arc<Daemon>) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(FETCH_MS)) => {}
            _ = daemon.mcp().wake.notified() => {}
        }
        if daemon.stopping() {
            continue;
        }
        daemon.fetch_events().await;
        tokio::time::sleep(Duration::from_millis(SPACING_MS)).await;
    }
}

/// A cursor: the decimal local sequence; absent or null is the start.
fn cursor(value: &Value) -> Result<u64> {
    match value {
        Value::Null => Ok(0),
        Value::Number(n) => n
            .as_u64()
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| Error::InvalidInput("cursor".into())),
        Value::String(s) if s.is_empty() => Ok(0),
        Value::String(s) => sequence(s, "cursor"),
        _ => Err(Error::InvalidInput("cursor".into())),
    }
}

/// SQLite sequences are signed integers. A required acknowledgement never
/// inherits the optional read cursor's missing/empty => start behavior.
fn sequence(text: &str, field: &str) -> Result<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::InvalidInput(field.into()));
    }
    text.parse::<u64>()
        .ok()
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or_else(|| Error::InvalidInput(field.into()))
}

fn input<T: DeserializeOwned>(value: &Value) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|err| Error::InvalidInput(err.to_string()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindEndpoint {
    endpoint_id: String,
    server: Option<String>,
    subscription_id: Option<String>,
    #[serde(default, deserialize_with = "read_instant")]
    refresh_before: Option<u64>,
    session: Option<String>,
}

fn read_instant<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<u64>, D::Error> {
    instant(&Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    endpoint_id: Option<String>,
    endpoint_ids: Option<Vec<String>>,
    session: Option<String>,
}

impl Selection {
    fn filter(self, agent: Option<&str>, mcp_session: Option<&str>) -> Result<Filter> {
        let mut endpoint_ids = self.endpoint_ids.unwrap_or_default();
        endpoint_ids.extend(self.endpoint_id);
        if endpoint_ids.len() > 16 || endpoint_ids.iter().any(|id| id.trim().is_empty()) {
            return Err(Error::InvalidInput("endpoint_ids".into()));
        }
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.trim().is_empty())
        {
            return Err(Error::InvalidInput("session".into()));
        }
        Ok(Filter {
            agent: agent.map(str::to_string),
            endpoint_ids,
            session: self.session,
            mcp_session: mcp_session.map(str::to_string),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AckEvents {
    through: String,
    endpoint_id: Option<String>,
    endpoint_ids: Option<Vec<String>>,
    session: Option<String>,
}

fn acknowledgement(
    value: &Value,
    agent: Option<&str>,
    mcp_session: Option<&str>,
) -> Result<(Filter, u64)> {
    let args: AckEvents = input(value)?;
    let through = sequence(&args.through, "through")?;
    let filter = Selection {
        endpoint_id: args.endpoint_id,
        endpoint_ids: args.endpoint_ids,
        session: args.session,
    }
    .filter(agent, mcp_session)?;
    Ok((filter, through))
}

fn binding(value: &Value) -> Result<BindEndpoint> {
    let args: BindEndpoint = input(value)?;
    for (field, value) in [
        ("endpoint_id", Some(args.endpoint_id.as_str())),
        ("server", args.server.as_deref()),
        ("subscription_id", args.subscription_id.as_deref()),
        ("session", args.session.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return Err(Error::InvalidInput(field.into()));
        }
    }
    Ok(args)
}

/// Unix milliseconds, or an RFC 3339 UTC time as MCP servers give
/// `refreshBefore`.
fn instant(value: &Value) -> Result<Option<u64>> {
    let invalid = || Error::InvalidInput("refresh_before".into());
    match value {
        Value::Null => Ok(None),
        Value::Number(n) => n.as_u64().map(Some).ok_or_else(invalid),
        Value::String(s) => parse_rfc3339(s).map(Some).ok_or_else(invalid),
        _ => Err(invalid()),
    }
}

/// RFC 3339 timestamps, including numeric offsets; reject pre-epoch values.
fn parse_rfc3339(text: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .and_then(|time| u64::try_from(time.timestamp_millis()).ok())
}

/// The endpoints an input names: `endpoint_id`, `endpoint_ids` or
/// `session`; without them, those of the caller's MCP session.
fn filter(agent: Option<&str>, mcp_session: Option<&str>, value: &Value) -> Result<Filter> {
    let selection: Selection = input(&json!({
        "endpoint_id": value["endpoint_id"],
        "endpoint_ids": value["endpoint_ids"],
        "session": value["session"],
    }))?;
    selection.filter(agent, mcp_session)
}

fn context(input: &Value) -> Result<BTreeMap<String, String>> {
    match &input["context"] {
        Value::Null => Ok(BTreeMap::new()),
        context => serde_json::from_value(context.clone())
            .map_err(|_| Error::InvalidInput("context: an object of strings".into())),
    }
}

impl Daemon {
    /// One fetch pass when unlocked: the bound upstreams' keys, then every
    /// endpoint's records, then the due refresh reminders.
    pub(crate) async fn fetch_events(&self) {
        let servers = {
            let client = self.client().lock().await;
            if client.session_state().unlocked_account().is_none() {
                return;
            }
            client.mcp_servers().unwrap_or_default()
        };
        let now = now_ms();
        let mut jwks = BTreeMap::new();
        for origin in servers {
            let keys = self.mcp().keys(&origin, now).await;
            jwks.insert(origin, keys);
        }
        // Each page waits on the relay without the client; one pass at a time.
        let report = {
            let _turn = self.turn("mcp-events").await;
            dmsg_client::mcp_events::fetch(&self.hold(), &jwks).await
        };
        let due = self
            .client()
            .lock()
            .await
            .mcp_refresh_due(now_ms())
            .unwrap_or_default();
        let stored = report.as_ref().map_or(0, |r| r.stored);
        self.note("mcp-events", &crate::daemon::outcome(&report));
        {
            let mut state = self.mcp().state.lock().expect("events state");
            if let Ok(report) = &report {
                state.rejected += report.rejected;
                if report.rejected > 0 {
                    // Deliveries whose v1a signature failed were dropped.
                    crate::logs::write("mcp-events/rejected", &[("count", &report.rejected)]);
                }
            }
            state.last = Some(match &report {
                Ok(report) => json!({"at": now, "report": report}),
                Err(error) => json!({"at": now, "error": dmsg_client::error_code(error)}),
            });
        }
        if stored + due > 0 {
            self.mail_push().receive.notify_one();
            self.mcp().arrived.notify_waiters();
            self.emit(
                "mcp-events/changed",
                json!({"stored": stored, "refresh_due": due}),
            );
        }
    }

    /// Records after `after`, waiting up to `wait_ms` for the first one.
    async fn events_next(
        &self,
        filter: &Filter,
        after: u64,
        limit: usize,
        wait_ms: u64,
    ) -> Result<Value> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(wait_ms.min(MAX_WAIT_MS));
        if wait_ms > 0 {
            self.mcp().wake();
        }
        loop {
            // Registered before reading, so no arrival in between is missed.
            let arrived = self.mcp().arrived.notified();
            tokio::pin!(arrived);
            arrived.as_mut().enable();
            let mut page = self
                .client()
                .lock()
                .await
                .mcp_events_read(filter, after, limit)?;
            let empty = page["events"].as_array().is_none_or(Vec::is_empty);
            if !empty
                || page["truncated"] == true
                || tokio::time::Instant::now() >= deadline
                || self.stopping()
            {
                page["nextPollMs"] = if page["hasMore"] == true { 0 } else { 5_000 }.into();
                return Ok(page);
            }
            tokio::select! {
                _ = arrived => {}
                _ = tokio::time::sleep_until(deadline) => {}
            }
        }
    }

    /// The keyless view: lock state and the last fetch.
    async fn events_status(&self) -> Result<Value> {
        let status = self.client().lock().await.status()?;
        let state = self.mcp().state.lock().expect("events state");
        Ok(json!({
            "state": status.state,
            "background_online": status.background_online,
            "fetching": status.state == "unlocked",
            "last_fetch": state.last,
            "rejected": state.rejected,
        }))
    }

    /// One of the owner's [`METHODS`].
    pub(crate) async fn call_mcp_events(&self, method: &str, params: &Value) -> Result<Value> {
        match method {
            "mcp-events/poll" => {
                return self
                    .events_next(
                        &filter(None, None, params)?,
                        cursor(&params["cursor"])?,
                        params["limit"].as_u64().unwrap_or(100) as usize,
                        params["wait_ms"].as_u64().unwrap_or(0),
                    )
                    .await;
            }
            "mcp-events/status" => return self.events_status().await,
            // The relay's view waits without the client.
            "mcp-events/list" => {
                return dmsg_client::mcp_events::endpoint_list(&self.hold(), None).await;
            }
            "mcp-events/bind" => {
                let args = binding(params)?;
                return self.client().lock().await.mcp_endpoint_bind(
                    None,
                    None,
                    &args.endpoint_id,
                    args.server,
                    args.subscription_id,
                    args.refresh_before,
                    args.session,
                );
            }
            "mcp-events/ack" => {
                let (filter, through) = acknowledgement(params, None, None)?;
                return self.client().lock().await.mcp_events_ack(&filter, through);
            }
            _ => {}
        }
        // Creating, rotating and deleting an endpoint still wait on the
        // relay under the client.
        let outcome = self
            .under_lock(async |client| match method {
                "mcp-events/create" => {
                    client
                        .mcp_endpoint_create(None, None, context(params)?)
                        .await
                }
                "mcp-events/rotate" => {
                    client
                        .mcp_endpoint_rotate(None, &text(params, "endpoint_id")?)
                        .await
                }
                "mcp-events/delete" => {
                    client
                        .mcp_endpoint_delete(None, &text(params, "endpoint_id")?)
                        .await
                }
                _ => Err(Error::NotFound),
            })
            .await;
        if method == "mcp-events/delete" {
            self.mcp().arrived.notify_waiters();
        }
        outcome
    }

    /// A `dmsg_events_*` tool for `agent`, which needs `events.endpoint`
    /// and reaches only the endpoints it created.
    pub(crate) async fn events_tool(
        &self,
        agent: &AgentEntry,
        mcp_session: Option<&str>,
        tool: &str,
        input: &Value,
    ) -> Result<Value> {
        let did = agent.agent_id.as_str();
        self.client()
            .lock()
            .await
            .capability_check(did, "events.endpoint")?;
        if tool == "dmsg_events_next" {
            return self
                .events_next(
                    &filter(Some(did), mcp_session, input)?,
                    cursor(&input["cursor"])?,
                    input["max"].as_u64().unwrap_or(100) as usize,
                    input["wait_ms"].as_u64().unwrap_or(0),
                )
                .await;
        }
        match tool {
            // The relay's view waits without the client.
            "dmsg_events_endpoint_list" => {
                return dmsg_client::mcp_events::endpoint_list(&self.hold(), Some(did)).await;
            }
            "dmsg_events_bind" => {
                let args = binding(input)?;
                return self.client().lock().await.mcp_endpoint_bind(
                    Some(did),
                    mcp_session,
                    &args.endpoint_id,
                    args.server,
                    args.subscription_id,
                    args.refresh_before,
                    args.session,
                );
            }
            "dmsg_events_ack" => {
                let (filter, through) = acknowledgement(input, Some(did), mcp_session)?;
                return self.client().lock().await.mcp_events_ack(&filter, through);
            }
            _ => {}
        }
        // Creating, rotating and deleting an endpoint still wait on the
        // relay under the client.
        let outcome = self
            .under_lock(async |client| match tool {
                "dmsg_events_endpoint_create" => {
                    client
                        .mcp_endpoint_create(Some(did), mcp_session, context(input)?)
                        .await
                }
                "dmsg_events_endpoint_rotate" => {
                    client
                        .mcp_endpoint_rotate(Some(did), &text(input, "endpoint_id")?)
                        .await
                }
                "dmsg_events_endpoint_delete" => {
                    client
                        .mcp_endpoint_delete(Some(did), &text(input, "endpoint_id")?)
                        .await
                }
                _ => Err(Error::NotFound),
            })
            .await;
        if tool == "dmsg_events_endpoint_delete" {
            self.mcp().arrived.notify_waiters();
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledgements_require_a_valid_explicit_position_and_selection() {
        for args in [
            json!({}),
            json!({"through": null}),
            json!({"through": ""}),
            json!({"through": 42}),
            json!({"through": true}),
            json!({"through": "-1"}),
            json!({"through": " 42"}),
            json!({"through": "+42"}),
            json!({"through": "42x"}),
            json!({"through": "9223372036854775808"}),
            json!({"cursor": "42"}),
            json!({"through": "42", "cursor": "42"}),
            json!({"through": "42", "endpoint_id": 7}),
            json!({"through": "42", "endpoint_ids": [1]}),
            json!({"through": "42", "session": false}),
        ] {
            assert!(
                matches!(
                    acknowledgement(&args, Some("agent"), Some("mcp")),
                    Err(Error::InvalidInput(_))
                ),
                "{args}"
            );
        }
        for position in ["0", "42", "9223372036854775807"] {
            let (filter, through) = acknowledgement(
                &json!({"through": position, "endpoint_id": "ep"}),
                Some("agent"),
                Some("mcp"),
            )
            .unwrap();
            assert_eq!(through, position.parse::<u64>().unwrap());
            assert_eq!(filter.endpoint_ids, ["ep"]);
            assert_eq!(filter.agent.as_deref(), Some("agent"));
        }
        assert!(cursor(&json!(u64::MAX)).is_err());
        assert!(cursor(&json!(u64::MAX.to_string())).is_err());
        assert_eq!(cursor(&Value::Null).unwrap(), 0);
    }

    #[test]
    fn bindings_reject_unknown_fields_and_wrong_types() {
        for args in [
            json!({"endpoint_id": "ep", "refreshBefore": 42}),
            json!({"endpoint_id": "ep", "server": 7}),
            json!({"endpoint_id": "ep", "subscription_id": {}}),
            json!({"endpoint_id": "ep", "refresh_before": false}),
            json!({"endpoint_id": "ep", "session": []}),
            json!({"endpoint_id": ""}),
            json!({"endpoint_id": "ep", "session": " "}),
        ] {
            assert!(binding(&args).is_err(), "{args}");
        }
        for value in [
            json!(1_790_000_000_123u64),
            json!("2026-09-21T14:13:20.123Z"),
            json!("2026-09-21T14:13:20.123+00:00"),
            json!("2026-09-21T22:13:20.123+08:00"),
        ] {
            assert_eq!(
                binding(&json!({"endpoint_id": "ep", "refresh_before": value}))
                    .unwrap()
                    .refresh_before,
                Some(1_790_000_000_123)
            );
        }
        assert_eq!(
            binding(&json!({"endpoint_id": "ep", "refresh_before": null}))
                .unwrap()
                .refresh_before,
            None
        );
        // Wrong selectors must never broaden an acknowledgement or a read.
        for selection in [
            json!({"endpoint_id": 7}),
            json!({"endpoint_ids": [null]}),
            json!({"session": false}),
            json!({"endpoint_id": ""}),
        ] {
            assert!(filter(Some("agent"), None, &selection).is_err());
        }
    }

    #[test]
    fn timestamps_validate_dates_offsets_and_the_unix_epoch() {
        for text in [
            "2026-02-29T00:00:00Z",
            "2026-02-30T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-01-01T-1:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2026-01-01T00:00:00+24:00",
            "2026-01-01T00:00:00",
            "1969-12-31T23:59:59Z",
        ] {
            assert_eq!(parse_rfc3339(text), None, "{text}");
        }
        for text in [
            "1970-01-01T00:00:00Z",
            "1970-01-01T01:00:00+01:00",
            "1969-12-31T23:00:00-01:00",
        ] {
            assert_eq!(parse_rfc3339(text), Some(0), "{text}");
        }
    }

    #[test]
    fn reads_refresh_before_and_cursors() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_rfc3339("2026-09-21T14:13:20.123Z"),
            Some(1_790_000_000_123)
        );
        assert_eq!(
            parse_rfc3339("2000-02-29T00:00:00.5Z"),
            Some(951_782_400_500)
        );
        assert_eq!(
            parse_rfc3339("2026-09-21T22:13:20.123+08:00"),
            Some(1_790_000_000_123)
        );
        assert_eq!(parse_rfc3339("2026-13-01T00:00:00Z"), None);
        assert_eq!(instant(&json!(42)).unwrap(), Some(42));
        assert_eq!(cursor(&json!("17")).unwrap(), 17);
        assert_eq!(cursor(&Value::Null).unwrap(), 0);
        assert!(cursor(&json!("x")).is_err());
        let f = filter(Some("did:agent:a"), Some("m"), &json!({"endpoint_id": "e"})).unwrap();
        assert_eq!(f.endpoint_ids, vec!["e".to_string()]);
        assert_eq!(f.mcp_session.as_deref(), Some("m"));
    }
}
