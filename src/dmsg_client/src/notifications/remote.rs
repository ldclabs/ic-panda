//! Purpose-limited receivers keep inactive accounts' metadata current without
//! loading their content roots, switching Client, or extending signing authority.
use super::{Category, Item, Runtime, Target, settings, upsert};
use crate::{
    Client,
    codec::{b64, digest, hex_sha256},
    crypto,
    hold::{Hold, Unlocked},
    now_ms,
};
use dmsg_types::{AccountId, Error, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Write;
use zeroize::Zeroizing;

const RECEIVE: &str = "notification:receive";
const CREDENTIAL: &str = "notification:credential";
const SYNC: &str = "notification:sync";

/// No bearer secret is exposed to native UI or diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiveContext {
    /// Original local account.
    pub account: String,
    /// Device approved in that account.
    pub device: String,
    /// Installation key's public digest.
    pub installation: String,
    /// Opaque registration identity.
    pub registration: String,
    /// Lease revision, used to reject late network replies.
    pub revision: u64,
    /// UTC milliseconds; renewal requires device authentication.
    pub expires_at: u64,
}

#[derive(Serialize, Deserialize)]
struct Credential {
    context: ReceiveContext,
    receiver: String,
}

/// The native App Attest/APNs registration already verified by the service.
#[derive(Clone, Serialize, Deserialize)]
pub struct AppleRegistration {
    /// Original platform UUID.
    pub installation: String,
    /// Platform registration revision returned by push-register.
    pub revision: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct SyncState {
    expected: u64,
    blocked: Option<u64>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RecoveryProof {
    pub digest: String,
    pub pending: String,
}

/// The existing public recovery commitment verifies the owner's disclosed
/// execute-after. The proof is transient on the service and has no content keys.
pub async fn sync_recovery(h: &impl Hold, context: &ReceiveContext) -> Result<()> {
    let h = Unlocked::new(h).await?;
    let proof = h
        .with(|c| {
            ensure(
                c.account()?.id.to_string() == context.account,
                Error::Locked,
            )?;
            let settings = settings(&c.workspace.store)?;
            if !settings.enabled || !settings.categories[&Category::Security].enabled {
                return Ok(None);
            }
            let proof = c
                .workspace
                .private::<RecoveryProof>(c.keys()?, "notification:recovery-proof")?;
            let Some(proof) = proof else {
                return Ok(None);
            };
            let id = super::item_id(&context.account, &Target::Recovery(proof.digest.clone()));
            Ok(c.workspace
                .store
                .notification_item(&id)?
                .is_some_and(|i| i.open)
                .then_some(proof))
        })
        .await?;
    let Some(proof) = proof else {
        return Ok(());
    };
    let body = json!({"installation":context.installation,"pending":proof.pending});
    let reply = h
        .send(|c| {
            let state = c.fresh_state().ok_or(Error::PolicyStale)?;
            c.relay().prepare_raw(
                &c.signer(state)?,
                "/v1/notifications/recovery",
                &body,
                now_ms(),
            )
        })
        .await?;
    ensure(
        reply["id"] == super::item_id(&context.account, &Target::Recovery(proof.digest.clone())),
        Error::IntegrityFailed,
    )?;
    h.with(|c| {
        if c.workspace
            .private::<RecoveryProof>(c.keys()?, "notification:recovery-proof")?
            .as_ref()
            == Some(&proof)
        {
            c.workspace
                .store
                .set_meta("notification:recovery-ack", &proof.digest)?;
            c.workspace.delete_private("notification:recovery-proof")?;
        }
        Ok(())
    })
    .await
}

impl Client {
    fn retired_notification_name(&self, context: &ReceiveContext) -> String {
        self.notification_secret_name(&format!(
            "revoke:{}:{}",
            context.registration, context.revision
        ))
    }

    fn retired_notification_path(&self, context: &ReceiveContext) -> std::path::PathBuf {
        self.notification_runtime_path().with_file_name(format!(
            "notification-revoke-{}-{}",
            context.registration, context.revision
        ))
    }

    /// Preserve only the revocation capability before destructive workspace
    /// cleanup. Removing a local account must work offline without losing the
    /// ability to stop its remote lease on the next connection.
    pub(crate) fn retire_notifications(&self) -> Result<()> {
        if self.workspace.meta.account.is_none() {
            return Ok(());
        }
        let path = self.notification_runtime_path();
        let mut runtime = Runtime::load(&path, now_ms())?;
        runtime.removals.extend(
            self.workspace
                .store
                .notification_deliveries()?
                .into_iter()
                .map(|d| d.system_id),
        );
        runtime.removals.sort();
        runtime.removals.dedup();
        if let Some(context) = self.workspace.store.meta::<ReceiveContext>(RECEIVE)? {
            let credential = self.notification_credential(&context)?;
            let bytes = Zeroizing::new(
                serde_json::to_vec(&credential).map_err(|_| Error::IntegrityFailed)?,
            );
            match self.secure() {
                Some(secure) => secure.set(&self.retired_notification_name(&context), &bytes)?,
                None => {
                    let mut options = std::fs::OpenOptions::new();
                    options.write(true).create(true).truncate(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    let mut file = options
                        .open(self.retired_notification_path(&context))
                        .map_err(crate::storage)?;
                    file.write_all(&bytes).map_err(crate::storage)?;
                    file.sync_all().map_err(crate::storage)?;
                }
            }
            if !runtime.revocations.contains(&context) {
                runtime.revocations.push(context);
            }
        }
        runtime.save(&path)
    }

    fn notification_secret_name(&self, account: &str) -> String {
        format!(
            "dmsg:notifications:{}:{account}",
            hex_sha256(
                self.notification_runtime_path()
                    .as_os_str()
                    .as_encoded_bytes()
            )
        )
    }

    fn installation_seed(&self) -> Result<Zeroizing<crypto::Key>> {
        let name = self.notification_secret_name("installation");
        let path = self
            .notification_runtime_path()
            .with_file_name("notification-key");
        let stored = match self.secure() {
            Some(secure) => secure.get(&name)?,
            None => match std::fs::read(&path) {
                Ok(bytes) => Some(Zeroizing::new(bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(crate::storage(error)),
            },
        };
        if let Some(bytes) = stored {
            return Ok(Zeroizing::new(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::IntegrityFailed)?,
            ));
        }
        let key = Zeroizing::new(crypto::random_key());
        match self.secure() {
            Some(secure) => secure.set(&name, key.as_ref())?,
            None => {
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(path).map_err(crate::storage)?;
                file.write_all(key.as_ref()).map_err(crate::storage)?;
                file.sync_all().map_err(crate::storage)?;
            }
        }
        Ok(key)
    }

    fn notification_credential(&self, context: &ReceiveContext) -> Result<Credential> {
        let id = account_id(&context.account)?;
        self.with_notification_store(&id, |store| {
            let meta = store
                .meta::<crate::workspace::Meta>(crate::workspace::META)?
                .ok_or(Error::IntegrityFailed)?;
            ensure(
                hex::encode(*meta.device_id) == context.device
                    && meta.account.is_some_and(|a| a.id == id),
                Error::AuthRequired,
            )?;
            ensure(
                store.meta::<ReceiveContext>(RECEIVE)?.as_ref() == Some(context),
                Error::VersionConflict,
            )?;
            let value = match self.secure() {
                Some(secure) => secure
                    .get(&self.notification_secret_name(&context.account))?
                    .map(|bytes| serde_json::from_slice(&bytes).map_err(|_| Error::IntegrityFailed))
                    .transpose()?,
                None => store.meta(CREDENTIAL)?,
            }
            .ok_or(Error::AuthRequired)?;
            let credential: Credential = value;
            ensure(credential.context == *context, Error::VersionConflict)?;
            Ok(credential)
        })
    }

    /// Enumerate only locally bound account receivers, without switching
    /// accounts; a workspace from other hardware receives nothing.
    pub fn notification_receivers(&self) -> Result<Vec<ReceiveContext>> {
        let mut result = Vec::new();
        for local in self.accounts()?.into_iter().filter(|a| a.bound && !a.moved) {
            let id = account_id(&local.account)?;
            if self.hardware_pending.contains(&id) {
                continue;
            }
            if let Some(context) =
                self.with_notification_store(&id, |s| s.meta::<ReceiveContext>(RECEIVE))?
                && context.account == local.account
                && context.device == local.device_id
            {
                result.push(context);
            }
        }
        Ok(result)
    }

    /// Capability state belongs to the requested local account, not the active UI.
    pub fn notification_remote_supported(&self, account: &AccountId) -> Result<bool> {
        self.with_notification_store(account, |s| {
            Ok(s.meta::<bool>("notification:remote_v1")?.unwrap_or(false))
        })
    }

    /// Whether the current local account draft conflicts with a remote revision.
    pub fn notification_settings_conflict(&self, account: &AccountId) -> Result<bool> {
        self.with_notification_store(account, |s| {
            Ok(s.meta::<SyncState>(SYNC)?.unwrap_or_default().blocked
                == Some(settings(s)?.revision))
        })
    }

    /// Record a definitive expired/revoked receiver, without turning transient
    /// transport errors into a fabricated account incident.
    pub fn notification_receiver_health(
        &self,
        context: &ReceiveContext,
        needs_auth: bool,
    ) -> Result<()> {
        let account = account_id(&context.account)?;
        let current = self.with_notification_store(&account, |s| {
            if s.meta::<ReceiveContext>(RECEIVE)?.as_ref() != Some(context) {
                return Ok(false);
            }
            s.set_meta("notification:registration_required", &needs_auth)?;
            Ok(true)
        })?;
        if !current {
            return Ok(());
        }
        self.notification_health_episode(
            &context.account,
            "registration",
            needs_auth,
            true,
            now_ms(),
        )
    }

    /// Reuse an already issued hint token for an inactive registered account.
    /// This grants no new scope and never loads that account's signing key.
    pub fn notification_hint_token(
        &self,
        context: &ReceiveContext,
    ) -> Result<Option<crate::connect::HintToken>> {
        self.notification_credential(context)?;
        let account = account_id(&context.account)?;
        self.with_notification_store(&account, |s| {
            if !settings(s)?.enabled
                || s.meta::<bool>("notification:revoke_pending")?
                    .unwrap_or(false)
            {
                return Ok(None);
            }
            let meta = s
                .meta::<crate::workspace::Meta>(crate::workspace::META)?
                .ok_or(Error::IntegrityFailed)?;
            let name = format!(
                "dmsg:{}:{}:{}:hint",
                meta.environment, context.account, context.device
            );
            let token: Option<crate::connect::HintToken> = match self.secure() {
                Some(secure) => secure
                    .get(&name)?
                    .map(|bytes| serde_json::from_slice(&bytes).map_err(|_| Error::IntegrityFailed))
                    .transpose()?,
                None => s.meta(&name)?,
            };
            Ok(token.filter(|t| t.expires_at > now_ms()))
        })
    }

    fn save_notification_credential(&self, credential: &Credential) -> Result<()> {
        let account = account_id(&credential.context.account)?;
        if let Some(secure) = self.secure() {
            secure.set(
                &self.notification_secret_name(&credential.context.account),
                &serde_json::to_vec(credential).map_err(|_| Error::IntegrityFailed)?,
            )?;
        }
        self.with_notification_store(&account, |store| {
            store.atomic(|store| {
                if store
                    .meta::<ReceiveContext>(RECEIVE)?
                    .is_none_or(|previous| previous.registration != credential.context.registration)
                {
                    // A revoked/removed registration starts a fresh account CAS
                    // scope. Its old acknowledgement cannot enable the new lease.
                    let mut preference = settings(store)?;
                    preference.remote_revision = 0;
                    store.set_meta("notification:settings", &preference)?;
                    store.delete_meta(SYNC)?;
                }
                if self.secure().is_none() {
                    store.set_meta(CREDENTIAL, credential)?;
                }
                store.set_meta(RECEIVE, &credential.context)?;
                // An opt-out during the registration request must still revoke
                // its result; saving the response never restores user consent.
                store.set_meta("notification:revoke_pending", &!settings(store)?.enabled)
            })
        })
    }
}

/// Drain removed accounts' restricted revocations without opening a workspace,
/// switching accounts, or retaining its signing/content keys.
pub async fn revoke_retired(h: &impl Hold) -> Result<()> {
    let contexts = h
        .with(|c| Ok(Runtime::load(&c.notification_runtime_path(), now_ms())?.revocations))
        .await?;
    let mut failure = None;
    for context in contexts {
        let result = async {
            if context.expires_at > now_ms() {
                let (cloud, credential) = h.with(|c| {
                    let bytes = match c.secure() {
                        Some(secure) => secure.get(&c.retired_notification_name(&context))?.ok_or(Error::AuthRequired)?,
                        None => Zeroizing::new(std::fs::read(c.retired_notification_path(&context)).map_err(crate::storage)?),
                    };
                    let credential: Credential = serde_json::from_slice(&bytes).map_err(|_| Error::IntegrityFailed)?;
                    ensure(credential.context == context, Error::IntegrityFailed)?;
                    Ok((c.relay(), credential))
                }).await?;
                let response = cloud.notification_receive(&path(&context, "revoke"), &credential.receiver,
                    Some(&json!({"installation":context.installation,"expected_revision":context.revision}))).await;
                if !matches!(response, Ok(_) | Err(Error::AuthRequired | Error::NotFound | Error::DeviceNotApproved)) { response?; }
            }
            h.with(|c| {
                let path = c.notification_runtime_path();
                let mut runtime = Runtime::load(&path, now_ms())?;
                runtime.revocations.retain(|old| old != &context);
                runtime.save(&path)?;
                match c.secure() {
                    Some(secure) => secure.delete(&c.retired_notification_name(&context))?,
                    None => match std::fs::remove_file(c.retired_notification_path(&context)) {
                        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(crate::storage(error)),
                        _ => {}
                    },
                }
                Ok(())
            }).await
        }.await;
        if let Err(error) = result {
            failure.get_or_insert(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

fn account_id(value: &str) -> Result<AccountId> {
    value.parse().map_err(|_| Error::IntegrityFailed)
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn sequence(value: &Value) -> Result<u64> {
    let text = value.as_str().ok_or(Error::IntegrityFailed)?;
    let n = text.parse::<u64>().map_err(|_| Error::IntegrityFailed)?;
    ensure(text == n.to_string(), Error::IntegrityFailed)?;
    Ok(n)
}
fn path(context: &ReceiveContext, action: &str) -> String {
    let (action, query) = action.split_once('?').unwrap_or((action, ""));
    let base = format!(
        "/v1/notifications/{action}?account={}&installation={}&registration={}",
        context.account, context.installation, context.registration
    );
    if query.is_empty() {
        base
    } else {
        format!("{base}&{query}")
    }
}

/// Register/renew the current unlocked account. Existing receivers need no
/// renewal until their last day; native APNs changes explicitly provide a new revision.
pub async fn register(
    h: &impl Hold,
    language: &str,
    apple: Option<AppleRegistration>,
) -> Result<Option<ReceiveContext>> {
    let h = Unlocked::new(h).await?;
    let Some((account, device, key, current)) = h
        .with(|c| {
            let account = c.account()?.id;
            if !c.notification_remote_supported(&account)?
                || !settings(&c.workspace.store)?.enabled
                || c.workspace
                    .store
                    .meta::<bool>("notification:revoke_pending")?
                    .unwrap_or(false)
            {
                return Ok(None);
            }
            Ok(Some((
                account,
                hex::encode(*c.workspace.meta.device_id),
                c.installation_seed()?,
                c.workspace.store.meta::<ReceiveContext>(RECEIVE)?,
            )))
        })
        .await?
    else {
        return Ok(None);
    };
    let needs_auth = h
        .with(|c| {
            Ok(c.workspace
                .store
                .meta::<bool>("notification:registration_required")?
                .unwrap_or(false))
        })
        .await?;
    if !needs_auth
        && apple.is_none()
        && let Some(context) = current
            .as_ref()
            .filter(|c| c.expires_at > now_ms() + 86_400_000)
    {
        return Ok(Some(context.clone()));
    }
    let public_key = crypto::signing_public(&key);
    let installation = hex_sha256(&public_key);
    let request_id = hex::encode(crypto::random_id());
    let language = language.to_string();
    let response = h.send(|c| {
        ensure(c.account()?.id == account,Error::Locked)?;
        let details = json!({"installation":installation,"public_key":hex::encode(public_key),"platform":if apple.is_some(){"ios"}else{"desktop"},"language":language,"apple":apple});
        let mut proof = details.clone();
        proof["account"] = account.to_string().into(); proof["device"] = device.clone().into(); proof["request_id"] = request_id.clone().into();
        proof["environment"] = crate::config::environment_name(&c.config().environment).into(); proof["application"] = "dmsg".into();
        let mut body = details;
        body["proof"] = b64(&crypto::sign(&key,digest("dmsg/notification/installation/v1",&proof).as_ref())).into();
        let state = c.fresh_state().ok_or(Error::PolicyStale)?;
        c.relay().prepare_raw_id(&c.signer(state)?,"/v1/notifications/register",&body,&request_id,now_ms())
    }).await?;
    let context = ReceiveContext {
        account: account.to_string(),
        device,
        installation,
        registration: response["registration"]
            .as_str()
            .ok_or(Error::IntegrityFailed)?
            .into(),
        revision: response["revision"]
            .as_u64()
            .ok_or(Error::IntegrityFailed)?,
        expires_at: response["expires_at"]
            .as_u64()
            .ok_or(Error::IntegrityFailed)?,
    };
    let receiver = response["receiver"]
        .as_str()
        .ok_or(Error::IntegrityFailed)?
        .to_string();
    ensure(
        response["protocol"] == "dmsg-notifications/1"
            && response["installation"] == context.installation
            && valid_hash(&context.registration)
            && valid_hash(&receiver)
            && context.revision > 0
            && context.expires_at > now_ms(),
        Error::IntegrityFailed,
    )?;
    h.with(|c| {
        c.save_notification_credential(&Credential {
            context: context.clone(),
            receiver,
        })?;
        c.notification_receiver_health(&context, false)?;
        let path = c.notification_runtime_path();
        let mut runtime = Runtime::load(&path, now_ms())?;
        runtime.installation = context.installation.clone();
        runtime.save(&path)
    })
    .await?;
    Ok(Some(context))
}

async fn receive(
    h: &impl Hold,
    context: &ReceiveContext,
    action: &str,
    body: Option<Value>,
) -> Result<Value> {
    let (cloud, credential) = h
        .with(|c| Ok((c.relay(), c.notification_credential(context)?)))
        .await?;
    let response = cloud
        .notification_receive(&path(context, action), &credential.receiver, body.as_ref())
        .await?;
    h.with(|c| {
        c.notification_credential(context)?;
        Ok(())
    })
    .await?;
    Ok(response)
}

/// Apply a bounded feed page atomically with its cursor. Domain terminal state
/// is explicit: failed/partial pages never erase existing pending work.
pub async fn refresh(h: &impl Hold, context: &ReceiveContext) -> Result<bool> {
    let account = account_id(&context.account)?;
    let (cursor, revoke, snapshot) = h
        .with(|c| {
            c.with_notification_store(&account, |s| {
                let cursor = s.meta::<u64>("notification:cursor")?.unwrap_or(0);
                let mut snapshot = s.meta::<u64>("notification:snapshot")?;
                if cursor == 0 && snapshot.is_none() {
                    let generation = s
                        .meta::<u64>("notification:snapshot-generation")?
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or(Error::VersionConflict)?;
                    s.set_meta("notification:snapshot-generation", &generation)?;
                    s.set_meta("notification:snapshot", &generation)?;
                    snapshot = Some(generation);
                }
                Ok((
                    cursor,
                    s.meta::<bool>("notification:revoke_pending")?
                        .unwrap_or(false),
                    snapshot,
                ))
            })
        })
        .await?;
    if revoke {
        revoke_receiver(h, context).await?;
        return Ok(false);
    }
    ensure(context.expires_at > now_ms(), Error::AuthRequired)?;
    let page = receive(h, context, &format!("feed?after={cursor}&limit=100"), None).await?;
    if page["gap"] == true {
        ensure(cursor > 0, Error::IntegrityFailed)?;
        h.with(|c| {
            c.with_notification_store(&account, |s| {
                s.delete_meta("notification:snapshot")?;
                s.set_meta("notification:cursor", &0u64)
            })
        })
        .await?;
        return Ok(true);
    }
    let entries: Vec<Item> =
        serde_json::from_value(page["entries"].clone()).map_err(|_| Error::IntegrityFailed)?;
    let next = sequence(&page["next_cursor"])?;
    ensure(
        entries.len() <= 100
            && next >= cursor
            && page["complete"].is_boolean()
            && (page["complete"] == true || (next > cursor && !entries.is_empty())),
        Error::IntegrityFailed,
    )?;
    h.with(|c| {
        c.notification_credential(context)?;
        c.with_notification_store(&account, |s| {
            s.atomic(|s| {
                ensure(
                    s.meta::<u64>("notification:cursor")?.unwrap_or(0) == cursor,
                    Error::VersionConflict,
                )?;
                let now = now_ms();
                for preference in page["resource_preferences"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let channel = preference["resource"]
                        .as_str()
                        .filter(|s| valid_hash(s))
                        .ok_or(Error::IntegrityFailed)?;
                    let revision = preference["revision"]
                        .as_u64()
                        .ok_or(Error::IntegrityFailed)?;
                    let muted = preference["muted"]
                        .as_bool()
                        .ok_or(Error::IntegrityFailed)?;
                    let until = if preference["until"].is_null() {
                        None
                    } else {
                        Some(preference["until"].as_u64().ok_or(Error::IntegrityFailed)?)
                    };
                    super::preferences::adopt(
                        s,
                        channel,
                        revision,
                        muted,
                        preference["hidden"].as_bool().unwrap_or(false),
                        until,
                    )?;
                }
                for mut item in entries {
                    ensure(
                        item.account == context.account
                            && item.id == super::item_id(&item.account, &item.target)
                            && item.version > 0
                            && !item.target.local_only(),
                        Error::IntegrityFailed,
                    )?;
                    ensure(
                        match &item.target {
                            Target::Channel(id)
                            | Target::Inbox(Some(id))
                            | Target::Recovery(id)
                            | Target::Plan(id) => valid_hash(id),
                            _ => false,
                        },
                        Error::IntegrityFailed,
                    )?;
                    s.set_meta(
                        &format!("notification:remote-observed:{}", item.id),
                        &item.observed,
                    )?;
                    item.observed = 0;
                    item.notified = 0;
                    item.stage = 0;
                    item.attempted_at = None;
                    s.set_meta(
                        &format!("notification:seen:{}", item.id),
                        &(snapshot.unwrap_or(0), item.version),
                    )?;
                    if let Target::Channel(channel) = &item.target
                        && let Some(preference) = super::preferences::stored(s, channel)?
                    {
                        item.muted = preference.muted || preference.hidden;
                        item.muted_until = if preference.hidden {
                            None
                        } else {
                            preference.until
                        };
                    }
                    let old = s.notification_item(&item.id)?;
                    if let Some(old) = old.as_ref().filter(|old| old.version == item.version) {
                        if item.unread.is_none() {
                            item.unread = old.unread;
                        }
                        item.quiet |= old.quiet;
                    } else if cursor == 0 && item.category() != Category::Security {
                        item.quiet = true;
                        item.pending_since = now;
                    }
                    upsert(s, item)?;
                }
                if page["complete"] == true {
                    if let Some(generation) = snapshot {
                        // Only items previously supplied by this feed may be
                        // ended by a complete rebuild. Newer local evidence wins.
                        for mut old in s.notification_items()? {
                            if old.open
                                && s.meta::<(u64, u64)>(&format!("notification:seen:{}", old.id))?
                                    .is_some_and(|(seen, version)| {
                                        seen != generation && old.version <= version
                                    })
                            {
                                old.open = false;
                                old.actionable = false;
                                old.unread = Some(0);
                                old.updated_at = now;
                                upsert(s, old)?;
                            }
                        }
                        s.delete_meta("notification:snapshot")?;
                    }
                    let id = super::item_id(&context.account, &Target::Summary(Category::Message));
                    if let Some(mut legacy) = s.notification_item(&id)?.filter(|i| i.open) {
                        legacy.open = false;
                        legacy.unread = Some(0);
                        legacy.updated_at = now;
                        upsert(s, legacy)?;
                    }
                }
                s.set_meta("notification:cursor", &next)
            })
        })
    })
    .await?;
    Ok(page["complete"] == false)
}

/// Send only actual-view watermarks. Server acknowledgements never alter business read state.
pub async fn send_observed(h: &impl Hold, context: &ReceiveContext) -> Result<()> {
    let account = account_id(&context.account)?;
    let items = h
        .with(|c| {
            c.with_notification_store(&account, |s| {
                let mut result = Vec::new();
                if s.meta::<bool>("notification:revoke_pending")?
                    .unwrap_or(false)
                {
                    return Ok(result);
                }
                for item in s.notification_items()? {
                    if matches!(
                        item.target,
                        Target::Channel(_) | Target::Inbox(Some(_)) | Target::Recovery(_)
                    ) && item.observed
                        > s.meta::<u64>(&format!("notification:observed-ack:{}", item.id))?
                            .unwrap_or(0)
                    {
                        result.push((item.id, item.observed));
                        if result.len() == 100 {
                            break;
                        }
                    }
                }
                Ok(result)
            })
        })
        .await?;
    for (id, version) in items {
        match receive(
            h,
            context,
            "observed",
            Some(
                json!({"installation":context.installation,"id":id,"version":version.to_string()}),
            ),
        )
        .await
        {
            Ok(_) => {}
            // A business view can precede the server's durable projection.
            Err(Error::NotFound | Error::InvalidInput(_)) => continue,
            Err(error) => return Err(error),
        }
        h.with(|c| {
            c.with_notification_store(&account, |s| {
                s.set_meta(&format!("notification:observed-ack:{id}"), &version)
            })
        })
        .await?;
    }
    Ok(())
}

/// Revoke with the restricted receiver even after sign-out discarded signing keys.
pub async fn revoke_receiver(h: &impl Hold, context: &ReceiveContext) -> Result<()> {
    let result = receive(
        h,
        context,
        "revoke",
        Some(json!({"installation":context.installation,"expected_revision":context.revision})),
    )
    .await;
    if !matches!(
        result,
        Ok(_) | Err(Error::AuthRequired | Error::NotFound | Error::DeviceNotApproved)
    ) {
        result?;
    }
    let account = account_id(&context.account)?;
    h.with(|c| {
        // Errors such as AUTH_REQUIRED can race a successful replacement lease.
        // Retire only the exact receiver this request tried to revoke.
        ensure(
            c.with_notification_store(&account, |s| s.meta::<ReceiveContext>(RECEIVE))?
                .as_ref()
                == Some(context),
            Error::VersionConflict,
        )?;
        if let Some(secure) = c.secure() {
            secure.delete(&c.notification_secret_name(&context.account))?;
        }
        c.with_notification_store(&account, |s| {
            s.atomic(|s| {
                let mut preference = settings(s)?;
                if !preference.enabled {
                    preference.remote_revision = preference.revision;
                    s.set_meta("notification:settings", &preference)?;
                }
                s.delete_meta(SYNC)?;
                s.delete_meta(RECEIVE)?;
                s.delete_meta(CREDENTIAL)?;
                s.delete_meta("notification:revoke_pending")
            })
        })
    })
    .await
}

/// Resolve only an opaque original-installation route; no arbitrary URL is accepted.
pub async fn resolve(h: &impl Hold, context: &ReceiveContext, route: &str) -> Result<Value> {
    ensure(
        route.len() == 128 && route.starts_with(&context.installation) && valid_hash(&route[64..]),
        Error::InvalidInput("notification route".into()),
    )?;
    let result = receive(h, context, &format!("routes/{route}"), None).await?;
    ensure(result["account"] == context.account, Error::IntegrityFailed)?;
    serde_json::from_value::<Target>(result["target"].clone())
        .map_err(|_| Error::IntegrityFailed)?;
    Ok(result)
}

/// Synchronize local owner edits with explicit CAS. A conflict retains the
/// draft and stops retries until another user save changes its local revision.
pub async fn sync_settings(h: &impl Hold, context: &ReceiveContext) -> Result<()> {
    let h = Unlocked::new(h).await?;
    let account = account_id(&context.account)?;
    for global in [true, false] {
        let pending = h
            .with(|c| {
                ensure(c.account()?.id == account, Error::Locked)?;
                c.notification_credential(context)?;
                let runtime = Runtime::load(&c.notification_runtime_path(), now_ms())?;
                let (revision, acked, expected, blocked, mut value) = if global {
                    (
                        runtime.settings.revision,
                        runtime.remote_revision,
                        runtime.remote_expected,
                        runtime.remote_blocked,
                        serde_json::to_value(runtime.settings),
                    )
                } else {
                    let settings = settings(&c.workspace.store)?;
                    let state = c
                        .workspace
                        .store
                        .meta::<SyncState>(SYNC)?
                        .unwrap_or_default();
                    (
                        settings.revision,
                        settings.remote_revision,
                        state.expected,
                        state.blocked,
                        serde_json::to_value(settings),
                    )
                };
                if revision <= acked || blocked == Some(revision) {
                    return Ok(None);
                }
                let value = value.as_mut().map_err(|_| Error::IntegrityFailed)?;
                value
                    .as_object_mut()
                    .ok_or(Error::IntegrityFailed)?
                    .remove("revision");
                value
                    .as_object_mut()
                    .ok_or(Error::IntegrityFailed)?
                    .remove("remote_revision");
                Ok(Some((revision, expected, value.clone())))
            })
            .await?;
        let Some((revision, expected, value)) = pending else {
            continue;
        };
        let scope = if global { "global" } else { "account" };
        let input = json!({"scope":scope,"installation":context.installation,"expected_revision":expected,"value":value});
        let result = h
            .send(|c| {
                let state = c.fresh_state().ok_or(Error::PolicyStale)?;
                c.relay().prepare_raw(
                    &c.signer(state)?,
                    "/v1/notifications/settings",
                    &input,
                    now_ms(),
                )
            })
            .await;
        let (server_revision, acknowledged) = match result {
            Ok(value) => (
                value["revision"].as_u64().ok_or(Error::IntegrityFailed)?,
                true,
            ),
            Err(Error::VersionConflict | Error::ExecutionUnknown) => {
                let state = h
                    .get(&format!(
                        "/v1/notifications/settings?installation={}",
                        context.installation
                    ))
                    .await?;
                (
                    state[scope]["revision"]
                        .as_u64()
                        .ok_or(Error::IntegrityFailed)?,
                    state[scope]["value"] == value,
                )
            }
            Err(error) => return Err(error),
        };
        h.with(|c| {
            c.notification_credential(context)?;
            if global {
                let path = c.notification_runtime_path();
                let mut runtime = Runtime::load(&path, now_ms())?;
                runtime.remote_expected = server_revision;
                if acknowledged {
                    runtime.remote_revision = revision;
                    runtime.remote_blocked = None;
                } else {
                    runtime.remote_blocked = Some(runtime.settings.revision);
                }
                runtime.save(&path)
            } else {
                let mut state = c
                    .workspace
                    .store
                    .meta::<SyncState>(SYNC)?
                    .unwrap_or_default();
                state.expected = server_revision;
                state.blocked = (!acknowledged).then_some(settings(&c.workspace.store)?.revision);
                if acknowledged {
                    let mut settings = settings(&c.workspace.store)?;
                    settings.remote_revision = revision;
                    c.workspace
                        .store
                        .set_meta("notification:settings", &settings)?;
                }
                c.workspace.store.set_meta(SYNC, &state)
            }
        })
        .await?;
        if !acknowledged {
            return Err(Error::VersionConflict);
        }
    }
    Ok(())
}

/// Apply an explicit device-local snooze to this installation's remote owner.
pub async fn sync_snoozes(h: &impl Hold, context: &ReceiveContext) -> Result<()> {
    let h = Unlocked::new(h).await?;
    let pending = h
        .with(|c| {
            ensure(
                c.account()?.id.to_string() == context.account,
                Error::Locked,
            )?;
            let mut pending = Vec::new();
            let now = now_ms();
            for item in c.workspace.store.notification_items()? {
                let key = format!("notification:snooze:{}", item.id);
                if let Some((version, until)) = c.workspace.store.meta::<(u64, u64)>(&key)? {
                    if item.open && item.version == version && until > now {
                        pending.push((item.id, version, until));
                    } else {
                        c.workspace.store.delete_meta(&key)?;
                    }
                }
            }
            Ok(pending)
        })
        .await?;
    for (id, version, until) in pending {
        let body = json!({"installation":context.installation,"id":id,"version":version.to_string(),"until":until});
        h.send(|c| {
            let state = c.fresh_state().ok_or(Error::PolicyStale)?;
            c.relay().prepare_raw(
                &c.signer(state)?,
                "/v1/notifications/snooze",
                &body,
                now_ms(),
            )
        })
        .await?;
        h.with(|c| {
            let key = format!("notification:snooze:{id}");
            if c.workspace.store.meta::<(u64, u64)>(&key)? == Some((version, until)) {
                c.workspace.store.delete_meta(&key)?;
            }
            Ok(())
        })
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Shared(Mutex<Client>);

    impl Hold for Shared {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut self.0.lock().unwrap())
        }
    }

    fn credential(c: &Client) -> Credential {
        let mut preference = settings(&c.workspace.store).unwrap();
        if !preference.enabled {
            preference.enabled = true;
            super::super::configure(&c.workspace.store, preference.revision, preference).unwrap();
        }
        Credential {
            context: ReceiveContext {
                account: c.account().unwrap().id.to_string(),
                device: hex::encode(*c.workspace.meta.device_id),
                installation: "11".repeat(32),
                registration: "22".repeat(32),
                revision: 1,
                expires_at: now_ms() + 60000,
            },
            receiver: "33".repeat(32),
        }
    }

    #[test]
    fn notifications_registration_recreation_resets_only_its_account_acknowledgement() {
        let dir = crate::testing::TempDir::new();
        let c = crate::testing::unlocked(&dir.0, false, None);
        let mut credential = credential(&c);
        c.save_notification_credential(&credential).unwrap();
        let mut preference = settings(&c.workspace.store).unwrap();
        preference.remote_revision = preference.revision;
        c.workspace
            .store
            .set_meta("notification:settings", &preference)
            .unwrap();
        c.workspace
            .store
            .set_meta(
                SYNC,
                &SyncState {
                    expected: 4,
                    blocked: None,
                },
            )
            .unwrap();
        credential.context.revision += 1;
        c.save_notification_credential(&credential).unwrap();
        assert_eq!(
            settings(&c.workspace.store).unwrap().remote_revision,
            preference.revision
        );
        assert_eq!(
            c.workspace
                .store
                .meta::<SyncState>(SYNC)
                .unwrap()
                .unwrap()
                .expected,
            4
        );
        credential.context.registration = "77".repeat(32);
        credential.context.revision = 1;
        c.save_notification_credential(&credential).unwrap();
        assert_eq!(settings(&c.workspace.store).unwrap().remote_revision, 0);
        assert!(c.workspace.store.meta::<SyncState>(SYNC).unwrap().is_none());
        preference.enabled = false;
        super::super::configure(&c.workspace.store, preference.revision, preference).unwrap();
        c.save_notification_credential(&credential).unwrap();
        assert_eq!(
            c.workspace
                .store
                .meta::<bool>("notification:revoke_pending")
                .unwrap(),
            Some(true)
        );
    }

    #[tokio::test]
    async fn notifications_disabled_receiver_revokes_while_locked_then_can_register_again() {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let credential = credential(&c);
        let context = credential.context.clone();
        c.save_notification_credential(&credential).unwrap();
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        crate::testing::relay_to(&mut c, &format!("http://{}", server.local_addr().unwrap()));
        c.lock();
        let mut preference = settings(&c.workspace.store).unwrap();
        preference.enabled = false;
        super::super::configure(&c.workspace.store, preference.revision, preference).unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut bytes = vec![0; 8192];
            let n = socket.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..n]);
            assert!(request.starts_with("POST /v1/notifications/revoke?"));
            assert!(request.contains(&format!("Bearer {}", credential.receiver)));
            let body = json!({"ok":true,"data":{"removed":true}}).to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        assert!(!refresh(&c.hold(), &context).await.unwrap());
        task.await.unwrap();
        assert!(c.notification_receivers().unwrap().is_empty());
        let mut preference = settings(&c.workspace.store).unwrap();
        assert_eq!(preference.remote_revision, preference.revision);
        preference.enabled = true;
        super::super::configure(&c.workspace.store, preference.revision, preference).unwrap();
        assert!(
            c.workspace
                .store
                .meta::<bool>("notification:revoke_pending")
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn notifications_late_revoke_failure_cannot_delete_a_replacement_credential() {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let credential = credential(&c);
        let context = credential.context.clone();
        c.save_notification_credential(&credential).unwrap();
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        crate::testing::relay_to(&mut c, &format!("http://{}", server.local_addr().unwrap()));
        let shared = Arc::new(Shared(Mutex::new(c)));
        let during = shared.clone();
        let task = tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut bytes = vec![0; 8192];
            assert!(socket.read(&mut bytes).await.unwrap() > 0);
            let mut replacement = credential;
            replacement.context.revision += 1;
            replacement.receiver = "99".repeat(32);
            during
                .0
                .try_lock()
                .unwrap()
                .save_notification_credential(&replacement)
                .unwrap();
            let body =
                json!({"ok":false,"error":{"code":"AUTH_REQUIRED","message":"Receiver replaced"}})
                    .to_string();
            socket.write_all(format!("HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            replacement
        });
        assert_eq!(
            revoke_receiver(shared.as_ref(), &context).await,
            Err(Error::VersionConflict)
        );
        let replacement = task.await.unwrap();
        let c = shared.0.lock().unwrap();
        assert_eq!(
            c.notification_credential(&replacement.context)
                .unwrap()
                .receiver,
            replacement.receiver
        );
    }

    #[tokio::test]
    async fn notifications_removed_account_keeps_an_offline_revocation_without_its_workspace() {
        for secure in [false, true] {
            let dir = crate::testing::TempDir::new();
            let storage = secure.then(|| {
                Arc::new(crate::testing::MemoryStorage::default())
                    as Arc<dyn crate::secure::SecureStorage>
            });
            let mut c = crate::testing::unlocked(&dir.0, false, storage);
            let credential = credential(&c);
            let context = credential.context.clone();
            c.save_notification_credential(&credential).unwrap();
            let runtime_path = c.notification_runtime_path();
            c.remove_local_account(&context.account).unwrap();
            assert!(c.notification_receivers().unwrap().is_empty());
            assert!(c.session_state().account.is_none());
            assert_eq!(
                Runtime::load(&runtime_path, now_ms()).unwrap().revocations,
                vec![context.clone()]
            );
            assert!(
                !std::fs::read_to_string(&runtime_path)
                    .unwrap()
                    .contains(&credential.receiver)
            );

            let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = server.local_addr().unwrap();
            drop(server);
            crate::testing::relay_to(&mut c, &format!("http://{address}"));
            assert!(revoke_retired(&c.hold()).await.is_err());
            assert_eq!(
                Runtime::load(&runtime_path, now_ms())
                    .unwrap()
                    .revocations
                    .len(),
                1
            );
            let server = tokio::net::TcpListener::bind(address).await.unwrap();
            let task = tokio::spawn(async move {
                let (mut socket, _) = server.accept().await.unwrap();
                let mut bytes = vec![0; 8192];
                let n = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..n]);
                assert!(request.starts_with("POST /v1/notifications/revoke?"));
                assert!(request.contains(&format!("Bearer {}", credential.receiver)));
                let body = json!({"ok":true,"data":{"removed":true}}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
            revoke_retired(&c.hold()).await.unwrap();
            task.await.unwrap();
            assert!(
                Runtime::load(&runtime_path, now_ms())
                    .unwrap()
                    .revocations
                    .is_empty()
            );
            assert!(!c.retired_notification_path(&context).exists());
        }
    }

    #[test]
    fn notifications_installation_key_is_stable_and_receiver_is_separate_from_metadata() {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(
            &dir.0,
            false,
            Some(Arc::new(crate::testing::MemoryStorage::default())),
        );
        let before = c.installation_seed().unwrap();
        let credential = credential(&c);
        c.save_notification_credential(&credential).unwrap();
        assert!(
            !serde_json::to_string(&c.notification_receivers().unwrap())
                .unwrap()
                .contains(&credential.receiver)
        );
        assert!(
            c.workspace
                .store
                .meta::<Value>(CREDENTIAL)
                .unwrap()
                .is_none()
        );
        c.lock();
        c.select(crate::client::Slot::Account(
            "080g2081040g2081040g".parse().unwrap(),
        ))
        .unwrap();
        assert_eq!(*before, *c.installation_seed().unwrap());
        assert!(c.session_state().account.is_none());
        assert_eq!(
            c.notification_credential(&credential.context)
                .unwrap()
                .receiver,
            credential.receiver
        );
    }

    #[tokio::test]
    async fn notifications_inactive_feed_is_atomic_and_never_switches_the_active_workspace() {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let credential = credential(&c);
        let context = credential.context.clone();
        c.save_notification_credential(&credential).unwrap();
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        crate::testing::relay_to(&mut c, &format!("http://{}", server.local_addr().unwrap()));
        let mut notice = Item::new(
            context.account.clone(),
            Target::Inbox(Some("44".repeat(32))),
            1,
            now_ms(),
        );
        notice.unread = Some(1);
        let bad = Item::new(
            "080g2081040g2081040g".into(),
            Target::Recovery("55".repeat(32)),
            2,
            now_ms(),
        );
        let expected = notice.clone();
        let mut rebuilt = Item::new(
            context.account.clone(),
            Target::Inbox(Some("66".repeat(32))),
            1,
            now_ms(),
        );
        rebuilt.unread = Some(1);
        c.lock();
        c.select(crate::client::Slot::Account(
            "080g2081040g2081040g".parse().unwrap(),
        ))
        .unwrap();
        let shared = Arc::new(Shared(Mutex::new(c)));
        let receiver = credential.receiver.clone();
        let original = context.account.clone();
        let task = tokio::spawn(async move {
            for (after, body) in [
                (
                    0,
                    json!({"entries":[notice],"next_cursor":"1","complete":true,"gap":false}),
                ),
                (
                    1,
                    json!({"entries":[bad],"next_cursor":"2","complete":true,"gap":false}),
                ),
                (
                    1,
                    json!({"entries":[],"next_cursor":"0","complete":false,"gap":true}),
                ),
                (
                    0,
                    json!({"entries":[rebuilt],"next_cursor":"2","complete":false,"gap":false}),
                ),
                (
                    2,
                    json!({"entries":[],"next_cursor":"2","complete":true,"gap":false}),
                ),
            ] {
                let (mut socket, _) = server.accept().await.unwrap();
                let mut bytes = vec![0; 16384];
                let n = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..n]);
                assert!(
                    request.contains(&format!("account={original}"))
                        && request.contains(&format!("after={after}"))
                );
                assert!(request.contains(&format!("Bearer {receiver}")));
                assert!(!request.contains("x-dmsg-pop"));
                let body = json!({"ok":true,"data":body}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        assert!(!refresh(shared.as_ref(), &context).await.unwrap());
        assert_eq!(
            refresh(shared.as_ref(), &context).await,
            Err(Error::IntegrityFailed)
        );
        shared
            .with(|c| {
                c.with_notification_store(&account_id(&context.account).unwrap(), |s| {
                    assert_eq!(s.notification_item(&expected.id)?.unwrap().unread, Some(1));
                    assert_eq!(s.meta::<u64>("notification:cursor")?, Some(1));
                    assert_eq!(s.notification_items()?.len(), 1);
                    Ok(())
                })
            })
            .await
            .unwrap();
        assert!(refresh(shared.as_ref(), &context).await.unwrap());
        assert!(refresh(shared.as_ref(), &context).await.unwrap());
        shared
            .with(|c| {
                c.with_notification_store(&account_id(&context.account).unwrap(), |s| {
                    assert!(s.notification_item(&expected.id)?.unwrap().open);
                    Ok(())
                })
            })
            .await
            .unwrap();
        assert!(!refresh(shared.as_ref(), &context).await.unwrap());
        task.await.unwrap();
        shared
            .with(|c| {
                c.with_notification_store(&account_id(&context.account).unwrap(), |s| {
                    assert!(!s.notification_item(&expected.id)?.unwrap().open);
                    assert_eq!(s.notification_items()?.iter().filter(|i| i.open).count(), 1);
                    assert!(c.session_state().account.is_none());
                    Ok(())
                })
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn notifications_replaced_registration_rejects_a_late_page() {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let credential = credential(&c);
        let context = credential.context.clone();
        c.save_notification_credential(&credential).unwrap();
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        crate::testing::relay_to(&mut c, &format!("http://{}", server.local_addr().unwrap()));
        let shared = Arc::new(Shared(Mutex::new(c)));
        let during = shared.clone();
        let task = tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut bytes = vec![0; 8192];
            assert!(socket.read(&mut bytes).await.unwrap() > 0);
            // The client lock is available throughout HTTP. Replace the lease while waiting.
            {
                let c = during.0.try_lock().unwrap();
                let mut replacement = credential.context;
                replacement.revision += 1;
                c.workspace.store.set_meta(RECEIVE, &replacement).unwrap();
            }
            let body = json!({"ok":true,"data":{"entries":[],"next_cursor":"9","complete":true,"gap":false}}).to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        assert_eq!(
            refresh(shared.as_ref(), &context).await,
            Err(Error::VersionConflict)
        );
        task.await.unwrap();
        assert!(
            shared
                .0
                .lock()
                .unwrap()
                .workspace
                .store
                .meta::<u64>("notification:cursor")
                .unwrap()
                .is_none()
        );
    }
}
