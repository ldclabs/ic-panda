//! Native Apple integration uses fixed, signed service routes. App Attest
//! proves an installation, never the person's approval or a paid entitlement.
use super::*;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Serialize, uniffi::Record)]
pub struct AppAssertion {
    pub challenge_id: String,
    pub key_id: String,
    pub object: String,
}
#[derive(Clone, Serialize, uniffi::Record)]
pub struct PushRegistration {
    pub installation: String,
    pub token: String,
    pub environment: String,
}

impl MobileEngine {
    pub(crate) async fn mobile_read(&self, suffix: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                let account = h
                    .with(|c| c.session_state().account.ok_or(Error::AuthRequired))
                    .await?;
                h.get(&format!("/v1/accounts/{account}/mobile/{suffix}"))
                    .await
            })
        })
        .await
    }

    pub(crate) async fn mobile_post(
        &self,
        action: &'static str,
        payload: Value,
        request_id: Option<String>,
    ) -> Result<ObjectSnapshot> {
        let id = match request_id {
            Some(id) => dmsg_types::Hash::new(
                hex::decode(id)
                    .ok()
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| dmsg_types::invalid("request ID"))?,
            ),
            None => dmsg_types::Hash::new(dmsg_client::crypto::random_id()),
        };
        self.held_object(move |h| {
            Box::pin(async move {
                let account = h
                    .with(|c| c.session_state().account.ok_or(Error::AuthRequired))
                    .await?;
                h.post(
                    &format!("/v1/accounts/{account}/mobile/{action}"),
                    &format!("dmsg/mobile/{action}/v1"),
                    &payload,
                    &id,
                )
                .await
            })
        })
        .await
    }

    async fn app_challenge(
        &self,
        installation: String,
        key_id: String,
        purpose: &'static str,
        details: Value,
    ) -> Result<ObjectSnapshot> {
        let digest = hex::encode(*dmsg_protocol::digest("dmsg/mobile/action/v1", &details));
        self.mobile_post(
            "challenge",
            json!({"installation":installation,"key_id":key_id,"purpose":purpose,"digest":digest}),
            None,
        )
        .await
    }
}

#[uniffi::export]
impl MobileEngine {
    pub async fn purchase_preflight(
        &self,
        product: String,
        environment: String,
    ) -> Result<ObjectSnapshot> {
        let store_environment = match environment.as_str() {
            "Production" => dmsg_types::app_store::StoreEnvironment::Production,
            "Sandbox" => dmsg_types::app_store::StoreEnvironment::Sandbox,
            _ => return Err(Error::UnsupportedProtocol.into()),
        };
        let selected = product.clone();
        // The independent commerce and gateway reads can overlap. Neither writes
        // rights; transaction validation still happens after StoreKit succeeds.
        let (snapshot, gateway) = tokio::try_join!(
            self.held_object(move |h| {
                Box::pin(async move {
                    dmsg_client::commerce::app_store_purchase_check(
                        h,
                        &selected,
                        &store_environment,
                    )
                    .await
                })
            }),
            self.mobile_read("configuration".into())
        )?;
        let mut check: serde_json::Value =
            serde_json::from_str(&snapshot.json).map_err(|_| Error::IntegrityFailed)?;
        if check["eligible"] == true {
            let configured: serde_json::Value =
                serde_json::from_str(&gateway.json).map_err(|_| Error::IntegrityFailed)?;
            if !purchase_gateway_matches(&configured, &check, &product, &environment) {
                check["eligible"] = false.into();
                check["reason"] = "unavailable".into();
            }
        }
        Ok(ObjectSnapshot {
            generation: snapshot.generation,
            json: serde_json::to_string(&check).map_err(|_| Error::IntegrityFailed)?,
        })
    }

    pub async fn store_products(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::commerce::app_store_products(h).await }))
            .await
    }

    pub async fn attestation_status(&self, key_id: String) -> Result<ObjectSnapshot> {
        ensure_valid(key_id.len() <= 100, "key ID")?;
        self.mobile_read(format!(
            "attestation/{}",
            dmsg_client::codec::b64(key_id.as_bytes())
        ))
        .await
    }

    pub async fn attestation_challenge(
        &self,
        installation: String,
        key_id: String,
    ) -> Result<ObjectSnapshot> {
        self.app_challenge(installation, key_id, "attest", json!({}))
            .await
    }

    pub async fn register_attestation(
        &self,
        installation: String,
        key_id: String,
        challenge_id: String,
        object: String,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(object.len() <= 50000, "attestation")?;
        self.mobile_post("attest",json!({"installation":installation,"key_id":key_id,"challenge_id":challenge_id,"object":object}),None).await
    }

    pub async fn push_challenge(
        &self,
        registration: PushRegistration,
        key_id: String,
    ) -> Result<ObjectSnapshot> {
        let details = serde_json::to_value(&registration).map_err(|_| Error::IntegrityFailed)?;
        self.app_challenge(registration.installation, key_id, "push", details)
            .await
    }

    pub async fn register_push(
        &self,
        registration: PushRegistration,
        proof: AppAssertion,
        request_id: String,
    ) -> Result<ObjectSnapshot> {
        self.mobile_post(
            "push-register",
            json!({"details":registration,"proof":proof}),
            Some(request_id),
        )
        .await
    }

    pub async fn unregister_push(&self, installation: String) -> Result<ObjectSnapshot> {
        self.mobile_post(
            "push-unregister",
            json!({"installation":installation}),
            None,
        )
        .await
    }

    pub async fn purchase_challenge(
        &self,
        installation: String,
        jws: String,
        key_id: String,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(jws.len() <= 64000, "transaction")?;
        self.app_challenge(
            installation.clone(),
            key_id,
            "purchase",
            json!({"installation":installation,"jws":jws}),
        )
        .await
    }

    pub async fn submit_purchase(
        &self,
        installation: String,
        jws: String,
        proof: AppAssertion,
        request_id: String,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(jws.len() <= 64000, "transaction")?;
        self.mobile_post(
            "purchase",
            json!({"details":{"installation":installation,"jws":jws},"proof":proof}),
            Some(request_id),
        )
        .await
    }
}

/// A routing/preflight check only; Apple JWS verification remains server-side.
fn purchase_gateway_matches(
    configured: &Value,
    check: &Value,
    product: &str,
    environment: &str,
) -> bool {
    matches!(environment, "Production" | "Sandbox")
        && configured["available"] == true
        && (configured["environment"] == environment
            || (environment == "Sandbox"
                && configured["environment"] == "Production"
                && configured["accept_sandbox"] == true))
        && check["environment"] == environment
        && configured["products"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|p| p["id"] == product && p["kind"] == check["kind"])
        })
}

#[cfg(test)]
mod purchase_tests {
    use super::*;

    #[test]
    fn production_business_can_preflight_sandbox_only_when_both_services_allow_it() {
        let mut gateway = json!({"available":true,"environment":"Production","products":[{"id":"plus","kind":"subscription"}]});
        let check = json!({"environment":"Sandbox","kind":"subscription"});
        assert!(!purchase_gateway_matches(
            &gateway, &check, "plus", "Sandbox"
        ));
        gateway["accept_sandbox"] = true.into();
        assert!(purchase_gateway_matches(
            &gateway, &check, "plus", "Sandbox"
        ));
        assert!(!purchase_gateway_matches(
            &gateway, &check, "other", "Sandbox"
        ));
        assert!(!purchase_gateway_matches(
            &gateway,
            &check,
            "plus",
            "Production"
        ));
        assert!(!purchase_gateway_matches(&gateway, &check, "plus", "Xcode"));
        gateway["available"] = false.into();
        assert!(!purchase_gateway_matches(
            &gateway, &check, "plus", "Sandbox"
        ));
    }
}
