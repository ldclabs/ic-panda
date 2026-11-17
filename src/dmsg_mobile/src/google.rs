//! Android uses its own proof provider, never an Apple assertion or client verdict.
use super::*;
use serde::Serialize;
use serde_json::json;

#[derive(Clone, Serialize, uniffi::Record)]
pub struct AndroidPushRegistration {
    pub installation: String,
    pub token: String,
    pub project_id: String,
}

#[derive(Clone, Serialize, uniffi::Record)]
pub struct AndroidIntegrityProof {
    pub challenge_id: String,
    pub token: String,
}

#[derive(Clone, Serialize, uniffi::Record)]
pub struct AndroidPurchase {
    pub installation: String,
    pub token: String,
    pub kind: String,
    pub product_id: String,
}

#[derive(Clone, Serialize, uniffi::Record)]
pub struct AndroidPurchasePreparation {
    pub installation: String,
    pub product_id: String,
    pub option_id: String,
    pub offer_id: Option<String>,
    pub kind: String,
    pub base_contract_id: Option<String>,
}

fn validate_preparation(value: &AndroidPurchasePreparation) -> Result<()> {
    let hash = |v: &str| {
        v.len() == 64
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    let id = |v: &str| {
        !v.is_empty()
            && v.len() <= 200
            && v.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    };
    ensure_valid(
        hash(&value.installation)
            && id(&value.product_id)
            && id(&value.option_id)
            && value.offer_id.as_ref().is_none_or(|v| id(v)),
        "purchase selection",
    )?;
    ensure_valid(
        match value.kind.as_str() {
            "subscription" => value.base_contract_id.is_none(),
            "storage" => value.base_contract_id.as_ref().is_some_and(|v| hash(v)),
            _ => false,
        },
        "purchase base",
    )?;
    Ok(())
}

fn validate_purchase(value: &AndroidPurchase) -> Result<()> {
    ensure_valid(
        value.installation.len() == 64
            && value
                .installation
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "installation",
    )?;
    ensure_valid(
        matches!(value.kind.as_str(), "subscription" | "storage")
            && !value.token.is_empty()
            && value.token.len() <= 4096
            && !value.token.chars().any(char::is_whitespace),
        "purchase",
    )?;
    ensure_valid(
        !value.product_id.is_empty()
            && value.product_id.len() <= 200
            && value
                .product_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')),
        "product",
    )?;
    Ok(())
}

fn validate_push(value: &AndroidPushRegistration) -> Result<()> {
    ensure_valid(
        value.installation.len() == 64
            && value
                .installation
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "installation",
    )?;
    ensure_valid(
        !value.token.is_empty()
            && value.token.len() <= 4096
            && !value.token.chars().any(char::is_whitespace),
        "FCM token",
    )?;
    ensure_valid(
        (6..=63).contains(&value.project_id.len())
            && value
                .project_id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "Firebase project",
    )?;
    Ok(())
}

#[uniffi::export]
impl MobileEngine {
    pub async fn android_preparation_challenge(
        &self,
        details: AndroidPurchasePreparation,
    ) -> Result<ObjectSnapshot> {
        validate_preparation(&details)?;
        let digest = hex::encode(*dmsg_protocol::digest("dmsg/android/action/v1", &details));
        self.mobile_post("android-challenge",json!({"installation":details.installation,"purpose":"purchase-prepare","digest":digest}),None).await
    }

    pub async fn android_prepare_purchase(
        &self,
        details: AndroidPurchasePreparation,
        proof: AndroidIntegrityProof,
        request_id: String,
    ) -> Result<ObjectSnapshot> {
        validate_preparation(&details)?;
        ensure_valid(
            !proof.token.is_empty() && proof.token.len() <= 32000,
            "integrity token",
        )?;
        let digest = hex::encode(*dmsg_protocol::digest("dmsg/android/action/v1", &details));
        let snapshot = self
            .mobile_post(
                "android-purchase-prepare",
                json!({"details":details,"proof":proof}),
                Some(request_id),
            )
            .await?;
        let value: serde_json::Value =
            serde_json::from_str(&snapshot.json).map_err(|_| Error::IntegrityFailed)?;
        ensure_valid(
            value["prepared"] == true
                && value["digest"] == digest
                && value["expires_at"].as_u64().is_some(),
            "purchase preparation",
        )?;
        Ok(snapshot)
    }
    pub async fn google_play_products(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| {
            Box::pin(async { dmsg_client::commerce::google_play_products(h).await })
        })
        .await
    }

    pub async fn google_play_preflight(
        &self,
        product: String,
        option: String,
        offer: Option<String>,
        package: String,
    ) -> Result<ObjectSnapshot> {
        let p = product.clone();
        let o = option.clone();
        let f = offer.clone();
        let application = package.clone();
        let (snapshot, gateway) = tokio::try_join!(
            self.held_object(move |h| Box::pin(async move {
                dmsg_client::commerce::google_play_purchase_check(
                    h,
                    &p,
                    &o,
                    f.as_deref(),
                    &application,
                )
                .await
            })),
            self.android_configuration()
        )?;
        let mut check: serde_json::Value =
            serde_json::from_str(&snapshot.json).map_err(|_| Error::IntegrityFailed)?;
        let gateway: serde_json::Value =
            serde_json::from_str(&gateway.json).map_err(|_| Error::IntegrityFailed)?;
        if check["eligible"] == true
            && !play_gateway_matches(
                &gateway,
                &check,
                &package,
                &product,
                &option,
                offer.as_deref(),
            )
        {
            check["eligible"] = false.into();
            check["reason"] = "unavailable".into();
        }
        Ok(ObjectSnapshot {
            generation: snapshot.generation,
            json: serde_json::to_string(&check).map_err(|_| Error::IntegrityFailed)?,
        })
    }

    pub async fn android_purchase_challenge(
        &self,
        details: AndroidPurchase,
    ) -> Result<ObjectSnapshot> {
        validate_purchase(&details)?;
        let digest = hex::encode(*dmsg_protocol::digest("dmsg/android/action/v1", &details));
        self.mobile_post(
            "android-challenge",
            json!({"installation":details.installation,"purpose":"purchase","digest":digest}),
            None,
        )
        .await
    }

    pub async fn android_submit_purchase(
        &self,
        details: AndroidPurchase,
        proof: AndroidIntegrityProof,
        request_id: String,
    ) -> Result<ObjectSnapshot> {
        validate_purchase(&details)?;
        ensure_valid(
            !proof.token.is_empty() && proof.token.len() <= 32000,
            "integrity token",
        )?;
        self.mobile_post(
            "android-purchase",
            json!({"details":details,"proof":proof}),
            Some(request_id),
        )
        .await
    }

    pub async fn android_purchases(&self, cursor: Option<String>) -> Result<ObjectSnapshot> {
        ensure_valid(
            cursor.as_ref().is_none_or(|c| {
                c.len() == 64
                    && c.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }),
            "purchase cursor",
        )?;
        self.mobile_read(cursor.map_or_else(
            || "android-purchases".into(),
            |c| format!("android-purchases?cursor={c}"),
        ))
        .await
    }

    pub async fn android_configuration(&self) -> Result<ObjectSnapshot> {
        self.mobile_read("android-configuration".into()).await
    }

    pub async fn notification_installation(&self) -> Result<String> {
        self.host
            .run(|c| Box::pin(async { c.notification_installation_id() }))
            .await
            .map_err(Into::into)
    }

    pub async fn android_push_challenge(
        &self,
        details: AndroidPushRegistration,
    ) -> Result<ObjectSnapshot> {
        validate_push(&details)?;
        let digest = hex::encode(*dmsg_protocol::digest("dmsg/android/action/v1", &details));
        self.mobile_post(
            "android-challenge",
            json!({"installation":details.installation,"purpose":"push","digest":digest}),
            None,
        )
        .await
    }

    pub async fn android_register_push(
        &self,
        details: AndroidPushRegistration,
        proof: AndroidIntegrityProof,
        request_id: String,
    ) -> Result<ObjectSnapshot> {
        validate_push(&details)?;
        ensure_valid(
            proof.token.len() <= 32000 && !proof.token.is_empty(),
            "integrity token",
        )?;
        self.mobile_post(
            "android-push-register",
            json!({"details":details,"proof":proof}),
            Some(request_id),
        )
        .await
    }

    pub async fn android_unregister_push(&self, installation: String) -> Result<ObjectSnapshot> {
        self.mobile_post(
            "android-push-unregister",
            json!({"installation":installation}),
            None,
        )
        .await
    }

    pub async fn notification_register_android(
        &self,
        installation: String,
        revision: u64,
        language: String,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                use dmsg_client::notifications as notice;
                notice::refresh_capabilities(h).await?;
                let context = notice::remote::register_android(
                    h,
                    &language,
                    notice::remote::AndroidRegistration {
                        installation,
                        revision,
                    },
                )
                .await?;
                if let Some(context) = &context {
                    notice::remote::sync_settings(h, context).await?;
                    notice::preferences::sync(h, context).await?;
                }
                Ok(context)
            })
        })
        .await
    }
}

fn play_gateway_matches(
    gateway: &serde_json::Value,
    check: &serde_json::Value,
    package: &str,
    product: &str,
    option: &str,
    offer: Option<&str>,
) -> bool {
    let (Some(home), Some(commerce), Some(verifier)) = (
        check["authority"]["home"].as_str(),
        check["authority"]["commerce"].as_str(),
        gateway["play"]["verifier"].as_str(),
    ) else {
        return false;
    };
    gateway["package_name"] == package
        && gateway["play"]["commerce_canisters"][home] == commerce
        && check["authority"]["verifiers"]
            .as_array()
            .is_some_and(|values| values.iter().any(|v| v.as_str() == Some(verifier)))
        && gateway["play"]["enabled"] == true
        && gateway["play"]["environment"] == check["environment"]
        && gateway["play"]["accept_test"] == check["accept_test"]
        && gateway["play"]["products"]
            .as_array()
            .is_some_and(|products| {
                products.iter().any(|p| {
                    p["product_id"] == product
                        && p["option_id"] == option
                        && p["offer_id"].as_str() == offer
                        && p["kind"] == check["kind"]
                        && p["enabled"] == true
                })
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_preflight_requires_both_exact_governance_and_ready_provider_selection() {
        let check = json!({"environment":"Production","accept_test":false,"kind":"subscription","authority":{"home":"f7crg-kabae","commerce":"ryjl3-tyaaa-aaaaa-aaaba-cai","verifiers":["f7crg-kabae"]}});
        let mut gateway = json!({"package_name":"net.dmsg.android","play":{"enabled":true,"environment":"Production","accept_test":false,"verifier":"f7crg-kabae","commerce_canisters":{"f7crg-kabae":"ryjl3-tyaaa-aaaaa-aaaba-cai"},"products":[{"product_id":"plus","option_id":"annual","offer_id":null,"kind":"subscription","enabled":true}]}});
        assert!(play_gateway_matches(
            &gateway,
            &check,
            "net.dmsg.android",
            "plus",
            "annual",
            None
        ));
        assert!(!play_gateway_matches(
            &gateway,
            &check,
            "net.dmsg.android",
            "plus",
            "monthly",
            None
        ));
        assert!(!play_gateway_matches(
            &gateway,
            &check,
            "net.dmsg.android",
            "plus",
            "annual",
            Some("trial")
        ));
        assert!(!play_gateway_matches(
            &gateway,
            &check,
            "net.dmsg.android.other",
            "plus",
            "annual",
            None
        ));
        gateway["play"]["verifier"] = "unregistered".into();
        assert!(!play_gateway_matches(
            &gateway,
            &check,
            "net.dmsg.android",
            "plus",
            "annual",
            None
        ));
        gateway["play"]["verifier"] = "f7crg-kabae".into();
        gateway["play"]["enabled"] = false.into();
        assert!(!play_gateway_matches(
            &gateway,
            &check,
            "net.dmsg.android",
            "plus",
            "annual",
            None
        ));
        assert!(!play_gateway_matches(
            &json!({}),
            &check,
            "net.dmsg.android",
            "plus",
            "annual",
            None
        ));
    }
}
