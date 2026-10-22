//! The account's official PrincipalDocument, published by dmsg_directory.
//! Both JSON and human-facing readers consume the same certified bytes.
use crate::codec::{canonical, sha256};
use crate::upload::{ChunkRef, UploadPlan, random_hex};
use crate::{Client, cloud::Cloud, config::Config, hold::Hold, ic::Ic, now_ms};
pub use agent_protocols::delegation::PrincipalDocument;
use agent_protocols::delegation::{validate_principal_document, validate_principal_resolution};
use dmsg_types::{
    AccountId, Error, Result,
    agent::{DirectoryInit, PrincipalInfo},
    ensure, ensure_valid,
    user::{AccountCommand, UserInit},
};
use serde::Serialize;

/// Resolve a user home and its pinned directory from the name registry.
async fn context(config: &Config, account: &AccountId) -> Result<(Ic, UserInit, DirectoryInit)> {
    let ic = Ic::anonymous(config).await?;
    let registry: dmsg_types::handle::HandleInit =
        ic.query(config.handle, "get_handle_config", ()).await?;
    ensure(
        registry.environment == config.environment
            && config
                .user_homes
                .iter()
                .all(|h| registry.user_homes.contains(h)),
        Error::IntegrityFailed,
    )?;
    let home = dmsg_protocol::agent::account_home(
        &registry.environment,
        &registry.issuer_namespace,
        &registry.user_homes,
        account,
    )
    .ok_or(Error::NotFound)?;
    let user: UserInit = ic.query(home, "user_config", ()).await?;
    let directory: DirectoryInit = ic
        .query(user.directory_canister, "directory_config", ())
        .await?;
    ensure(
        directory.principal_origin == user.principal_origin
            && directory.environment == registry.environment
            && directory.issuer_namespace == registry.issuer_namespace
            && directory.user_homes.contains(&home),
        Error::IntegrityFailed,
    )?;
    Ok((ic, user, directory))
}

/// Read exact certified JSON, independently of the HTTP gateway or relay.
/// # Errors
/// Rejects stale, forged or misbound certificates and noncanonical documents.
pub async fn resolve(config: &Config, account: &AccountId) -> Result<Option<PrincipalDocument>> {
    let (ic, user, _) = context(config, account).await?;
    let batch: Result<dmsg_types::CertifiedBatch> = ic
        .query(user.directory_canister, "principal_document", (*account,))
        .await?;
    let bytes = crate::certified::certified_path(
        &batch?,
        account.as_slice(),
        &[b"documents", account.as_slice()],
        user.directory_canister,
        &ic.root_key(),
        now_ms(),
    )?;
    bytes
        .map(|bytes| {
            let document: PrincipalDocument =
                serde_json::from_slice(&bytes).map_err(|_| Error::IntegrityFailed)?;
            validate_principal_document(&document).map_err(|_| Error::IntegrityFailed)?;
            validate_principal_resolution(
                &document,
                &dmsg_protocol::agent::principal_id(&user.principal_origin, account),
            )
            .map_err(|_| Error::IntegrityFailed)?;
            ensure(
                serde_jcs::to_vec(&document).ok().as_ref() == Some(&bytes),
                Error::IntegrityFailed,
            )?;
            Ok(document)
        })
        .transpose()
}

/// Resolve another account without holding a client across network requests.
pub async fn resolve_account<H: Hold>(
    h: &H,
    account: &AccountId,
) -> Result<Option<PrincipalDocument>> {
    let config = h.with(|c| Ok(c.config.clone())).await?;
    resolve(&config, account).await
}

/// Own document, or an unpublished empty document for a first edit.
pub async fn show<H: Hold>(h: &H) -> Result<PrincipalDocument> {
    let (config, account) = h.with(|c| Ok((c.config.clone(), c.account()?.id))).await?;
    if let Some(document) = resolve(&config, &account).await? {
        return Ok(document);
    }
    let (_, user, directory) = context(&config, &account).await?;
    Ok(PrincipalDocument {
        id: dmsg_protocol::agent::principal_id(&user.principal_origin, &account),
        kind: Some("person".into()),
        name: None,
        description: None,
        avatar_url: None,
        aliases: vec![format!("{}{account}", directory.profile_url_prefix)],
        links: vec![],
        protocol: "agent-delegation/1.0".into(),
        controllers: vec![],
        retired_controllers: vec![],
        delegation_service: Some(directory.delegation_service),
        updated_at: 0,
        extra: Default::default(),
    })
}

/// Public names, fetched concurrently from the same documents as profile pages.
pub async fn names<H: Hold>(
    h: &H,
    accounts: &[AccountId],
) -> Result<Vec<(String, Option<String>)>> {
    let config = h.with(|c| Ok(c.config.clone())).await?;
    let results =
        futures_util::future::join_all(accounts.iter().map(|id| resolve(&config, id))).await;
    Ok(accounts
        .iter()
        .zip(results)
        .map(|(id, result)| {
            (
                id.to_string(),
                result
                    .ok()
                    .flatten()
                    .and_then(|d| d.name)
                    .filter(|s| !s.is_empty()),
            )
        })
        .collect())
}

impl Client {
    /// Whether this workspace has a document mutation still awaiting confirmation.
    /// Directory publication alone cannot settle the local account journal.
    /// # Errors
    /// Returns `Error::Locked` when the encrypted journal cannot be read.
    pub fn document_pending(&self) -> Result<bool> {
        Ok(self.pending()?.is_some_and(|journal| {
            matches!(journal.request, crate::account::Request::Mutate(m) if matches!(m.command, AccountCommand::UpdatePrincipal { .. }))
        }))
    }

    /// Read the verified avatar bytes kept in a legacy archive.
    pub fn legacy_avatar(&self, id: &str, source: &str) -> Result<Vec<u8>> {
        let (_, bytes) = self.kept_archive(id)?;
        let archive = crate::legacy::Archive::decode(&bytes)?;
        ensure(
            archive
                .inventory
                .objects
                .iter()
                .any(|o| o.key == source && o.kind == "avatar"),
            Error::NotFound,
        )?;
        Ok(crate::legacy::open_file(&archive, source)?.bytes.to_vec())
    }

    /// Reconcile only this document's uncertain mutation, then retry publication.
    /// Other pending account operations remain owned by their original flow.
    pub async fn retry_document(&mut self) -> Result<PrincipalInfo> {
        self.keys()?;
        if let Some(journal) = self.pending()? {
            ensure(
                matches!(journal.request, crate::account::Request::Mutate(ref m) if matches!(m.command, AccountCommand::UpdatePrincipal { .. })),
                Error::Pending,
            )?;
            self.resume().await?;
        }
        self.principal_publish().await?;
        self.principal().await?.ok_or(Error::NotFound)
    }

    /// Commit display fields under the account mutation journal and publish.
    /// # Errors
    /// Requires an administrator device; stale editor versions are refused.
    pub async fn publish_document(
        &mut self,
        document: &PrincipalDocument,
    ) -> Result<PrincipalInfo> {
        dmsg_protocol::agent::principal_metadata(document)?;
        let bytes = serde_jcs::to_vec(document).map_err(|e| dmsg_types::invalid(&e.to_string()))?;
        self.mutate(
            AccountCommand::UpdatePrincipal {
                document: bytes.into(),
            },
            None,
        )
        .await?;
        self.principal_settled().await;
        self.principal().await?.ok_or(Error::NotFound)
    }

    /// The avatar's upload: its plan and manifest, the image kept in the
    /// store for the upload to read, collected after it.
    fn avatar_upload(&self, bytes: &[u8]) -> Result<(UploadPlan, Vec<u8>)> {
        let mime = avatar_type(bytes)?;
        let account = self.account()?.id.to_string();
        let upload_id = random_hex();
        let manifest = canonical(&AvatarManifest {
            format: "dmsg-cloud-avatar/1",
            account_id: &account,
            upload_id: &upload_id,
            mime,
        });
        let digest = hex::encode(*sha256(bytes));
        let plan = UploadPlan {
            upload_id,
            object_id: random_hex(),
            version_id: random_hex(),
            root_generation: 0,
            epoch: 0,
            control_head: None,
            kind: "avatar".into(),
            chunks: vec![ChunkRef {
                digest: digest.clone(),
                size: bytes.len() as u64,
            }],
            manifest_digest: hex::encode(*sha256(&manifest)),
            manifest_size: manifest.len() as u64,
            expires_at: now_ms() + dmsg_types::DAY,
        };
        self.workspace.store.put_chunk(&digest, bytes)?;
        Ok((plan, manifest))
    }
}

#[derive(Serialize)]
struct AvatarManifest<'a> {
    format: &'a str,
    account_id: &'a str,
    upload_id: &'a str,
    mime: &'a str,
}

/// Upload a public image and return its URL. Publishing the document is a
/// separate explicit operation, so uploading never overwrites editor drafts.
pub async fn upload_avatar<H: Hold>(h: &H, bytes: &[u8]) -> Result<String> {
    let (config, account) = h.with(|c| Ok((c.config.clone(), c.account()?.id))).await?;
    let (_, _, directory) = context(&config, &account).await?;
    let origin = directory.delegation_service;
    // Local replicas use a loopback transport while documents keep HTTPS IDs.
    ensure(
        config.environment == dmsg_types::Environment::Local || config.relay_origin() == origin,
        Error::IntegrityFailed,
    )?;
    let (base, (plan, manifest)) = h
        .with(|c| {
            ensure(c.account()?.id == account, Error::VersionConflict)?;
            Ok((c.base()?, c.avatar_upload(bytes)?))
        })
        .await?;
    crate::upload::upload(
        h,
        &base,
        &plan,
        &manifest,
        0,
        crate::files::Source::Store,
        |_, _| Ok(()),
    )
    .await?;
    Ok(format!(
        "{origin}/public/accounts/{account}/avatar/{}",
        plan.upload_id
    ))
}

/// Largest public avatar: the relay's limit.
pub const MAX_AVATAR: usize = 512 * 1024;

/// The media type of avatar bytes: PNG, JPEG or WebP of 12 bytes to
/// 512 KiB, which the relay checks the same way.
///
/// # Errors
/// Returns `Error::InvalidInput` naming `avatar` for anything else.
pub fn avatar_type(bytes: &[u8]) -> Result<&'static str> {
    ensure_valid((12..=MAX_AVATAR).contains(&bytes.len()), "avatar")?;
    if bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        Ok("image/png")
    } else if bytes.starts_with(&[255, 216, 255]) {
        Ok("image/jpeg")
    } else if bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Ok("image/webp")
    } else {
        Err(dmsg_types::invalid("avatar"))
    }
}

/// The avatar an account's profile names, as the relay serves it to
/// anyone: its media type and bytes, `None` when gone. It needs no key, so
/// the caller holds only the relay client (`Client::relay`) while it
/// downloads, never the client.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for bytes that are not an avatar.
pub async fn avatar(
    relay: &Cloud,
    account: &AccountId,
    upload: &str,
) -> Result<Option<(&'static str, Vec<u8>)>> {
    crate::upload::hex_id(upload)?;
    let bytes = relay
        .public_bytes(&format!("/public/accounts/{account}/avatar/{upload}"))
        .await?;
    bytes
        .map(|b| {
            let mime = avatar_type(&b).map_err(|_| Error::IntegrityFailed)?;
            Ok((mime, b))
        })
        .transpose()
}
