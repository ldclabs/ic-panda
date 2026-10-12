//! MCP Events (plan §6.11): the daemon fetches what the relay holds for
//! this device's callback endpoints, on the relay's `mcp-event` hint and
//! every minute while unlocked, and hands it to agents as the
//! `dmsg.relay.delivery` events of `dmsg mcp` or through the
//! `dmsg_events_*` tools. Locked, nothing is fetched: deliveries wait on
//! the relay for seven days.
//!
//! The daemon only relays: it never subscribes upstream or renews a
//! subscription, it reminds with `refresh_due` records.

use crate::daemon::Daemon;
use dmsg_client::agents::AgentEntry;
use dmsg_client::mcp_events::{jwks_keys, Filter};
use dmsg_client::now_ms;
use dmsg_types::{Error, Hash, Result};
use serde_json::{json, Value};
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
        if let Some((at, keys)) = self.state.lock().expect("events state").jwks.get(origin) {
            if now < at + JWKS_TTL_MS {
                return keys.clone();
            }
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

fn text(input: &Value, name: &str) -> Result<String> {
    input[name]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::InvalidInput(format!("missing {name}")))
}

/// A cursor: the decimal local sequence; absent or null is the start.
fn cursor(value: &Value) -> Result<u64> {
    match value {
        Value::Null => Ok(0),
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| Error::InvalidInput("cursor".into())),
        Value::String(s) if s.is_empty() => Ok(0),
        Value::String(s) => s.parse().map_err(|_| Error::InvalidInput("cursor".into())),
        _ => Err(Error::InvalidInput("cursor".into())),
    }
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

/// `YYYY-MM-DDTHH:MM:SS[.fff]Z` as Unix milliseconds.
fn parse_rfc3339(text: &str) -> Option<u64> {
    let text = text.strip_suffix('Z')?;
    let (date, time) = text.split_once('T')?;
    let mut date = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (date.next()??, date.next()??, date.next()??);
    let (time, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut time = time.splitn(3, ':').map(|p| p.parse::<i64>().ok());
    let (hour, minute, second) = (time.next()??, time.next()??, time.next()??);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let millis = match fraction.len() {
        0 => 0,
        _ => {
            fraction.get(..3.min(fraction.len()))?.parse::<i64>().ok()?
                * 10i64.pow(3 - 3.min(fraction.len()) as u32)
        }
    };
    // Days since 1970-01-01 from the civil date (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let ms = ((days * 86_400 + hour * 3600 + minute * 60 + second) * 1000) + millis;
    u64::try_from(ms).ok()
}

/// The endpoints an input names: `endpoint_id`, `endpoint_ids` or
/// `session`; without them, those of the caller's MCP session.
fn filter(agent: Option<&str>, mcp_session: Option<&str>, input: &Value) -> Result<Filter> {
    let mut endpoint_ids: Vec<String> = match &input["endpoint_ids"] {
        Value::Null => Vec::new(),
        ids => serde_json::from_value(ids.clone())
            .map_err(|_| Error::InvalidInput("endpoint_ids".into()))?,
    };
    if let Some(id) = input["endpoint_id"].as_str() {
        endpoint_ids.push(id.to_string());
    }
    if endpoint_ids.len() > 16 {
        return Err(Error::InvalidInput("endpoint_ids".into()));
    }
    Ok(Filter {
        agent: agent.map(str::to_string),
        endpoint_ids,
        session: input["session"].as_str().map(str::to_string),
        mcp_session: mcp_session.map(str::to_string),
    })
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
            if !client.status().is_ok_and(|s| s.state == "unlocked") {
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
        let mut client = self.client().lock().await;
        let report = client.mcp_events_fetch(&jwks).await;
        let due = client.mcp_refresh_due(now_ms()).unwrap_or_default();
        drop(client);
        let stored = report.as_ref().map_or(0, |r| r.stored);
        {
            let mut state = self.mcp().state.lock().expect("events state");
            if let Ok(report) = &report {
                state.rejected += report.rejected;
                if report.rejected > 0 {
                    eprintln!(
                        "dmsg: dropped {} MCP event deliveries whose v1a signature failed",
                        report.rejected
                    );
                }
            }
            state.last = Some(match &report {
                Ok(report) => json!({"at": now, "report": report}),
                Err(error) => json!({"at": now, "error": dmsg_client::error_code(error)}),
            });
        }
        if stored + due > 0 {
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
            if !empty || tokio::time::Instant::now() >= deadline || self.stopping() {
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
                    .await
            }
            "mcp-events/status" => return self.events_status().await,
            _ => {}
        }
        let mut client = self.client().lock().await;
        match method {
            "mcp-events/create" => {
                client
                    .mcp_endpoint_create(None, None, context(params)?)
                    .await
            }
            "mcp-events/list" => client.mcp_endpoint_list(None).await,
            "mcp-events/rotate" => {
                client
                    .mcp_endpoint_rotate(None, &text(params, "endpoint_id")?)
                    .await
            }
            "mcp-events/delete" => {
                let deleted = client
                    .mcp_endpoint_delete(None, &text(params, "endpoint_id")?)
                    .await;
                self.mcp().arrived.notify_waiters();
                deleted
            }
            "mcp-events/bind" => client.mcp_endpoint_bind(
                None,
                None,
                &text(params, "endpoint_id")?,
                params["server"].as_str().map(str::to_string),
                params["subscription_id"].as_str().map(str::to_string),
                instant(&params["refresh_before"])?,
                params["session"].as_str().map(str::to_string),
            ),
            "mcp-events/ack" => {
                client.mcp_events_ack(&filter(None, None, params)?, cursor(&params["through"])?)
            }
            _ => Err(Error::NotFound),
        }
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
        let mut client = self.client().lock().await;
        match tool {
            "dmsg_events_endpoint_create" => {
                client
                    .mcp_endpoint_create(Some(did), mcp_session, context(input)?)
                    .await
            }
            "dmsg_events_endpoint_list" => client.mcp_endpoint_list(Some(did)).await,
            "dmsg_events_endpoint_rotate" => {
                client
                    .mcp_endpoint_rotate(Some(did), &text(input, "endpoint_id")?)
                    .await
            }
            "dmsg_events_endpoint_delete" => {
                let deleted = client
                    .mcp_endpoint_delete(Some(did), &text(input, "endpoint_id")?)
                    .await;
                self.mcp().arrived.notify_waiters();
                deleted
            }
            "dmsg_events_bind" => client.mcp_endpoint_bind(
                Some(did),
                mcp_session,
                &text(input, "endpoint_id")?,
                input["server"].as_str().map(str::to_string),
                input["subscription_id"].as_str().map(str::to_string),
                instant(&input["refresh_before"])?,
                input["session"].as_str().map(str::to_string),
            ),
            "dmsg_events_ack" => client.mcp_events_ack(
                &filter(Some(did), mcp_session, input)?,
                cursor(&input["through"])?,
            ),
            _ => Err(Error::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(parse_rfc3339("2026-09-21T14:13:20+08:00"), None);
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
