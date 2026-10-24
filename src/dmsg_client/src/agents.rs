//! The Agent IDs this device hosts (plan §6.8–§6.9): their keys, the
//! credential an agent's MCP session presents to the daemon, and the
//! capabilities the owner grants them over this account's data.
//!
//! An Agent ID's key is a vault item whose ID its label determines, so the
//! same label on another device of the account brings the same Agent ID.
//! While the device is locked only a resident Agent ID can sign: its seed
//! is copied into the operating system's credential store, and turning
//! residency off deletes the copy. The list of hosted Agent IDs and the
//! digests of their credentials are public; capability grants are sealed
//! under the LDK and checked here, never synced. A resident Agent ID's live
//! grants are copied beside its seed, which it is checked against while
//! locked.
//!
//! The Agent Mail decryption keys of an Agent ID's mailboxes (Agent Mail
//! §4.3) come from its connector, which keeps none: they are sealed under
//! the LDK on this device, like the connector state they belong to, and a
//! resident Agent ID also keeps a copy beside its seed, which holds the
//! keys it made while the device was locked until the next unlocked call
//! folds them in.

use crate::agent::agent_id;
use crate::client::Client;
use crate::codec::canonical;
use crate::crypto::{Key, random_id, signing_public};
use crate::now_ms;
use crate::secure::SecureStorage;
use crate::vault::Item;
use agent_protocols::identity::{AgentSigner, service_origin};
use dmsg_types::{DAY, Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const AGENTS: &str = "agents";
const GRANTS: &str = "agent-grants";
/// Sealed record of an Agent ID's mail keys, by `did:agent:`.
const MAIL_KEYS: &str = "agent-mail-keys:";
/// Most Agent IDs one device hosts.
pub const MAX_AGENTS: usize = 16;
/// Most capability grants kept, revoked and expired ones included until a
/// new grant needs their room.
pub const MAX_GRANTS: usize = 64;

/// What an agent may be granted (plan §6.9). A `:<target>` names a vault
/// tag, a local use policy ID (`secret.use`), a library folder ID (`files.*`,
/// covering the folders under it) or a channel ID; `*` is every target and
/// an explicit owner choice. Account, device and root changes, payments,
/// names, principal changes, signing, backups, deletions, sharing and the
/// grants themselves are never grantable, and no vault capability reaches
/// dMsg's own keys (items tagged `dmsg-…`, see
/// [`crate::vault::Item::reserved`]). A team wallet capability names a
/// wallet canister: `wallet.propose` only proposes, which the wallet
/// accepts when the member lists the Agent ID, and `wallet.execute` only
/// carries out what members approved; no capability approves. The last
/// eight are the [`HOST_CAPABILITIES`].
pub const CAPABILITIES: [&str; 27] = [
    "account.read",
    "vault.list",
    "vault.read:",
    "vault.write:",
    "secret.use:",
    "files.read:",
    "files.write:",
    "channel.read:",
    "channel.publish:",
    "inbox.read",
    "inbox.contact",
    "profile.read",
    "attest.request",
    "action.request",
    "agent.delegation.read",
    "events.endpoint",
    "wallet.read",
    "wallet.propose:",
    "wallet.execute:",
    "agent.profile.read:",
    "agent.profile.write:",
    "discourse.read:",
    "discourse.write:",
    "mail.read:",
    "mail.write:",
    "knowledge.read:",
    "knowledge.write:",
];

/// The Agent Protocols services at the allowed hosts: reading and publishing the Agent
/// ID's profile, and reading from or writing to Agent Discourse, Agent Mail
/// and Agent Knowledge. The target is an origin, or `*` for every one.
pub const HOST_CAPABILITIES: [&str; 8] = [
    "agent.profile.read",
    "agent.profile.write",
    "discourse.read",
    "discourse.write",
    "mail.read",
    "mail.write",
    "knowledge.read",
    "knowledge.write",
];

/// What every agent may do without a grant.
pub const DEFAULT_CAPABILITIES: [&str; 1] = ["profile.read"];

/// The vocabulary as people type it: each capability, with the kind of
/// target it names after the colon.
pub fn vocabulary() -> Vec<String> {
    CAPABILITIES
        .iter()
        .map(|c| match c.strip_suffix(':') {
            None => c.to_string(),
            Some(name) => {
                let target = match name {
                    "vault.read" | "vault.write" => "tag",
                    "secret.use" => "policy",
                    "files.read" | "files.write" => "folder",
                    "channel.read" | "channel.publish" => "channel",
                    "wallet.propose" | "wallet.execute" => "wallet",
                    _ => "origin",
                };
                format!("{name}:<{target}>")
            }
        })
        .collect()
}

/// Mail decryption keys: public key to secret, both base64url.
type MailKeys = std::collections::BTreeMap<String, String>;

/// An Agent ID this device hosts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentEntry {
    /// Local name, `[a-z0-9_-]{1,40}`.
    pub label: String,
    /// `did:agent:…`.
    pub agent_id: String,
    /// Its seed has a copy in the credential store and signs while locked.
    pub resident: bool,
    /// SHA-256 of the agent's credential, hex.
    pub credential: Option<String>,
    /// When the credential was issued, Unix milliseconds.
    #[serde(default)]
    pub credential_at: Option<u64>,
    /// The owner paused it: its credential reaches nothing until resumed.
    #[serde(default)]
    pub paused: bool,
    /// Monotonic pause generation: resuming cannot revive old use requests.
    #[serde(default)]
    pub pause_generation: u64,
    /// Unix milliseconds.
    pub created_at: u64,
}

impl AgentEntry {
    /// The entry as clients see it: whether a credential exists, not its digest.
    pub fn view(&self) -> Json {
        json!({
            "label": self.label,
            "agent_id": self.agent_id,
            "resident": self.resident,
            "credential": self.credential.is_some(),
            "credential_at": self.credential_at,
            "paused": self.paused,
            "created_at": self.created_at,
        })
    }
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct Agents {
    active: Option<String>,
    pub(crate) list: Vec<AgentEntry>,
}

/// A capability grant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CapabilityGrant {
    /// Random ID, hex.
    pub id: String,
    /// The agent, `did:agent:…`.
    pub grantee: String,
    /// What it may do, from [`CAPABILITIES`].
    pub capabilities: Vec<String>,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Optional end, Unix milliseconds.
    pub expires_at: Option<u64>,
    /// When the owner revoked it.
    pub revoked_at: Option<u64>,
}

impl CapabilityGrant {
    fn live(&self, now: u64) -> bool {
        self.revoked_at.is_none() && self.expires_at.is_none_or(|t| t > now)
    }
}

/// The vault item ID of Agent ID `label` of `account`.
pub fn agent_item(account: &str, label: &str) -> String {
    hex::encode(Sha256::digest(canonical(&(
        "dmsg/agent-id/1",
        account,
        label,
    ))))
}

fn validate_label(label: &str) -> Result<()> {
    ensure_valid(
        (1..=40).contains(&label.len())
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b)),
        "label",
    )
}

/// Check a capability against the vocabulary.
///
/// # Errors
/// Returns `Error::InvalidInput` for anything else.
pub fn validate_capability(capability: &str) -> Result<()> {
    let known = CAPABILITIES.iter().any(|c| match c.strip_suffix(':') {
        Some(name) if HOST_CAPABILITIES.contains(&name) => {
            capability.strip_prefix(c).is_some_and(|target| {
                target == "*"
                    || (target.len() <= 128
                        && service_origin(target).is_ok_and(|origin| origin == target))
            })
        }
        Some("secret.use") => capability
            .strip_prefix(c)
            .is_some_and(|target| crate::secrets::hex_id(target, 32)),
        Some(_) => capability.strip_prefix(c).is_some_and(|target| {
            (1..=128).contains(&target.len())
                && !target.contains(char::is_whitespace)
                && !target.starts_with(crate::vault::RESERVED_TAG)
        }),
        None => capability == *c,
    });
    ensure_valid(known, "capability")
}

/// Check a grant's capabilities and lifetime: 1 to 16 capabilities from the
/// vocabulary, for 1 to 366 days or until revoked.
///
/// # Errors
/// Returns `Error::InvalidInput` for anything else.
pub fn check_grant(capabilities: &[String], days: Option<u64>) -> Result<()> {
    ensure_valid(
        !capabilities.is_empty() && capabilities.len() <= 16,
        "capabilities",
    )?;
    for capability in capabilities {
        validate_capability(capability)?;
    }
    ensure_valid(days.is_none_or(|d| (1..=366).contains(&d)), "days")
}

/// Whether granting `capability` deserves a second look: one naming every
/// target, a secret's value, or carrying out a wallet's transactions.
pub fn sensitive(capability: &str) -> bool {
    capability.ends_with(":*")
        || capability.starts_with("secret.use:")
        || capability.starts_with("wallet.execute:")
}

/// Whether `granted` covers `needed`: equal, or `<name>:*` for any target.
pub fn covers(granted: &str, needed: &str) -> bool {
    if needed.starts_with("vault.secret:") || granted.starts_with("vault.secret:") {
        return false;
    }
    if needed.starts_with("secret.use:") {
        return granted == needed && validate_capability(needed).is_ok();
    }
    granted == needed
        || granted
            .strip_suffix('*')
            .is_some_and(|prefix| prefix.ends_with(':') && needed.starts_with(prefix))
}

fn credential_digest(credential: &str) -> String {
    hex::encode(Sha256::digest(credential.as_bytes()))
}

impl Client {
    pub(crate) fn agents(&self) -> Result<Agents> {
        Ok(self.workspace.store.meta(AGENTS)?.unwrap_or_default())
    }

    fn save_agents(&self, agents: &Agents) -> Result<()> {
        self.workspace.store.set_meta(AGENTS, agents)
    }

    pub(crate) fn resident_name(&self, agent_id: &str) -> Result<String> {
        Ok(format!("{}:agent:{agent_id}", self.workspace.name()?))
    }

    fn resident_mail_name(&self, agent_id: &str) -> Result<String> {
        Ok(format!("{}:agent-mail:{agent_id}", self.workspace.name()?))
    }

    fn resident_grants_name(&self, agent_id: &str) -> Result<String> {
        Ok(format!(
            "{}:agent-grants:{agent_id}",
            self.workspace.name()?
        ))
    }

    /// Delete every copy a resident Agent ID keeps in the credential store:
    /// its grants first, so a copy never allows more than the grants do,
    /// then its Mail subscription key, mail decryption keys and seed.
    pub(crate) fn forget_resident(&self, secure: &dyn SecureStorage, agent_id: &str) -> Result<()> {
        secure.delete(&self.resident_grants_name(agent_id)?)?;
        self.mail_push_forget_resident(secure, agent_id)?;
        secure.delete(&self.resident_mail_name(agent_id)?)?;
        secure.delete(&self.resident_name(agent_id)?)
    }

    /// Copy the live ones of `grants` for resident Agent ID `agent_id`
    /// beside its seed.
    fn resident_grants_save(&self, agent_id: &str, grants: &[CapabilityGrant]) -> Result<()> {
        let secure = self
            .secure()
            .ok_or_else(|| Error::Unavailable("no system credential store".into()))?;
        let now = now_ms();
        let own: Vec<CapabilityGrant> = grants
            .iter()
            .filter(|g| g.grantee == agent_id && g.live(now))
            .cloned()
            .map(|mut g| {
                g.capabilities.retain(|c| !c.starts_with("secret.use:"));
                g
            })
            .filter(|g| !g.capabilities.is_empty())
            .collect();
        secure.set(
            &self.resident_grants_name(agent_id)?,
            &serde_json::to_vec(&own).expect("serializable"),
        )
    }

    /// The copy of a resident Agent ID's grants.
    fn resident_grants(&self, agent_id: &str) -> Result<Vec<CapabilityGrant>> {
        let entry = self.agent_find(agent_id)?;
        ensure(entry.resident, Error::Locked)?;
        let secure = self.secure().ok_or(Error::Locked)?;
        match secure.get(&self.resident_grants_name(&entry.agent_id)?)? {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| Error::IntegrityFailed),
            None => Ok(Vec::new()),
        }
    }

    fn sealed_mail_keys(&self, agent_id: &str) -> Result<MailKeys> {
        Ok(self
            .workspace
            .private(self.keys()?, &format!("{MAIL_KEYS}{agent_id}"))?
            .unwrap_or_default())
    }

    fn resident_mail_keys(&self, agent_id: &str) -> Result<MailKeys> {
        let Some(secure) = self.secure() else {
            return Ok(MailKeys::new());
        };
        match secure.get(&self.resident_mail_name(agent_id)?)? {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| Error::IntegrityFailed),
            None => Ok(MailKeys::new()),
        }
    }

    /// The hosted Agent IDs and the active one; readable while locked.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn agent_list(&self) -> Result<Json> {
        let agents = self.agents()?;
        Ok(json!({
            "active": agents.active,
            "agents": agents.list.iter().map(AgentEntry::view).collect::<Vec<_>>(),
        }))
    }

    /// The hosted Agent ID named by its label or its `did:agent:`.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown one.
    pub fn agent_find(&self, name: &str) -> Result<AgentEntry> {
        self.agents()?
            .list
            .into_iter()
            .find(|a| a.label == name || a.agent_id == name)
            .ok_or(Error::NotFound)
    }

    /// The active Agent ID, else the only one.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when none is chosen.
    pub fn agent_active(&self) -> Result<AgentEntry> {
        let agents = self.agents()?;
        match (&agents.active, agents.list.as_slice()) {
            (Some(active), _) => self.agent_find(active),
            (None, [only]) => Ok(only.clone()),
            _ => Err(Error::NotFound),
        }
    }

    /// Host Agent ID `label`, creating its key in the vault unless another
    /// device of the account already did.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::QuotaExceeded`
    /// beyond [`MAX_AGENTS`].
    pub fn agent_create(&mut self, label: &str) -> Result<AgentEntry> {
        validate_label(label)?;
        if let Ok(entry) = self.agent_find(label) {
            return Ok(entry);
        }
        let mut agents = self.agents()?;
        ensure(agents.list.len() < MAX_AGENTS, Error::QuotaExceeded)?;
        let account = self.account()?.id.to_string();
        let now = now_ms();
        let mut item = Item::new("key", &format!("Agent ID {label}"), now);
        item.tags = vec!["dmsg-agent".into()];
        item.notes = format!("Agent Protocols identity \"{label}\" of {account}.");
        let seed = self.vault_key(&agent_item(&account, label), Some(item))?;
        let entry = AgentEntry {
            label: label.into(),
            agent_id: agent_id(&signing_public(&seed)),
            resident: false,
            credential: None,
            credential_at: None,
            paused: false,
            pause_generation: 0,
            created_at: now,
        };
        agents.list.push(entry.clone());
        self.save_agents(&agents)?;
        Ok(entry)
    }

    /// Make an Agent ID the one the CLI's `ap` commands use by default.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown one.
    pub fn agent_use(&mut self, name: &str) -> Result<AgentEntry> {
        let entry = self.agent_find(name)?;
        let mut agents = self.agents()?;
        agents.active = Some(entry.agent_id.clone());
        self.save_agents(&agents)?;
        Ok(entry)
    }

    fn agent_update(&self, entry: &AgentEntry) -> Result<()> {
        let mut agents = self.agents()?;
        for a in &mut agents.list {
            if a.agent_id == entry.agent_id {
                *a = entry.clone();
            }
        }
        self.save_agents(&agents)
    }

    /// Keep an Agent ID's seed, mail keys, subscription key and grants in the
    /// credential store so it works while locked, or delete the copies.
    ///
    /// # Errors
    /// Returns `Error::Locked` to turn it on while locked and
    /// `Error::Unavailable` without a system credential store.
    pub fn agent_resident(&mut self, name: &str, on: bool) -> Result<AgentEntry> {
        let mut entry = self.agent_find(name)?;
        let secure = self
            .secure()
            .ok_or_else(|| Error::Unavailable("no system credential store".into()))?;
        let slot = self.resident_name(&entry.agent_id)?;
        let mail_slot = self.resident_mail_name(&entry.agent_id)?;
        if on {
            let seed = self.agent_vault_seed(&entry)?;
            secure.set(&slot, seed.as_slice())?;
            let keys = self.sealed_mail_keys(&entry.agent_id)?;
            secure.set(
                &mail_slot,
                &Zeroizing::new(serde_json::to_vec(&keys).expect("serializable")),
            )?;
            self.resident_grants_save(&entry.agent_id, &self.capability_grants()?)?;
            self.mail_push_keep_resident(&entry.agent_id)?;
        } else {
            // Keys made while locked live only in the copy: fold them in first.
            let resident = self.resident_mail_keys(&entry.agent_id)?;
            if !resident.is_empty() {
                let mut keys = self.sealed_mail_keys(&entry.agent_id)?;
                keys.extend(resident);
                self.workspace.put_private(
                    self.keys()?,
                    &format!("{MAIL_KEYS}{}", entry.agent_id),
                    &keys,
                )?;
            }
            self.forget_resident(secure.as_ref(), &entry.agent_id)?;
        }
        entry.resident = on;
        self.agent_update(&entry)?;
        Ok(entry)
    }

    /// Issue a new credential for an Agent ID's MCP sessions, replacing the
    /// old one. Only its digest is kept; the credential is shown once.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown Agent ID.
    pub fn agent_credential(&mut self, name: &str) -> Result<String> {
        let mut entry = self.agent_find(name)?;
        let credential = format!("dmsg-agent-{}", hex::encode(random_id()));
        entry.credential = Some(credential_digest(&credential));
        entry.credential_at = Some(now_ms());
        self.agent_update(&entry)?;
        self.secret_abort(Some(&entry.agent_id), None);
        Ok(credential)
    }

    /// Withdraw an Agent ID's credential without issuing another: its MCP
    /// sessions reach nothing from their next call.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown Agent ID.
    pub fn agent_credential_revoke(&mut self, name: &str) -> Result<AgentEntry> {
        let mut entry = self.agent_find(name)?;
        entry.credential = None;
        entry.credential_at = None;
        self.agent_update(&entry)?;
        self.secret_abort(Some(&entry.agent_id), None);
        Ok(entry)
    }

    /// Pause an Agent ID, or resume it: while paused its credential reaches
    /// nothing, whatever its grants.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown Agent ID.
    pub fn agent_pause(&mut self, name: &str, paused: bool) -> Result<AgentEntry> {
        let mut entry = self.agent_find(name)?;
        if paused && !entry.paused {
            entry.pause_generation = entry
                .pause_generation
                .checked_add(1)
                .ok_or(Error::QuotaExceeded)?;
        }
        entry.paused = paused;
        self.agent_update(&entry)?;
        if paused {
            self.secret_abort(Some(&entry.agent_id), None);
        }
        Ok(entry)
    }

    /// Stop hosting an Agent ID on this device: its grants are revoked, its
    /// credential and resident copies go, and so do its mail keys. Its key
    /// stays in the vault, where other devices of the account may host it;
    /// the same label brings it back. The caller forgets its connector
    /// state.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` for an
    /// unknown Agent ID.
    pub fn agent_remove(&mut self, via: &str, name: &str) -> Result<AgentEntry> {
        let entry = self.agent_find(name)?;
        let keys = self.keys()?;
        let now = now_ms();
        let mut grants = self.capability_grants()?;
        let mut revoked = Vec::new();
        for grant in grants
            .iter_mut()
            .filter(|g| g.grantee == entry.agent_id && g.live(now))
        {
            grant.revoked_at = Some(now);
            revoked.push(grant.clone());
        }
        if entry.resident
            && let Some(secure) = self.secure()
        {
            self.forget_resident(secure.as_ref(), &entry.agent_id)?;
        }
        self.workspace.put_private(keys, GRANTS, &grants)?;
        // Once kept, so the record never names a revocation that failed.
        for grant in &revoked {
            self.audit_grant(via, "grants/revoke", grant);
        }
        self.workspace
            .delete_private(&format!("{MAIL_KEYS}{}", entry.agent_id))?;
        let mut agents = self.agents()?;
        agents.list.retain(|a| a.agent_id != entry.agent_id);
        if agents.active.as_deref() == Some(entry.agent_id.as_str()) {
            agents.active = None;
        }
        self.save_agents(&agents)?;
        self.audit(
            "owner",
            via,
            "agent/remove",
            Some(&entry.agent_id),
            None,
            "ok",
        );
        self.secret_abort(Some(&entry.agent_id), None);
        Ok(entry)
    }

    /// The Agent ID whose credential this is; readable while locked.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn agent_by_credential(&self, credential: &str) -> Result<Option<AgentEntry>> {
        if !credential.starts_with("dmsg-agent-") {
            return Ok(None);
        }
        let digest = credential_digest(credential);
        Ok(self
            .agents()?
            .list
            .into_iter()
            .find(|a| a.credential.as_deref() == Some(digest.as_str())))
    }

    fn agent_vault_seed(&mut self, entry: &AgentEntry) -> Result<Zeroizing<Key>> {
        let account = self.account()?.id.to_string();
        let seed = self.vault_key(&agent_item(&account, &entry.label), None)?;
        ensure(
            agent_id(&signing_public(&seed)) == entry.agent_id,
            Error::IntegrityFailed,
        )?;
        Ok(seed)
    }

    /// The signer of a hosted Agent ID: from the vault while unlocked, else
    /// from its resident copy.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked for an Agent ID that is not resident.
    pub fn agent_signer(&mut self, agent: &str) -> Result<AgentSigner> {
        let entry = self.agent_find(agent)?;
        let seed = match (self.keys.is_some(), entry.resident) {
            (true, _) => self.agent_vault_seed(&entry)?,
            (false, true) => {
                let secure = self.secure().ok_or(Error::Locked)?;
                let copy = secure
                    .get(&self.resident_name(&entry.agent_id)?)?
                    .ok_or(Error::Locked)?;
                let seed: Key = copy
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::IntegrityFailed)?;
                Zeroizing::new(seed)
            }
            (false, false) => return Err(Error::Locked),
        };
        let signer = AgentSigner::from_seed(*seed);
        ensure(
            signer.agent_id().as_str() == entry.agent_id,
            Error::IntegrityFailed,
        )?;
        Ok(signer)
    }

    /// The mail decryption keys of a hosted Agent ID's mailboxes, as its
    /// connector restores them (`{version, agent_id, keys}`): the sealed
    /// ones while unlocked, and a resident Agent ID's copy.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked for an Agent ID that is not resident.
    pub fn agent_mail_keys(&mut self, agent: &str) -> Result<Json> {
        let entry = self.agent_find(agent)?;
        let mut keys = match (self.keys.is_some(), entry.resident) {
            (true, _) => self.sealed_mail_keys(&entry.agent_id)?,
            (false, true) => MailKeys::new(),
            (false, false) => return Err(Error::Locked),
        };
        if entry.resident {
            keys.extend(self.resident_mail_keys(&entry.agent_id)?);
        }
        Ok(json!({"version": 1, "agent_id": entry.agent_id, "keys": keys}))
    }

    /// Keep the mail keys a connector call left, which drop those of cards
    /// past `receive_until`: sealed while unlocked, and in a resident Agent
    /// ID's copy.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked for an Agent ID that is not
    /// resident and `Error::IntegrityFailed` for keys of another Agent ID.
    pub fn agent_mail_keys_save(&mut self, agent: &str, value: &Json) -> Result<()> {
        let entry = self.agent_find(agent)?;
        ensure(
            value["version"] == 1 && value["agent_id"] == entry.agent_id.as_str(),
            Error::IntegrityFailed,
        )?;
        let keys: MailKeys =
            serde_json::from_value(value["keys"].clone()).map_err(|_| Error::IntegrityFailed)?;
        ensure(self.keys.is_some() || entry.resident, Error::Locked)?;
        if self.keys.is_some() {
            self.workspace.put_private(
                self.keys()?,
                &format!("{MAIL_KEYS}{}", entry.agent_id),
                &keys,
            )?;
        }
        if entry.resident {
            let secure = self.secure().ok_or(Error::Locked)?;
            secure.set(
                &self.resident_mail_name(&entry.agent_id)?,
                &Zeroizing::new(serde_json::to_vec(&keys).expect("serializable")),
            )?;
        }
        Ok(())
    }

    /// Every capability grant kept, revoked and expired ones included.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn capability_grants(&self) -> Result<Vec<CapabilityGrant>> {
        Ok(self
            .workspace
            .private(self.keys()?, GRANTS)?
            .unwrap_or_default())
    }

    /// Grant a hosted Agent ID capabilities, for `days` or until revoked;
    /// the owner's action through `via` enters the audit record.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an unknown capability and
    /// `Error::NotFound` for an Agent ID this device does not host.
    pub fn capability_grant(
        &mut self,
        via: &str,
        grantee: &str,
        capabilities: Vec<String>,
        days: Option<u64>,
    ) -> Result<CapabilityGrant> {
        let id = hex::encode(&random_id()[..16]);
        self.capability_grant_as(via, id, grantee, capabilities, days)
    }

    /// [`Self::capability_grant`] under a given grant ID; a grant already
    /// kept under it is returned as it is, so an approved request that
    /// runs again grants once.
    pub(crate) fn capability_grant_as(
        &mut self,
        via: &str,
        id: String,
        grantee: &str,
        capabilities: Vec<String>,
        days: Option<u64>,
    ) -> Result<CapabilityGrant> {
        let grantee = self.agent_find(grantee)?.agent_id;
        check_grant(&capabilities, days)?;
        let mut grants = self.capability_grants()?;
        if let Some(kept) = grants.iter().find(|g| g.id == id) {
            return Ok(kept.clone());
        }
        let now = now_ms();
        // The audit record keeps what the revoked and expired ones were.
        if grants.len() >= MAX_GRANTS {
            grants.retain(|g| g.live(now));
        }
        ensure(grants.len() < MAX_GRANTS, Error::QuotaExceeded)?;
        let grant = CapabilityGrant {
            id,
            grantee,
            capabilities,
            created_at: now,
            expires_at: days.map(|d| now + d * DAY),
            revoked_at: None,
        };
        grants.push(grant.clone());
        self.workspace.put_private(self.keys()?, GRANTS, &grants)?;
        self.audit_grant(via, "grants/add", &grant);
        // After the grants, so the copy never allows more than they do.
        self.grants_changed(&grant.grantee, &grants)?;
        Ok(grant)
    }

    /// An Agent ID asks for capabilities: an approval request the owner
    /// decides within [`crate::approval::AGENT_REQUEST_TTL_MS`], which
    /// grants them once approved.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an unknown capability or lifetime,
    /// `Error::Locked` while locked and `Error::QuotaExceeded` while the
    /// agent's earlier requests fill its share of the queue.
    pub fn capability_request(
        &mut self,
        grantee: &str,
        capabilities: Vec<String>,
        days: Option<u64>,
    ) -> Result<Json> {
        let grantee = self.agent_find(grantee)?.agent_id;
        check_grant(&capabilities, days)?;
        let now = now_ms();
        let request = self.approval_create_at(
            &format!("agent:{grantee}"),
            crate::approval::Action::Grant {
                grantee: grantee.clone(),
                capabilities,
                days,
            },
            now,
            now + crate::approval::AGENT_REQUEST_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// Renew the copy of a resident grantee's grants from `grants`.
    fn grants_changed(&self, grantee: &str, grants: &[CapabilityGrant]) -> Result<()> {
        match self.agent_find(grantee) {
            Ok(entry) if entry.resident => self.resident_grants_save(&entry.agent_id, grants),
            _ => Ok(()),
        }
    }

    fn audit_grant(&self, via: &str, action: &str, grant: &CapabilityGrant) {
        let detail = format!("{} {}", grant.grantee, grant.capabilities.join(","));
        self.audit("owner", via, action, Some(&grant.id), Some(&detail), "ok");
    }

    /// Revoke a capability grant; the owner's action through `via` enters
    /// the audit record.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown grant.
    pub fn capability_revoke(&mut self, via: &str, id: &str) -> Result<CapabilityGrant> {
        let mut grants = self.capability_grants()?;
        let grant = grants
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or(Error::NotFound)?;
        grant.revoked_at.get_or_insert(now_ms());
        let revoked = grant.clone();
        // Before the grants, so the copy never allows more than they do.
        self.grants_changed(&revoked.grantee, &grants)?;
        self.workspace.put_private(self.keys()?, GRANTS, &grants)?;
        self.audit_grant(via, "grants/revoke", &revoked);
        self.secret_abort(Some(&revoked.grantee), None);
        Ok(revoked)
    }

    /// Whether `grantee` may do `needed`: checked against the sealed grants,
    /// or while locked against a resident Agent ID's copy.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` without a live grant covering it and
    /// `Error::Locked` while locked for an Agent ID that is not resident.
    pub fn capability_check(&self, grantee: &str, needed: &str) -> Result<()> {
        self.capability_check_at(grantee, needed, now_ms())
    }

    /// Check using the synchronous execution step's single time reading.
    pub(crate) fn capability_check_at(&self, grantee: &str, needed: &str, now: u64) -> Result<()> {
        if needed.starts_with("vault.secret:") {
            return Err(Error::Forbidden);
        }
        if needed.starts_with("secret.use:") {
            self.keys()?;
        }
        if DEFAULT_CAPABILITIES.contains(&needed) {
            return Ok(());
        }
        let granted = self
            .capability_live_at(grantee, now)?
            .iter()
            .any(|g| g.capabilities.iter().any(|c| covers(c, needed)));
        ensure(granted, Error::Forbidden)
    }

    /// The `targets` for which `grantee` may `name`, such as the origins of
    /// a [`HOST_CAPABILITIES`] name.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked for an Agent ID that is not resident.
    pub fn capability_targets(
        &self,
        grantee: &str,
        name: &str,
        targets: Vec<String>,
    ) -> Result<Vec<String>> {
        let grants = self.capability_live(grantee)?;
        Ok(targets
            .into_iter()
            .filter(|target| {
                let needed = format!("{name}:{target}");
                grants
                    .iter()
                    .any(|g| g.capabilities.iter().any(|c| covers(c, &needed)))
            })
            .collect())
    }

    /// `grantee`'s live grants, from the sealed ones or while locked from a
    /// resident Agent ID's copy.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked for an Agent ID that is not resident.
    pub fn capability_live(&self, grantee: &str) -> Result<Vec<CapabilityGrant>> {
        self.capability_live_at(grantee, now_ms())
    }

    pub(crate) fn capability_live_at(
        &self,
        grantee: &str,
        now: u64,
    ) -> Result<Vec<CapabilityGrant>> {
        let grants = match self.keys {
            Some(_) => self.capability_grants()?,
            None => self.resident_grants(grantee)?,
        };
        Ok(grants
            .into_iter()
            .filter(|g| g.grantee == grantee && g.live(now))
            .collect())
    }
}
