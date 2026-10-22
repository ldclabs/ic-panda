//! The account's official PrincipalDocument, published by dmsg_directory.
//! Both JSON and human-facing readers consume the same certified bytes.
use crate::codec::{canonical, sha256};
use crate::upload::{ChunkRef, UploadPlan, random_hex};
use crate::{Client, cloud::Cloud, config::Config, hold::Hold, ic::Ic, now_ms};
pub use agent_protocols::delegation::PrincipalDocument;
use agent_protocols::delegation::{validate_principal_document, validate_principal_resolution};
use candid::Principal;
use dmsg_types::{
    AccountId, CertifiedBatch, Error, Result,
    agent::{DirectoryInit, PrincipalInfo},
    ensure, ensure_valid,
    handle::HandleInit,
    user::{AccountCommand, UserInit},
};
use futures_util::stream::StreamExt;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Query concurrency for account names, so one large channel does not flood
/// a replica with a hundred agents at once.
const QUERY_CONCURRENCY: usize = 16;

/// One name-registry read and the user/directory configs it leads to, shared by
/// every account resolved in a batch.
#[derive(Clone)]
struct Resolver {
    ic: Ic,
    registry: HandleInit,
    users: BTreeMap<Principal, Result<UserInit>>,
    directories: BTreeMap<Principal, Result<DirectoryInit>>,
}

impl Resolver {
    async fn new(config: &Config) -> Result<Self> {
        let ic = Ic::anonymous(config).await?;
        let registry: HandleInit = ic.query(config.handle, "get_handle_config", ()).await?;
        ensure(
            registry.environment == config.environment
                && config
                    .user_homes
                    .iter()
                    .all(|home| registry.user_homes.contains(home)),
            Error::IntegrityFailed,
        )?;
        Ok(Self {
            ic,
            registry,
            users: BTreeMap::new(),
            directories: BTreeMap::new(),
        })
    }

    fn home(&self, account: &AccountId) -> Result<Principal> {
        dmsg_protocol::agent::account_home(
            &self.registry.environment,
            &self.registry.issuer_namespace,
            &self.registry.user_homes,
            account,
        )
        .ok_or(Error::NotFound)
    }

    /// Read each distinct home and directory once. Keep failures with their
    /// canister so unrelated accounts can still resolve in the same batch.
    async fn prepare(&mut self, homes: impl IntoIterator<Item = Principal>) {
        let homes: BTreeSet<Principal> = homes.into_iter().collect();
        let missing: Vec<Principal> = homes
            .iter()
            .copied()
            .filter(|home| !self.users.contains_key(home))
            .collect();
        if !missing.is_empty() {
            let ic = self.ic.clone();
            let fetched = futures_util::future::join_all(missing.into_iter().map(|home| {
                let ic = ic.clone();
                async move {
                    let config: Result<UserInit> = ic.query(home, "user_config", ()).await;
                    (home, config)
                }
            }))
            .await;
            for (home, user) in fetched {
                self.users.insert(home, user);
            }
        }
        let directories: BTreeSet<Principal> = homes
            .iter()
            .filter_map(|home| {
                self.users
                    .get(home)?
                    .as_ref()
                    .ok()
                    .map(|user| user.directory_canister)
            })
            .collect();
        let missing: Vec<Principal> = directories
            .iter()
            .copied()
            .filter(|directory| !self.directories.contains_key(directory))
            .collect();
        if !missing.is_empty() {
            let ic = self.ic.clone();
            let fetched = futures_util::future::join_all(missing.into_iter().map(|directory| {
                let ic = ic.clone();
                async move {
                    let config: Result<DirectoryInit> =
                        ic.query(directory, "directory_config", ()).await;
                    (directory, config)
                }
            }))
            .await;
            for (directory, config) in fetched {
                self.directories.insert(directory, config);
            }
        }
    }

    /// Check the directory binding and propagate only this account's errors.
    fn context(&self, account: &AccountId) -> Result<(Principal, &UserInit, &DirectoryInit)> {
        let home = self.home(account)?;
        let user = self
            .users
            .get(&home)
            .ok_or_else(|| Error::Unavailable("user config".into()))?
            .as_ref()
            .map_err(Clone::clone)?;
        let directory = self
            .directories
            .get(&user.directory_canister)
            .ok_or_else(|| Error::Unavailable("directory config".into()))?
            .as_ref()
            .map_err(Clone::clone)?;
        ensure(
            directory.principal_origin == user.principal_origin
                && directory.environment == self.registry.environment
                && directory.issuer_namespace == self.registry.issuer_namespace
                && directory.user_homes.contains(&home),
            Error::IntegrityFailed,
        )?;
        Ok((home, user, directory))
    }

    async fn document(&self, account: &AccountId) -> Result<Option<PrincipalDocument>> {
        let (_, user, _) = self.context(account)?;
        let batch: Result<CertifiedBatch> = self
            .ic
            .query(user.directory_canister, "principal_document", (*account,))
            .await?;
        let bytes = crate::certified::certified_path(
            &batch?,
            account.as_slice(),
            &[b"documents", account.as_slice()],
            user.directory_canister,
            &self.ic.root_key(),
            now_ms(),
        )?;
        bytes
            .map(|bytes| parse_document(&bytes, account, &user.principal_origin))
            .transpose()
    }
}

/// Parse and independently validate one certified document.
fn parse_document(
    bytes: &[u8],
    account: &AccountId,
    principal_origin: &str,
) -> Result<PrincipalDocument> {
    ensure(
        bytes.len() <= dmsg_protocol::agent::MAX_PRINCIPAL_DOCUMENT_BYTES,
        Error::IntegrityFailed,
    )?;
    let document: PrincipalDocument =
        serde_json::from_slice(bytes).map_err(|_| Error::IntegrityFailed)?;
    validate_principal_document(&document).map_err(|_| Error::IntegrityFailed)?;
    validate_principal_resolution(
        &document,
        &dmsg_protocol::agent::principal_id(principal_origin, account),
    )
    .map_err(|_| Error::IntegrityFailed)?;
    ensure(
        serde_jcs::to_vec(&document).ok().as_deref() == Some(bytes),
        Error::IntegrityFailed,
    )?;
    dmsg_protocol::agent::principal_metadata(&document).map_err(|_| Error::IntegrityFailed)?;
    Ok(document)
}

/// Read exact certified JSON, independently of the HTTP gateway or relay.
/// # Errors
/// Rejects stale, forged or misbound certificates and noncanonical documents.
pub async fn resolve(config: &Config, account: &AccountId) -> Result<Option<PrincipalDocument>> {
    let mut resolver = Resolver::new(config).await?;
    let home = resolver.home(account)?;
    resolver.prepare([home]).await;
    resolver.document(account).await
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
    let mut resolver = Resolver::new(&config).await?;
    let home = resolver.home(&account)?;
    resolver.prepare([home]).await;
    if let Some(document) = resolver.document(&account).await? {
        return Ok(document);
    }
    let (_, user, directory) = resolver.context(&account)?;
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
        delegation_service: Some(directory.delegation_service.clone()),
        updated_at: 0,
        extra: Default::default(),
    })
}

/// Public names, fetched concurrently from the same documents as profile pages.
/// One registry read and one config read per distinct home/directory serve a
/// whole batch; a missing account has no name, and a transient failure is
/// omitted so the caller retries instead of caching an empty name.
pub async fn names<H: Hold>(
    h: &H,
    accounts: &[AccountId],
) -> Result<Vec<(String, Option<String>)>> {
    let config = h.with(|c| Ok(c.config.clone())).await?;
    let mut resolver = Resolver::new(&config).await?;
    let unique: BTreeSet<AccountId> = accounts.iter().copied().collect();
    let mut located = BTreeMap::new();
    for account in &unique {
        match resolver.home(account) {
            Ok(home) => {
                located.insert(*account, home);
            }
            Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
    }
    resolver.prepare(located.values().copied()).await;
    let results: BTreeMap<AccountId, Result<Option<PrincipalDocument>>> =
        futures_util::stream::iter(located.keys().copied())
            .map(|account| {
                let resolver = resolver.clone();
                async move { (account, resolver.document(&account).await) }
            })
            .buffer_unordered(QUERY_CONCURRENCY)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect();
    let mut found = Vec::with_capacity(accounts.len());
    for account in accounts {
        let name = match results.get(account) {
            Some(Ok(Some(document))) => document.name.clone().filter(|name| !name.is_empty()),
            Some(Ok(None)) | Some(Err(Error::NotFound | Error::IntegrityFailed)) | None => None,
            Some(Err(_)) => continue,
        };
        found.push((account.to_string(), name));
    }
    Ok(found)
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
    let mut resolver = Resolver::new(&config).await?;
    let home = resolver.home(&account)?;
    resolver.prepare([home]).await;
    let (_, _, directory) = resolver.context(&account)?;
    let origin = directory.delegation_service.clone();
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

/// The avatar an account's document names, as the relay serves it to
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

#[cfg(test)]
mod tests {
    use super::*;

    fn document(account: &AccountId, origin: &str) -> PrincipalDocument {
        PrincipalDocument {
            id: dmsg_protocol::agent::principal_id(origin, account),
            kind: Some("person".into()),
            name: Some("Panda".into()),
            description: None,
            avatar_url: None,
            aliases: vec![],
            links: vec![agent_protocols::delegation::PrincipalLink {
                name: "dMsg".into(),
                url: format!("https://dmsg.test/{account}"),
                rel: "profile".into(),
            }],
            protocol: "agent-delegation/1.0".into(),
            controllers: vec![],
            retired_controllers: vec![],
            delegation_service: Some("https://api.dmsg.test".into()),
            updated_at: 1,
            extra: Default::default(),
        }
    }

    #[test]
    fn certified_documents_are_bounded_canonical_and_valid_metadata() {
        let account = AccountId([7; 12]);
        let origin = "https://id.dmsg.test";
        let document = document(&account, origin);
        let bytes = serde_jcs::to_vec(&document).unwrap();
        assert_eq!(parse_document(&bytes, &account, origin).unwrap(), document);

        let noncanonical = serde_json::to_vec_pretty(&document).unwrap();
        assert!(parse_document(&noncanonical, &account, origin).is_err());

        let mut too_long = document.clone();
        too_long.name = Some("x".repeat(81));
        assert!(parse_document(&serde_jcs::to_vec(&too_long).unwrap(), &account, origin).is_err());

        let mut too_large = document;
        too_large.description = Some("x".repeat(70_000));
        let bytes = serde_jcs::to_vec(&too_large).unwrap();
        assert!(bytes.len() > dmsg_protocol::agent::MAX_PRINCIPAL_DOCUMENT_BYTES);
        assert!(parse_document(&bytes, &account, origin).is_err());
    }
}
