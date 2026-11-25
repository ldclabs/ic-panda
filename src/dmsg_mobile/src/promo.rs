//! Promotion code preview/redemption and code-gated independent sample workspace setup.
use super::*;
use dmsg_client::commerce::{self, PendingPromo, normalize_promo_code};
use dmsg_client::vault::{Changes, FieldChange};
use dmsg_types::Hash;
use serde_json::Value;

const DEMO_LOGIN_TTL_MS: u64 = 30 * dmsg_types::DAY;

const DEMO_SAMPLE_MARKDOWN: &str = "\
# Welcome to dMsg Sample Workspace

This is an independent, end-to-end encrypted sample cloud workspace created on this device.

## Suggested checks

- **Vault**: Create, edit, favorite, or search private logins and notes.
- **Files**: Preview this Markdown file, back it up to cloud storage, or create a share link.
- **File requests (Plus)**: Create a file request link, open it in a browser, upload a sample file, and receive it back in dMsg.
- **Membership**: Open *Account › Plan & usage* to check the current membership and its expiry.
";

fn check_sample_offer(preview: &Value) -> dmsg_types::Result<()> {
    ensure(
        preview["valid"] == true && preview["plan"] == "Plus" && preview["duration_days"] == 30,
        dmsg_types::invalid("PROMO_UNAVAILABLE"),
    )
}

fn parse_request_id(value: &str) -> dmsg_types::Result<Hash> {
    let bytes = hex::decode(value)
        .ok()
        .and_then(|v| <[u8; 32]>::try_from(v).ok())
        .ok_or_else(|| dmsg_types::invalid("request_id"))?;
    Ok(Hash::new(bytes))
}

fn seed_demo_workspace(client: &mut Client, staging: &std::path::Path) -> dmsg_types::Result<()> {
    let entries = client.vault_list(&Default::default())?;
    let mut sample = |kind: &str, title: &str, mut changes: Changes| -> dmsg_types::Result<()> {
        if !entries.iter().any(|e| {
            e.item.kind == kind
                && e.item.title == title
                && e.item.tags.iter().any(|t| t == "sample")
        }) {
            changes.title = Some(title.into());
            changes.tags = Some(vec!["sample".into()]);
            client.vault_create(kind, &changes)?;
        }
        Ok(())
    };
    sample(
        "login",
        "Sample Studio Portal",
        Changes {
            notes: Some(
                "Fictional sample login entry for testing dMsg vault editing and sync.".into(),
            ),
            favorite: Some(true),
            set: Some(vec![
                FieldChange {
                    id: Some("username".into()),
                    value: Some("reviewer@example.test".into()),
                    ..Default::default()
                },
                FieldChange {
                    id: Some("password".into()),
                    value: Some("Sample-Only-Pass-2026!".into()),
                    ..Default::default()
                },
                FieldChange {
                    id: Some("url".into()),
                    value: Some("https://example.test".into()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        },
    )?;

    sample(
        "note",
        "Sample Workspace Guide",
        Changes {
            notes: Some(
                "Check this sample account’s current membership in Account › Plan & usage.\n\n\
                 Try editing vault items, backing up files, opening Files › File requests \
                 to collect a file from a browser, and checking Account › Plan & usage."
                    .into(),
            ),
            ..Default::default()
        },
    )?;

    let folder = match client
        .files_list("root", false)?
        .folders
        .into_iter()
        .find(|f| f.name == "Sample Materials")
    {
        Some(folder) => folder,
        None => client.files_mkdir("root", "Sample Materials")?,
    };
    if client
        .files_list(&folder.id, false)?
        .files
        .iter()
        .any(|f| f.name == "Welcome-Sample.md")
    {
        return Ok(());
    }
    let seed_dir = staging.join(format!(
        "demo-seed-{}",
        hex::encode(dmsg_client::crypto::random_id())
    ));
    std::fs::create_dir_all(&seed_dir)
        .map_err(|_| Error::Unavailable("storage: demo staging".into()))?;
    let sample_path = seed_dir.join("Welcome-Sample.md");
    let imported = std::fs::write(&sample_path, DEMO_SAMPLE_MARKDOWN.as_bytes())
        .map_err(|_| Error::Unavailable("storage: demo sample".into()))
        .and_then(|()| {
            client
                .files_import(&sample_path, &folder.id, false, &mut |_, _| {})
                .map(|_| ())
        });
    let _ = std::fs::remove_dir_all(&seed_dir);
    imported
}

/// Only a response for the signed beneficiary can settle this device's intent.
fn redemption_status<'a>(reply: &'a Value, account: &str) -> dmsg_types::Result<&'a str> {
    ensure(reply["account"] == account, Error::IntegrityFailed)?;
    let status = reply["status"].as_str().ok_or(Error::IntegrityFailed)?;
    if status == "rejected" {
        ensure(
            reply["eligible"] == false && reply["reason"].is_string(),
            Error::IntegrityFailed,
        )?;
    } else {
        ensure(
            matches!(status, "pending" | "applied" | "failed" | "revoked")
                && reply["grant_id"].as_str().is_some_and(|id| !id.is_empty())
                && matches!(reply["plan"].as_str(), Some("Plus" | "Pro" | "Max"))
                && reply["expires_at_ms"].as_u64().is_some()
                && reply["synced"].is_boolean(),
            Error::IntegrityFailed,
        )?;
    }
    Ok(status)
}

async fn settle_promo<H: Hold>(
    h: &H,
    pending: &PendingPromo,
    staging: &std::path::Path,
) -> dmsg_types::Result<Value> {
    let account = h
        .with(|c| c.session_state().account.ok_or(Error::Locked))
        .await?;
    let reply = commerce::promo(
        h,
        "redeem",
        &pending.code,
        &pending.request_id,
        Some(&account),
        None,
    )
    .await?;
    settle_reply(h, pending, &account, reply, staging).await
}

async fn settle_reply<H: Hold>(
    h: &H,
    pending: &PendingPromo,
    account: &str,
    mut reply: Value,
    staging: &std::path::Path,
) -> dmsg_types::Result<Value> {
    let status = redemption_status(&reply, account)?;
    if status == "pending" {
        return Ok(reply);
    }
    if status == "applied" {
        ensure(
            !pending.sample_workspace || reply["plan"] == "Plus",
            Error::IntegrityFailed,
        )?;
        if reply["expires_at_ms"]
            .as_u64()
            .is_some_and(|at| at <= dmsg_client::now_ms())
        {
            h.with(|c| c.finish_promo(&pending.request_id)).await?;
            return Err(Error::Expired);
        }
        if reply["synced"] != true {
            reply["status"] = "pending".into();
            return Ok(reply);
        }
    }
    if status == "applied" && pending.sample_workspace {
        let kind = dmsg_client::cache::Kind::Plan;
        dmsg_client::snapshot::refresh(h, kind, "").await?;
        h.with(|c| {
            let data = &dmsg_client::snapshot::read(c, kind, "")?["data"];
            ensure(
                data["source"] == "Active"
                    && matches!(data["plan"].as_str(), Some("Plus" | "Pro" | "Max"))
                    && data["lease_until_ms"]
                        .as_u64()
                        .is_some_and(|at| at > dmsg_client::now_ms()),
                Error::MembershipStale,
            )?;
            if c.pending_promo()?
                .is_some_and(|p| p.request_id == pending.request_id)
            {
                seed_demo_workspace(c, staging)?;
                c.finish_promo(&pending.request_id)?;
            }
            Ok(())
        })
        .await?;
    } else {
        h.with(|c| c.finish_promo(&pending.request_id)).await?;
    }
    Ok(reply)
}

#[uniffi::export]
impl MobileEngine {
    /// Preview a promotion code for the currently unlocked cloud account.
    pub async fn promo_preview(&self, code: String) -> Result<ObjectSnapshot> {
        let code = normalize_promo_code(&code)?;
        let request_id = Hash::new(dmsg_client::crypto::random_id());
        self.held_object(move |h| {
            Box::pin(async move {
                dmsg_client::commerce::promo(h, "preview", &code, &request_id, None, None).await
            })
        })
        .await
    }

    /// Redeem a promotion code for the currently unlocked cloud account.
    pub async fn promo_redeem(&self, code: String, request_id: String) -> Result<ObjectSnapshot> {
        let code = normalize_promo_code(&code)?;
        let request_id = parse_request_id(&request_id)?;
        let staging = self.staging.clone();
        self.held_object(move |h| {
            Box::pin(async move {
                let pending = h
                    .with(|c| {
                        c.stage_promo(PendingPromo {
                            code,
                            request_id,
                            sample_workspace: false,
                        })
                    })
                    .await?;
                settle_promo(h, &pending, &staging).await
            })
        })
        .await
    }

    /// Reconcile an already authorized redemption for this workspace after a lost reply or restart.
    pub async fn resume_promo(&self) -> Result<ObjectSnapshot> {
        let staging = self.staging.clone();
        self.held_object(move |h| {
            Box::pin(async move {
                match h.with(|c| c.pending_promo()).await? {
                    Some(pending) => settle_promo(h, &pending, &staging).await,
                    None => Ok(Value::Null),
                }
            })
        })
        .await
    }

    /// Read promotion redemption history for the currently unlocked cloud account.
    pub async fn promo_history(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| {
            Box::pin(async {
                dmsg_client::commerce::promo(h, "history", "", &Hash::default(), None, None).await
            })
        })
        .await
    }

    /// Create an independent cloud sample workspace gated by a valid access code,
    /// redeem its Plus grant through the normal commerce pipeline, and populate
    /// safe fictional sample items.
    pub async fn start_demo_with_code(&self, code: String) -> Result<SessionSnapshot> {
        let code = normalize_promo_code(&code)?;
        let config = self.config.clone();
        let preview_code = code.clone();
        let delegated = self
            .held(move |h| {
                Box::pin(async move {
                    h.with(|c| {
                        ensure(
                            !c.is_local()
                                && matches!(c.session_state().state, "logged_out" | "locked"),
                            Error::PolicyStale,
                        )
                    })
                    .await?;
                    let preview = commerce::public_promo_preview(&config, &preview_code).await?;
                    check_sample_offer(&preview)?;
                    let targets = dmsg_client::account::login_targets(&config).await?;
                    dmsg_client::login::ephemeral_delegated(
                        &targets,
                        DEMO_LOGIN_TTL_MS,
                        dmsg_client::now_ms(),
                    )
                })
            })
            .await?;

        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        self.cancel_login();
        let generation = transition.generation();

        let prepared = transition
            .run(move |c| Box::pin(async move { c.prepare_login(delegated).await }))
            .await?;
        self.finish_login_unlock(generation, prepared).await?;

        // The provisional workspace (including this encrypted intent) moves with the account.
        let pending = transition
            .with(|c| {
                c.stage_promo(PendingPromo {
                    code,
                    request_id: Hash::new(dmsg_client::crypto::random_id()),
                    sample_workspace: true,
                })
            })
            .await?;
        let prepared = transition
            .run(|c| Box::pin(async { c.prepare_initialize_account().await }))
            .await?;
        self.finish_login_unlock(generation, prepared).await?;
        let (account, _) = transition
            .run(|c| Box::pin(async { c.initialize_account().await }))
            .await?;

        let staging = self.staging.clone();
        // Dropping the foreign future cancels the worker and releases its transition.
        let mut worker = tokio::task::JoinSet::new();
        worker.spawn_on(
            async move {
                let reply = settle_promo(&transition, &pending, &staging).await?;
                match reply["status"].as_str() {
                    Some("applied") => transition.finish(),
                    Some("pending") => Err(dmsg_types::invalid("PROMO_PENDING")),
                    Some("rejected" | "failed" | "revoked") => {
                        // Only this just-created account and a definitive refusal allow cleanup.
                        transition
                            .with(|c| c.remove_local_account(&account.id.to_string()).map(drop))
                            .await?;
                        Err(dmsg_types::invalid(
                            reply["reason"].as_str().unwrap_or("PROMO_UNAVAILABLE"),
                        ))
                    }
                    _ => Err(Error::IntegrityFailed),
                }
            },
            runtime().handle(),
        );
        worker
            .join_next()
            .await
            .ok_or(Error::Locked)?
            .map_err(|_| Error::Locked)??;

        self.session_state().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_client::testing;

    #[test]
    fn validates_promo_codes_and_request_ids() {
        assert_eq!(
            normalize_promo_code("  RVW-8K4P-9M2X ").unwrap(),
            "RVW8K4P9M2X"
        );
        assert!(normalize_promo_code("").is_err());
        assert!(normalize_promo_code("   ").is_err());
        assert!(normalize_promo_code("bad/code").is_err());
        assert!(normalize_promo_code("-- -").is_err());
        assert!(normalize_promo_code("abc").is_err());
        assert!(normalize_promo_code(&"A".repeat(33)).is_err());
        assert!(normalize_promo_code(&"A".repeat(97)).is_err());

        assert!(parse_request_id(&"a".repeat(64)).is_ok());
        assert!(parse_request_id("short").is_err());
    }

    #[test]
    fn only_the_advertised_sample_offer_can_start_account_creation() {
        let mut offer = json!({"valid": true, "plan": "Plus", "duration_days": 30});
        assert!(check_sample_offer(&offer).is_ok());
        offer["plan"] = "Pro".into();
        assert!(check_sample_offer(&offer).is_err());
        offer["plan"] = "Plus".into();
        offer["duration_days"] = 7.into();
        assert!(check_sample_offer(&offer).is_err());
        assert!(check_sample_offer(&json!({"valid": false})).is_err());
    }

    #[tokio::test]
    async fn incomplete_redemptions_survive_restart_and_only_their_own_reply_can_settle_them() {
        let dir = testing::TempDir::new();
        let staging = dir.path("staging");
        let client = testing::unlocked(&dir.0, false, None);
        let account = client.session_state().account.unwrap();
        let original = client
            .stage_promo(PendingPromo {
                code: "SAMPLECODE".into(),
                request_id: Hash::new([1; 32]),
                sample_workspace: true,
            })
            .unwrap();
        drop(client); // The app was killed after sending, before receiving a reply.
        let mut client = testing::reopen_fixture(&dir.0).unwrap();
        let mut retry = original.clone();
        retry.request_id = Hash::new([2; 32]);
        retry.sample_workspace = false;
        assert!(client.stage_promo(retry.clone()).unwrap() == original);
        retry.code = "OTHERCODE".into();
        assert!(matches!(
            client.stage_promo(retry.clone()),
            Err(Error::ExecutionUnknown)
        ));
        let mut reply = json!({"account": account, "grant_id": "grant", "status": "pending",
            "plan": "Plus", "expires_at_ms": dmsg_client::now_ms() + dmsg_types::DAY, "synced": false});
        for status in ["pending", "applied"] {
            reply["status"] = status.into();
            assert_eq!(
                settle_reply(&client.hold(), &original, &account, reply.clone(), &staging)
                    .await
                    .unwrap()["status"],
                "pending"
            );
            assert!(client.pending_promo().unwrap() == Some(original.clone()));
            assert!(client.vault_list(&Default::default()).unwrap().is_empty());
        }
        for invalid in [
            json!({"account": account}),
            json!({"account": account, "status": "unknown"}),
            json!({"account": "different", "status": "rejected", "eligible": false, "reason": "PROMO_EXHAUSTED"}),
        ] {
            assert!(matches!(
                settle_reply(&client.hold(), &original, &account, invalid, &staging).await,
                Err(Error::IntegrityFailed)
            ));
            assert!(client.pending_promo().unwrap() == Some(original.clone()));
        }
        reply["status"] = "revoked".into();
        assert_eq!(
            settle_reply(&client.hold(), &original, &account, reply, &staging)
                .await
                .unwrap()["status"],
            "revoked"
        );
        assert!(client.pending_promo().unwrap().is_none());
        client.stage_promo(retry.clone()).unwrap();
        client.finish_promo(&original.request_id).unwrap();
        assert!(client.pending_promo().unwrap() == Some(retry));
    }

    #[test]
    fn seeds_fictional_sample_vault_items_and_markdown_file_cleanly() {
        let dir = testing::TempDir::new();
        let staging = dir.path("staging");
        let mut client = testing::unlocked(&dir.0, false, None);
        seed_demo_workspace(&mut client, &staging).unwrap();
        seed_demo_workspace(&mut client, &staging).unwrap();

        let entries = client
            .vault_list(&dmsg_client::vault::Filter::default())
            .unwrap();
        let titles: Vec<&str> = entries
            .iter()
            .map(|entry| entry.item.title.as_str())
            .collect();
        assert!(titles.contains(&"Sample Studio Portal"));
        assert!(titles.contains(&"Sample Workspace Guide"));
        assert_eq!(titles.len(), 2);

        let root = client.files_list("root", false).unwrap();
        assert_eq!(
            root.folders
                .iter()
                .filter(|f| f.name == "Sample Materials")
                .count(),
            1
        );
        let folder = root
            .folders
            .iter()
            .find(|f| f.name == "Sample Materials")
            .expect("sample folder");
        let listing = client.files_list(&folder.id, false).unwrap();
        assert_eq!(
            listing
                .files
                .iter()
                .filter(|f| f.name == "Welcome-Sample.md")
                .count(),
            1
        );

        // Staging temporary directory must be removed after import.
        let leftover = std::fs::read_dir(&staging)
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(leftover, 0);
    }
}
