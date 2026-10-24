//! Owner control plane for local credential-use policies and history.
use crate::{
    daemon::Daemon,
    params::{number, optional, source, text},
};
use dmsg_types::{Error, Result};
use serde_json::{Value, json};

pub(crate) const METHODS: [&str; 9] = [
    "secrets/ref",
    "secrets/policies/list",
    "secrets/policies/save_request",
    "secrets/policies/disable",
    "secrets/policies/delete_request",
    "secrets/uses/get",
    "secrets/uses/cancel",
    "secrets/uses/list",
    "secrets/uses/mark_reviewed",
];

impl Daemon {
    pub(crate) async fn call_secrets(&self, method: &str, p: &Value) -> Result<Value> {
        let mut c = self.client().lock().await;
        match method {
            "secrets/ref" => c.secret_ref(text(p, "item_id")?, text(p, "field_id")?),
            "secrets/policies/list" => {
                let policies = c.secret_policies()?;
                let start = match p["after"].as_str() {
                    Some(id) => {
                        policies
                            .iter()
                            .position(|p| p.id == id)
                            .ok_or(Error::NotFound)?
                            + 1
                    }
                    None => 0,
                };
                let limit = p["limit"].as_u64().unwrap_or(64).clamp(1, 64) as usize;
                let page: Vec<_> = policies.into_iter().skip(start).take(limit).collect();
                Ok(json!({"policies":page,"next":page.last().map(|p|&p.id),"device_local":true}))
            }
            "secrets/policies/save_request" => c.secret_policy_request(
                source(p)?,
                optional(p, "id", text)?,
                optional(p, "base_version", number)?,
                Some(
                    serde_json::from_value(p["policy"].clone())
                        .map_err(|_| Error::InvalidInput("policy".into()))?,
                ),
                optional(p, "grant_to", text)?,
            ),
            "secrets/policies/disable" => c.secret_policy_disable(&text(p, "id")?),
            "secrets/policies/delete_request" => c.secret_policy_request(
                source(p)?,
                Some(text(p, "id")?),
                Some(number(p, "base_version")?),
                None,
                None,
            ),
            "secrets/uses/get" => c.secret_use_get(&text(p, "request_id")?, None),
            "secrets/uses/cancel" => c.secret_use_cancel(&text(p, "request_id")?, None),
            "secrets/uses/list" => {
                if p["kind"].as_str().is_some_and(|k| k != "secret_use") {
                    return Err(Error::InvalidInput("kind".into()));
                }
                c.secret_uses(
                    p["before"].as_str(),
                    p["actor"].as_str(),
                    p["limit"].as_u64().unwrap_or(50).min(100) as usize,
                )
            }
            "secrets/uses/mark_reviewed" => c.secret_use_reviewed(&text(p, "request_id")?),
            _ => Err(Error::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{home::Home, mcp_events::McpEvents};
    use dmsg_client::{
        testing::{TempDir, unlocked},
        vault::Changes,
    };

    #[tokio::test]
    async fn owner_configures_an_agent_requests_and_only_the_owner_can_review() {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0.join("workspace"), false, None);
        let actor = c.agent_create("test").unwrap();
        c.agent_credential(&actor.agent_id).unwrap();
        let actor = c.agent_find(&actor.agent_id).unwrap();
        let item=c.vault_create("api",&serde_json::from_value::<Changes>(json!({"title":"GitHub","set":[{"id":"credential","value":"fixture-secret-not-a-real-token"}]})).unwrap()).unwrap();
        let home = Home {
            dir: dir.0.join("home"),
        };
        let d = Daemon::new(
            "owner".into(),
            0,
            home.settings().unwrap(),
            c,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            McpEvents::new(&[]).unwrap(),
        );
        let reference = d
            .call_secrets(
                "secrets/ref",
                &json!({"item_id":item.id,"field_id":"credential"}),
            )
            .await
            .unwrap();
        let request=d.call_secrets("secrets/policies/save_request",&json!({"source":"gui","grant_to":actor.agent_id,"policy":{"schema_version":1,"name":"Repository","enabled":true,"adapter":"github.issues/1","credential":reference["reference"],"target":{"api_origin":"https://api.github.com","owner":"example","repo":"test"},"operations":["issue.create"]}})).await.unwrap();
        // Configuration is entirely local. Its decision is never exposed to MCP.
        d.under_lock(async |c| {
            c.approval_respond(
                request["id"].as_str().unwrap(),
                request["digest"].as_str().unwrap(),
                true,
                "gui",
                false,
            )
            .await
        })
        .await
        .unwrap();
        let listed = d
            .call_secrets("secrets/policies/list", &json!({}))
            .await
            .unwrap();
        let policy = listed["policies"][0]["id"].as_str().unwrap();
        let call = async |tool: &str, input: Value| {
            d.call_agent(&actor, "mcp/call", &json!({"tool":tool,"input":input}))
                .await
                .unwrap()
        };
        let public = call("dmsg_secret_policies", json!({})).await;
        assert!(!public.to_string().contains("credential"), "{public}");
        let input = json!({"policy_id":policy,"idempotency_key":"one","operation":"issue.create","arguments":{"title":"Full title","body":"<script>not markup</script>"}});
        let pending = call("dmsg_secret_use_request", input.clone()).await;
        assert_eq!(pending["result"]["state"], "awaiting_user", "{pending}");
        assert!(!pending.to_string().contains("fixture-secret"));
        let id = pending["result"]["request_id"].as_str().unwrap();
        let duplicate = call("dmsg_secret_use_request", input.clone()).await;
        assert_eq!(duplicate["result"]["request_id"], id);
        let mut forged = input;
        forged["actor"] = json!("another");
        assert_eq!(
            call("dmsg_secret_use_request", forged).await["error"]["code"],
            "INVALID_INPUT"
        );
        assert_eq!(
            call("dmsg_secret_use_cancel", json!({"request_id":id})).await["result"]["state"],
            "cancelled"
        );
        let use_view = d
            .call_secrets("secrets/uses/get", &json!({"request_id":id}))
            .await
            .unwrap();
        assert_eq!(
            use_view["approval"]["detail"]["arguments"]["body"],
            "<script>not markup</script>"
        );
        assert!(!use_view.to_string().contains("fixture-secret"));
        assert!(!use_view.to_string().contains("credential_digest"));
        assert_eq!(
            call("dmsg_vault_read", json!({"id":item.id,"secret":true})).await["error"]["code"],
            "FORBIDDEN"
        );
        d.client().lock().await.lock();
        let locked = d
            .call_secrets("secrets/uses/get", &json!({"request_id":id}))
            .await
            .unwrap();
        assert_eq!(locked.as_object().unwrap().len(), 2);
        assert_eq!(
            call("dmsg_secret_use_status", json!({"request_id":id})).await["error"]["code"],
            "LOCKED"
        );
    }
}
