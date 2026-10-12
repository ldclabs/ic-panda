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
//! under the LDK and checked here, never synced.

use crate::agent::agent_id;
use crate::client::Client;
use crate::codec::canonical;
use crate::crypto::{random_id, signing_public, Key};
use crate::now_ms;
use crate::vault::Item;
use agent_protocols::identity::AgentSigner;
use dmsg_types::{ensure, ensure_valid, Error, Result, DAY};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const AGENTS: &str = "agents";
const GRANTS: &str = "agent-grants";
/// Most Agent IDs one device hosts.
pub const MAX_AGENTS: usize = 16;
/// Most capability grants kept, revoked ones included.
pub const MAX_GRANTS: usize = 64;

/// What an agent may be granted (plan §6.9). A `:<target>` names a vault
/// tag or a channel ID; `*` is every target and an explicit owner choice.
/// Account, device and root changes, payments, names, principal changes,
/// signing and the grants themselves are never grantable.
pub const CAPABILITIES: [&str; 8] = [
    "account.read",
    "vault.list",
    "vault.read:",
    "channel.read:",
    "channel.publish:",
    "inbox.read",
    "profile.read",
    "attest.request",
];

/// What every agent may do without a grant.
pub const DEFAULT_CAPABILITIES: [&str; 1] = ["profile.read"];

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
        Some(_) => capability.strip_prefix(c).is_some_and(|target| {
            (1..=128).contains(&target.len()) && !target.contains(char::is_whitespace)
        }),
        None => capability == *c,
    });
    ensure_valid(known, "capability")
}

/// Whether `granted` covers `needed`: equal, or `<name>:*` for any target.
pub fn covers(granted: &str, needed: &str) -> bool {
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
        let item = Item {
            kind: "key".into(),
            title: format!("Agent ID {label}"),
            tags: vec!["dmsg-agent".into()],
            body: format!("Agent Protocols identity \"{label}\" of {account}."),
            username: String::new(),
            secret: String::new(),
            url: String::new(),
            favorite: false,
            created_at: now,
            updated_at: now,
            file: None,
            resolved_conflicts: None,
        };
        let seed = self.vault_key(&agent_item(&account, label), Some(item))?;
        let entry = AgentEntry {
            label: label.into(),
            agent_id: agent_id(&signing_public(&seed)),
            resident: false,
            credential: None,
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

    /// Keep a copy of an Agent ID's seed in the credential store so it
    /// signs while locked, or delete the copy.
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
        if on {
            let seed = self.agent_vault_seed(&entry)?;
            secure.set(&slot, seed.as_slice())?;
        } else {
            secure.delete(&slot)?;
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
        self.agent_update(&entry)?;
        Ok(credential)
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

    /// Every capability grant, revoked ones included.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn capability_grants(&self) -> Result<Vec<CapabilityGrant>> {
        Ok(self
            .workspace
            .private(self.keys()?, GRANTS)?
            .unwrap_or_default())
    }

    /// Grant a hosted Agent ID capabilities, for `days` or until revoked.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an unknown capability and
    /// `Error::NotFound` for an Agent ID this device does not host.
    pub fn capability_grant(
        &mut self,
        grantee: &str,
        capabilities: Vec<String>,
        days: Option<u64>,
    ) -> Result<CapabilityGrant> {
        let grantee = self.agent_find(grantee)?.agent_id;
        ensure_valid(
            !capabilities.is_empty() && capabilities.len() <= 16,
            "capabilities",
        )?;
        for capability in &capabilities {
            validate_capability(capability)?;
        }
        ensure_valid(days.is_none_or(|d| (1..=366).contains(&d)), "days")?;
        let mut grants = self.capability_grants()?;
        ensure(grants.len() < MAX_GRANTS, Error::QuotaExceeded)?;
        let now = now_ms();
        let grant = CapabilityGrant {
            id: hex::encode(&random_id()[..16]),
            grantee,
            capabilities,
            created_at: now,
            expires_at: days.map(|d| now + d * DAY),
            revoked_at: None,
        };
        grants.push(grant.clone());
        self.workspace.put_private(self.keys()?, GRANTS, &grants)?;
        Ok(grant)
    }

    /// Revoke a capability grant.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown grant.
    pub fn capability_revoke(&mut self, id: &str) -> Result<CapabilityGrant> {
        let mut grants = self.capability_grants()?;
        let grant = grants
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or(Error::NotFound)?;
        grant.revoked_at.get_or_insert(now_ms());
        let revoked = grant.clone();
        self.workspace.put_private(self.keys()?, GRANTS, &grants)?;
        Ok(revoked)
    }

    /// Whether `grantee` may do `needed`.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` without a live grant covering it and
    /// `Error::Locked` while locked.
    pub fn capability_check(&self, grantee: &str, needed: &str) -> Result<()> {
        if DEFAULT_CAPABILITIES.contains(&needed) {
            return Ok(());
        }
        let now = now_ms();
        let granted = self.capability_grants()?.iter().any(|g| {
            g.grantee == grantee && g.live(now) && g.capabilities.iter().any(|c| covers(c, needed))
        });
        ensure(granted, Error::Forbidden)
    }
}
