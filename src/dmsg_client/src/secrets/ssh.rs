//! SSH wire validation and device-local key bindings. Private keys never leave core.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use signature::{SignatureEncoding, Signer, Verifier};

const BINDINGS: &str = "secret-ssh-bindings";
/// Maximum wire frame, excluding its four-byte length.
pub const MAX_FRAME: usize = 256 * 1024;

/// SSH length-prefixed string, with bounded checked slicing.
pub fn take<'a>(data: &mut &'a [u8]) -> Result<&'a [u8]> {
    let size = u32::from_be_bytes(
        data.get(..4)
            .and_then(|v| v.try_into().ok())
            .ok_or(Error::Forbidden)?,
    ) as usize;
    let value = data
        .get(4..4usize.checked_add(size).ok_or(Error::Forbidden)?)
        .ok_or(Error::Forbidden)?;
    *data = &data[4 + size..];
    Ok(value)
}

/// Encode one public wire string.
pub fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// The verified remote host/session of an unforwarded connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionBinding {
    host: Vec<u8>,
    session: Vec<u8>,
    fingerprint: String,
}

/// Verify session-bind@openssh.com. A transport permits this only once per connection.
pub fn session_bind(frame: &[u8]) -> Result<SessionBinding> {
    ensure(frame.len() <= MAX_FRAME, Error::QuotaExceeded)?;
    let mut data = frame;
    ensure(
        take(&mut data)? == b"session-bind@openssh.com",
        Error::Forbidden,
    )?;
    let host = take(&mut data)?;
    let session = take(&mut data)?;
    let signature = take(&mut data)?;
    ensure(
        data == [0]
            && (16..=128).contains(&session.len())
            && host.len() <= 2048
            && signature.len() <= 2048,
        Error::Forbidden,
    )?;
    let mut fields = host;
    if take(&mut fields)? == b"ssh-rsa" {
        let exponent = take(&mut fields)?;
        let modulus = take(&mut fields)?;
        let modulus = modulus.strip_prefix(&[0]).unwrap_or(modulus);
        let bits = modulus.len() * 8 - modulus.first().map_or(0, |b| b.leading_zeros() as usize);
        ensure(
            exponent.len() <= 5 && (2048..=8192).contains(&bits) && fields.is_empty(),
            Error::Forbidden,
        )?;
    }
    let key = ssh_key::PublicKey::from_bytes(host).map_err(|_| Error::Forbidden)?;
    // Certificate hosts require a separate certificate policy; do not silently trust them.
    let sig = ssh_key::Signature::try_from(signature).map_err(|_| Error::Forbidden)?;
    key.key_data()
        .verify(session, &sig)
        .map_err(|_| Error::Forbidden)?;
    Ok(SessionBinding {
        host: host.to_vec(),
        session: session.to_vec(),
        fingerprint: format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(host))),
    })
}

/// Verified operation context, with no guessed DNS name or commit contents.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Purpose {
    /// ssh_auth or git_sign.
    pub kind: String,
    /// Host key verified against session-bind, not a DNS assertion.
    pub host_fingerprint: Option<String>,
    /// Session commitment for display in the sealed approval.
    pub session_digest: Option<String>,
    /// Git namespace if present.
    pub namespace: Option<String>,
    /// Git digest algorithm if present.
    pub hash_algorithm: Option<String>,
    /// SSH account in the signed authentication packet.
    pub username: Option<String>,
}

/// Accept only complete SSH publickey authentication or Git SSHSIG messages.
pub fn purpose(bytes: &[u8], public: &[u8], session: Option<&SessionBinding>) -> Result<Purpose> {
    ensure(bytes.len() <= MAX_FRAME, Error::QuotaExceeded)?;
    let mut p = Purpose {
        kind: String::new(),
        host_fingerprint: None,
        session_digest: None,
        namespace: None,
        hash_algorithm: None,
        username: None,
    };
    if let Some(mut data) = bytes.strip_prefix(b"SSHSIG") {
        ensure(
            take(&mut data)? == b"git" && take(&mut data)?.is_empty(),
            Error::Forbidden,
        )?;
        let hash = take(&mut data)?;
        let digest = take(&mut data)?;
        ensure(
            data.is_empty()
                && ((hash == b"sha256" && digest.len() == 32)
                    || (hash == b"sha512" && digest.len() == 64)),
            Error::Forbidden,
        )?;
        p.kind = "git_sign".into();
        p.namespace = Some("git".into());
        p.hash_algorithm = Some(String::from_utf8_lossy(hash).into_owned());
    } else {
        let bound = session.ok_or(Error::Forbidden)?;
        let mut data = bytes;
        ensure(
            take(&mut data)? == bound.session && data.first() == Some(&50),
            Error::Forbidden,
        )?;
        data = &data[1..];
        let username = take(&mut data)?;
        ensure(
            !username.is_empty()
                && username.len() <= 256
                && username.iter().all(|b| !b.is_ascii_control()),
            Error::Forbidden,
        )?;
        ensure(take(&mut data)? == b"ssh-connection", Error::Forbidden)?;
        let method = take(&mut data)?;
        ensure(
            matches!(
                method,
                b"publickey" | b"publickey-hostbound-v00@openssh.com"
            ) && data.first() == Some(&1),
            Error::Forbidden,
        )?;
        data = &data[1..];
        ensure(
            take(&mut data)? == b"ssh-ed25519" && take(&mut data)? == public,
            Error::Forbidden,
        )?;
        if method == b"publickey-hostbound-v00@openssh.com" {
            ensure(take(&mut data)? == bound.host, Error::Forbidden)?;
        }
        ensure(data.is_empty(), Error::Forbidden)?;
        p.kind = "ssh_auth".into();
        p.host_fingerprint = Some(bound.fingerprint.clone());
        p.session_digest = Some(hex::encode(Sha256::digest(&bound.session)));
        p.username = Some(String::from_utf8(username.to_vec()).map_err(|_| Error::Forbidden)?);
    }
    Ok(p)
}

/// Enabled only after an owner configuration approval, never synchronized.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeyBinding {
    /// Random local identity.
    pub id: String,
    /// Revision at configuration time; changing key requires new approval.
    pub resolved: ResolvedSecret,
    /// Canonical OpenSSH public key.
    pub public_key: String,
    /// Public fingerprint.
    pub fingerprint: String,
    /// Explicit permitted signature domains.
    pub purposes: Vec<String>,
    /// False immediately refuses pending signatures.
    pub enabled: bool,
    /// Device binding.
    pub device_id: String,
}

/// OS-provided connection peer; unavailable PID remains absent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Peer {
    /// Real local effective UID checked against socket owner.
    pub uid: u32,
    /// OS-provided PID when supported; not a claimed application identity.
    pub pid: Option<u32>,
}

/// In-memory only: no raw private key or resumable connection token.
#[derive(Clone)]
pub(crate) struct Connection {
    peer: Peer,
    generation: u64,
    session: Option<SessionBinding>,
}

/// Immutable data bound to the original socket and exact key version.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignPlan {
    /// Exact enabled binding.
    pub binding: KeyBinding,
    /// Original connection, never reused on reconnect.
    pub connection: String,
    /// OS peer information.
    pub peer: Peer,
    /// Validated protocol purpose.
    pub purpose: Purpose,
    /// Commitment to exact bytes awaiting signature.
    pub data_digest: String,
    /// Daemon instance.
    pub boot_id: String,
    /// Unlock generation.
    pub session_generation: u64,
}

fn private(value: &str) -> Result<ssh_key::PrivateKey> {
    let key = ssh_key::PrivateKey::from_openssh(value).map_err(|_| failure("SSH_KEY_INVALID"))?;
    ensure(!key.is_encrypted(), failure("SSH_KEY_PASSPHRASE_REQUIRED"))?;
    ensure(
        key.algorithm() == ssh_key::Algorithm::Ed25519,
        failure("SSH_ALGORITHM_UNSUPPORTED"),
    )?;
    Ok(key)
}

impl Client {
    /// Current device bindings, without any private values.
    pub fn secret_ssh_bindings(&self) -> Result<Vec<KeyBinding>> {
        Ok(self
            .workspace
            .private(self.keys()?, BINDINGS)?
            .unwrap_or_default())
    }

    /// Candidate SSH entries: unsupported keys remain visible with a stable reason.
    pub fn secret_ssh_keys(&self) -> Result<Value> {
        let mut rows = Vec::new();
        for mut entry in self.vault_list(&crate::vault::Filter::default())? {
            if entry.item.kind == "ssh" && !entry.deleted && !entry.item.reserved() {
                let reference = SecretRef {
                    schema_version: 1,
                    account_id: self.account()?.id.to_string(),
                    item_id: entry.id.clone(),
                    field_id: "private_key".into(),
                };
                let checked = private(entry.item.value("private_key"));
                let inspected = crate::vault::ssh::inspect(entry.item.value("private_key"));
                let reason = checked.as_ref().err().map(crate::error_code);
                rows.push(json!({"reference":reference,"title":entry.item.title,"public_key":inspected.as_ref().map(|(key,_)|key),"fingerprint":inspected.as_ref().map(|(_,fingerprint)|fingerprint),"supported":checked.is_ok(),"reason":reason}));
            }
            for f in &mut entry.item.fields {
                f.value.zeroize();
            }
            entry.item.notes.zeroize();
        }
        Ok(json!({"keys":rows,"bindings":self.secret_ssh_bindings()?}))
    }

    /// Request activation for explicitly chosen purposes.
    pub fn secret_ssh_binding_request(
        &mut self,
        source: &str,
        reference: SecretRef,
        purposes: Vec<String>,
    ) -> Result<Value> {
        ensure(matches!(source, "gui" | "cli"), Error::Forbidden)?;
        ensure_valid(
            !purposes.is_empty()
                && purposes.len() <= 2
                && purposes
                    .iter()
                    .all(|p| matches!(p.as_str(), "ssh_auth" | "git_sign"))
                && (purposes.len() == 1 || purposes[0] != purposes[1]),
            "SSH purposes",
        )?;
        let resolved = self.secret_resolve(&reference, false)?;
        let key = private(&self.secret_value(&reference)?)?;
        let binding = KeyBinding {
            id: hex::encode(random_id()),
            resolved,
            public_key: key
                .public_key()
                .to_openssh()
                .map_err(|_| Error::Forbidden)?,
            fingerprint: key.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
            purposes,
            enabled: true,
            device_id: self.secret_device(),
        };
        let now = now_ms();
        let r = self.approval_create_at(
            source,
            Action::SecretSshBinding {
                binding: Box::new(binding),
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&r)
    }

    pub(crate) fn secret_ssh_apply(
        &mut self,
        binding: &KeyBinding,
        request_id: &str,
    ) -> Result<()> {
        ensure(
            binding.device_id == self.secret_device()
                && self.secret_resolve(&binding.resolved.reference, false)? == binding.resolved,
            Error::Forbidden,
        )?;
        let mut bindings = self.secret_ssh_bindings()?;
        // A newly approved revision supersedes prior bindings of this field.
        bindings.retain(|b| b.resolved.reference != binding.resolved.reference);
        ensure(bindings.len() < 64, Error::QuotaExceeded)?;
        bindings.push(binding.clone());
        self.workspace.store.atomic(|_| {
            self.workspace
                .put_private(self.keys()?, BINDINGS, &bindings)?;
            let mut request = self.approval_record(request_id)?;
            request.started = true;
            request.state = State::Done;
            self.write_approval(&request)
        })
    }

    /// Stop this binding immediately, including outstanding signature approvals.
    pub fn secret_ssh_disable(&mut self, id: &str) -> Result<Value> {
        let mut bindings = self.secret_ssh_bindings()?;
        let b = bindings
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or(Error::NotFound)?;
        b.enabled = false;
        self.workspace
            .put_private(self.keys()?, BINDINGS, &bindings)?;
        self.expire_approvals(now_ms())?;
        Ok(json!({"disabled":id}))
    }

    /// Register a verified local socket peer; no RPC exposes this method.
    pub fn secret_ssh_open(&mut self, peer: Peer) -> Result<String> {
        self.keys()?;
        ensure(self.ssh_connections.len() < 32, Error::QuotaExceeded)?;
        let id = hex::encode(random_id());
        self.ssh_connections.insert(
            id.clone(),
            Connection {
                peer,
                generation: self.session_generation,
                session: None,
            },
        );
        Ok(id)
    }

    /// Bind once, rejecting forwarded, duplicate and changed host sessions.
    pub fn secret_ssh_bind(&mut self, id: &str, frame: &[u8]) -> Result<()> {
        self.keys()?;
        if !self
            .ssh_connections
            .get(id)
            .is_some_and(|c| c.generation == self.session_generation && c.session.is_none())
        {
            self.ssh_connections.remove(id);
            return Err(Error::Forbidden);
        }
        let verified = match session_bind(frame) {
            Ok(verified) => verified,
            Err(error) => {
                self.ssh_connections.remove(id);
                return Err(error);
            }
        };
        self.ssh_connections
            .get_mut(id)
            .ok_or(Error::Forbidden)?
            .session = Some(verified);
        Ok(())
    }

    /// A disconnected socket cannot be revived by a late decision.
    pub fn secret_ssh_close(&mut self, id: &str) {
        self.ssh_connections.remove(id);
        if self.keys.is_some() {
            let _ = self.expire_approvals(now_ms());
        }
    }

    /// Enumerate explicitly enabled current keys. Locked callers receive an empty list.
    pub fn secret_ssh_identities(&self) -> Result<Vec<(Vec<u8>, String)>> {
        if self.keys.is_none() {
            return Ok(vec![]);
        }
        self.secret_ssh_bindings()?
            .into_iter()
            .filter(|b| {
                b.enabled
                    && self
                        .secret_resolve(&b.resolved.reference, false)
                        .is_ok_and(|r| r == b.resolved)
            })
            .map(|b| {
                let key = ssh_key::PublicKey::from_openssh(&b.public_key)
                    .map_err(|_| Error::IntegrityFailed)?;
                Ok((
                    key.to_bytes().map_err(|_| Error::IntegrityFailed)?,
                    b.resolved.item_title,
                ))
            })
            .collect()
    }

    /// Request one signature; the transport holds bytes until approved or disconnected.
    pub fn secret_ssh_request(
        &mut self,
        connection: &str,
        public: &[u8],
        bytes: &[u8],
    ) -> Result<Value> {
        self.keys()?;
        let c = self
            .ssh_connections
            .get(connection)
            .ok_or(Error::Forbidden)?;
        ensure(c.generation == self.session_generation, Error::Forbidden)?;
        let public_key = ssh_key::PublicKey::from_bytes(public).map_err(|_| Error::Forbidden)?;
        let binding = self
            .secret_ssh_bindings()?
            .into_iter()
            .find(|b| {
                b.enabled
                    && ssh_key::PublicKey::from_openssh(&b.public_key)
                        .is_ok_and(|k| k.key_data() == public_key.key_data())
            })
            .ok_or(Error::Forbidden)?;
        ensure(
            self.secret_resolve(&binding.resolved.reference, false)? == binding.resolved,
            Error::Forbidden,
        )?;
        let purpose = purpose(bytes, public, c.session.as_ref())?;
        ensure(binding.purposes.contains(&purpose.kind), Error::Forbidden)?;
        let plan = SignPlan {
            binding,
            connection: connection.into(),
            peer: c.peer.clone(),
            purpose,
            data_digest: hex::encode(Sha256::digest(bytes)),
            boot_id: self.secret_boot.clone(),
            session_generation: self.session_generation,
        };
        let now = now_ms();
        let r = self.approval_create_at(
            "client:ssh",
            Action::SecretSsh {
                plan: Box::new(plan),
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&r)
    }

    pub(crate) fn secret_ssh_check(&self, p: &SignPlan) -> Result<()> {
        self.keys()?;
        let c = self
            .ssh_connections
            .get(&p.connection)
            .ok_or(Error::Forbidden)?;
        ensure(
            p.boot_id == self.secret_boot
                && p.session_generation == self.session_generation
                && c.generation == self.session_generation
                && c.peer == p.peer
                && self.secret_ssh_bindings()?.contains(&p.binding)
                && p.binding.enabled
                && self.secret_resolve(&p.binding.resolved.reference, false)? == p.binding.resolved,
            Error::Forbidden,
        )
    }

    pub(crate) fn secret_ssh_approve(&mut self, r: &Request) -> Result<Value> {
        let Action::SecretSsh { plan } = &r.action else {
            return Err(Error::Forbidden);
        };
        self.secret_ssh_check(plan)?;
        self.running.insert(r.id.clone(), r.updated_at);
        self.approval_view(r)
    }

    /// Sign once into the original connected transport; never a public owner/MCP RPC.
    pub fn secret_ssh_sign(&mut self, id: &str, connection: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        let now = now_ms();
        let mut r = self.approval_record(id)?;
        let Action::SecretSsh { plan } = &r.action else {
            return Err(Error::Forbidden);
        };
        ensure(
            r.state == State::Executing
                && !r.started
                && now < r.expires_at
                && plan.connection == connection
                && hex::encode(Sha256::digest(bytes)) == plan.data_digest,
            Error::Forbidden,
        )?;
        self.secret_ssh_check(plan)?;
        let key = private(&self.secret_value(&plan.binding.resolved.reference)?)?;
        r.started = true;
        self.save_approval(&r)?;
        let sig: ssh_key::Signature = key.try_sign(bytes).map_err(|_| Error::Forbidden)?;
        r.state = State::Done;
        r.updated_at = now;
        self.save_approval(&r)?;
        self.running.remove(id);
        Ok(sig.to_vec())
    }
}
