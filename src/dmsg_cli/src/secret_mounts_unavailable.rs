//! Explicit capability boundary on hosts without a native secret-files backend.
use crate::daemon::Daemon;
use dmsg_types::{Error, Result};
use serde_json::{Value, json};
use std::sync::Weak;

#[derive(Default)]
pub(crate) struct Manager;

impl Manager {
    pub(crate) fn bind(&self, _host: Weak<Daemon>) {}
}

pub(crate) fn configure_clock(_client: &mut dmsg_client::Client) {}

impl Daemon {
    pub(crate) async fn call_secret_mounts(&self, method: &str, _params: &Value) -> Result<Value> {
        if method == "secrets/mounts/list" {
            Ok(
                json!({"mounts":[],"platform":std::env::consts::OS,"supported":false,"backend_status":"unsupported_platform"}),
            )
        } else {
            Err(Error::UnsupportedProtocol)
        }
    }

    pub(crate) async fn stop_secret_mounts(&self) {}

    pub(crate) async fn check_secret_mounts(&self) {}
}
