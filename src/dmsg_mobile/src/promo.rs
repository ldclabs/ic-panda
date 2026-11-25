//! Promotion code preview/redemption and code-gated independent sample workspace setup.
use super::*;
use dmsg_client::vault::{Changes, FieldChange};
use dmsg_types::Hash;

const DEMO_LOGIN_TTL_MS: u64 = 30 * dmsg_types::DAY;

const DEMO_SAMPLE_MARKDOWN: &str = "\
# Welcome to dMsg Sample Workspace

This is an independent, end-to-end encrypted sample cloud workspace created on this device.

## Suggested checks

- **Vault**: Create, edit, favorite, or search private logins and notes.
- **Files**: Preview this Markdown file, back it up to cloud storage, or create a share link.
- **File requests (Plus)**: Create a file request link, open it in a browser, upload a sample file, and receive it back in dMsg.
- **Membership**: Open *Account › Plan & usage* to view the verified Plus membership granted to this sample account.
";

fn validate_promo_code(code: &str) -> dmsg_types::Result<String> {
    let trimmed = code.trim();
    ensure_valid(
        !trimmed.is_empty()
            && trimmed.len() <= 96
            && trimmed
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'-'),
        "code",
    )?;
    Ok(trimmed.to_string())
}

fn parse_request_id(value: &str) -> dmsg_types::Result<Hash> {
    let bytes = hex::decode(value)
        .ok()
        .and_then(|v| <[u8; 32]>::try_from(v).ok())
        .ok_or_else(|| dmsg_types::invalid("request_id"))?;
    Ok(Hash::new(bytes))
}

fn seed_demo_workspace(client: &mut Client, staging: &std::path::Path) -> dmsg_types::Result<()> {
    client.vault_create(
        "login",
        &Changes {
            title: Some("Sample Studio Portal".into()),
            notes: Some(
                "Fictional sample login entry for testing dMsg vault editing and sync.".into(),
            ),
            favorite: Some(true),
            tags: Some(vec!["sample".into()]),
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

    client.vault_create(
        "note",
        &Changes {
            title: Some("Sample Workspace Guide".into()),
            notes: Some(
                "This independent sample account has a verified Plus membership.\n\n\
                 Try editing vault items, backing up files, opening Files › File requests \
                 to collect a file from a browser, and checking Account › Plan & usage."
                    .into(),
            ),
            tags: Some(vec!["sample".into()]),
            ..Default::default()
        },
    )?;

    let folder = client.files_mkdir("root", "Sample Materials")?;
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

#[uniffi::export]
impl MobileEngine {
    /// Preview a promotion code for the currently unlocked cloud account.
    pub async fn promo_preview(&self, code: String) -> Result<ObjectSnapshot> {
        let code = validate_promo_code(&code)?;
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
        let code = validate_promo_code(&code)?;
        let request_id = parse_request_id(&request_id)?;
        self.held_object(move |h| {
            Box::pin(async move {
                let reply =
                    dmsg_client::commerce::promo(h, "redeem", &code, &request_id, None, None)
                        .await?;
                if reply["status"] == "applied"
                    && let Ok(plan) = dmsg_client::snapshot::kind("plan")
                {
                    let _ = dmsg_client::snapshot::refresh(h, plan, "").await;
                }
                Ok(reply)
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
        let code = validate_promo_code(&code)?;
        let config = self.config.clone();
        let preview_code = code.clone();
        let delegated = runtime()
            .spawn(async move {
                let preview =
                    dmsg_client::commerce::public_promo_preview(&config, &preview_code).await?;
                ensure(
                    preview["valid"] == true,
                    dmsg_types::invalid("PROMO_UNAVAILABLE"),
                )?;
                let targets = dmsg_client::account::login_targets(&config).await?;
                dmsg_client::login::ephemeral_delegated(
                    &targets,
                    DEMO_LOGIN_TTL_MS,
                    dmsg_client::now_ms(),
                )
            })
            .await
            .map_err(|_| Error::Unavailable("demo worker".into()))??;

        let controller = self.host.scoped(self.host.inner.generation())?;
        let transition = controller.begin_transition()?;
        self.platform.cancel_presence();
        self.cancel_login();
        let generation = transition.generation();

        let prepared = transition
            .run(move |c| {
                Box::pin(async move {
                    if c.is_local() {
                        c.leave_local()?;
                    }
                    c.lock();
                    c.prepare_login(delegated).await
                })
            })
            .await?;
        self.finish_login_unlock(generation, prepared).await?;

        let prepared = transition
            .run(|c| Box::pin(async { c.prepare_initialize_account().await }))
            .await?;
        self.finish_login_unlock(generation, prepared).await?;

        transition
            .run(|c| Box::pin(async { c.initialize_account().await }))
            .await?;

        let staging = self.staging.clone();
        runtime()
            .spawn(async move {
                let redeem_id = Hash::new(dmsg_client::crypto::random_id());
                let redeemed = dmsg_client::commerce::promo(
                    &transition,
                    "redeem",
                    &code,
                    &redeem_id,
                    None,
                    None,
                )
                .await;
                let failure = match redeemed {
                    Ok(reply) if matches!(reply["status"].as_str(), Some("rejected" | "failed")) => {
                        Some(dmsg_types::invalid(
                            reply["reason"]
                                .as_str()
                                .filter(|r| !r.is_empty())
                                .unwrap_or("PROMO_UNAVAILABLE"),
                        ))
                    }
                    Ok(_) => None,
                    Err(error) => Some(error),
                };
                if let Some(error) = failure {
                    if let Ok(Some(account)) = transition
                        .run(|c| Box::pin(async { Ok(c.session_state().account) }))
                        .await
                    {
                        let _ = transition
                            .run(move |c| {
                                Box::pin(async move { c.remove_local_account(&account).map(drop) })
                            })
                            .await;
                    }
                    let _ = transition.finish();
                    return Err(error);
                }

                if let Ok(plan) = dmsg_client::snapshot::kind("plan") {
                    let _ = dmsg_client::snapshot::refresh(&transition, plan, "").await;
                }

                transition
                    .run(move |c| Box::pin(async move { seed_demo_workspace(c, &staging) }))
                    .await?;

                transition.finish()
            })
            .await
            .map_err(|_| Error::Unavailable("demo worker".into()))??;

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
            validate_promo_code("  RVW-8K4P-9M2X ").unwrap(),
            "RVW-8K4P-9M2X"
        );
        assert!(validate_promo_code("").is_err());
        assert!(validate_promo_code("   ").is_err());
        assert!(validate_promo_code("bad/code").is_err());
        assert!(validate_promo_code(&"A".repeat(97)).is_err());

        assert!(parse_request_id(&"a".repeat(64)).is_ok());
        assert!(parse_request_id("short").is_err());
    }

    #[test]
    fn seeds_fictional_sample_vault_items_and_markdown_file_cleanly() {
        let dir = testing::TempDir::new();
        let staging = dir.path("staging");
        let mut client = testing::unlocked(&dir.0, false, None);
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

        let root = client.files_list("root", false).unwrap();
        let folder = root
            .folders
            .iter()
            .find(|f| f.name == "Sample Materials")
            .expect("sample folder");
        let listing = client.files_list(&folder.id, false).unwrap();
        assert!(listing.files.iter().any(|f| f.name == "Welcome-Sample.md"));

        // Staging temporary directory must be removed after import.
        let leftover = std::fs::read_dir(&staging)
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(leftover, 0);
    }
}
