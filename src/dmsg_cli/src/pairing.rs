use crate::daemon::Daemon;
use dmsg_types::Result;
use serde_json::Value;

impl Daemon {
    pub(crate) async fn device_request_preview(&self, token: &str) -> Result<Value> {
        let mut view =
            dmsg_client::approval_key::inspect_device_request(&self.hold(), token).await?;
        if view["route"] == "handoff" {
            view["qr"] = serde_json::json!(dmsg_client::qr::png_data(token).ok());
        }
        Ok(view)
    }
}
