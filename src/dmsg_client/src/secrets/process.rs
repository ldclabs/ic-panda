//! Approved delivery to a single CLI claimant. Never exposed through MCP.
use super::*;
use crate::{
    codec::{b64, unb64},
    hold::{Hold, Unlocked},
};
use std::collections::BTreeSet;

/// One environment variable, containing a reference rather than a value.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    /// POSIX environment variable name.
    pub name: String,
    /// Canonical current-field reference.
    pub reference: SecretRef,
}

/// Accepted environment variable grammar.
pub fn variable_name(name: &str) -> bool {
    (1..=128).contains(&name.len())
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_uppercase() || (i > 0 && b.is_ascii_digit()))
}

/// Parse references only: no quoting, substitution, escapes, or shell execution.
pub fn parse_refs(bytes: &[u8]) -> Result<Vec<Variable>> {
    ensure_valid(bytes.len() <= 65536, "refs size")?;
    let text = std::str::from_utf8(bytes).map_err(|_| failure("SECRET_REFERENCE_INVALID"))?;
    let mut variables = Vec::new();
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, uri) = line
            .split_once('=')
            .ok_or_else(|| failure("SECRET_REFERENCE_INVALID"))?;
        ensure_valid(
            variable_name(name) && names.insert(name.to_owned()) && variables.len() < 64,
            "refs variable",
        )?;
        variables.push(Variable {
            name: name.into(),
            reference: SecretRef::parse(uri)?,
        });
    }
    Ok(variables)
}

/// An identity checked by the launching CLI before request and after claim.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileIdentity {
    /// Canonical absolute path.
    pub path: String,
    /// OS device/inode pair (not a security principal).
    pub identity: String,
    /// SHA-256 of the complete file.
    pub sha256: String,
}

/// A sequential file passed through an inherited pipe.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileBinding {
    /// Name of environment variable holding /dev/fd/N.
    pub name: String,
    /// Canonical SecretRef or dmsg-file://file/version.
    pub source: String,
}

/// Request metadata; plaintext values are never accepted here.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunInput {
    /// Exact executable.
    pub executable: FileIdentity,
    /// argv[0] as invoked; multi-call programs choose their behavior from it.
    pub arg0: String,
    /// Explicit argv excluding argv[0].
    pub argv: Vec<String>,
    /// Canonical working directory.
    pub cwd: String,
    /// Optional reference file identity.
    pub refs_file: Option<FileIdentity>,
    /// Complete reference document (no values).
    pub refs: String,
    /// At most eight pipes.
    pub files: Vec<FileBinding>,
}

/// Pinned source metadata shared by process and filesystem delivery.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    /// A current Vault field.
    Field {
        /// Exact field revision.
        resolved: ResolvedSecret,
    },
    /// An exact, locally complete Files version.
    Library {
        /// File ID.
        file_id: String,
        /// Immutable version.
        version_id: String,
        /// Complete plaintext length.
        size: u64,
        /// Complete plaintext digest, sealed in local plans.
        sha256: String,
    },
}

/// Exact reviewed process and sources. Never contains values or claim token.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunPlan {
    /// Request metadata.
    pub input: RunInput,
    /// Account.
    pub account_id: String,
    /// Device.
    pub device_id: String,
    /// Process lifetime.
    pub boot_id: String,
    /// Unlock lifetime.
    pub session_generation: u64,
    /// One-use claim commitment.
    pub claim_digest: String,
    /// Approved environment fields.
    pub env: Vec<(String, ResolvedSecret)>,
    /// Approved sequential files, including their exact sizes.
    pub files: Vec<(String, Source, u64)>,
}

impl RunPlan {
    pub(crate) fn view(&self) -> Value {
        json!({"account_id":self.account_id,"device_id":self.device_id,"executable":self.input.executable,"arg0":self.input.arg0,"argv":self.input.argv,"cwd":self.input.cwd,"refs_file":self.input.refs_file,"env":self.env,"files":self.files,"delivery":"process_and_descendants","boundary":"executable_hash_does_not_verify_dependencies","report":"delivery_only"})
    }
}

impl Client {
    pub(crate) fn secret_source(&mut self, uri: &str, max: u64) -> Result<(Source, u64)> {
        if uri.starts_with("dmsg-secret://") {
            let resolved = self.secret_resolve(&SecretRef::parse(uri)?, false)?;
            let size = self.secret_value(&resolved.reference)?.len() as u64;
            ensure(size <= max, Error::QuotaExceeded)?;
            Ok((Source::Field { resolved }, size))
        } else {
            let pieces: Vec<_> = uri
                .strip_prefix("dmsg-file://")
                .unwrap_or("")
                .split('/')
                .collect();
            ensure_valid(
                pieces.len() == 2 && pieces.iter().all(|v| hex_id(v, 64)),
                "file reference",
            )?;
            let (f, v, mut m) = self.library_manifest(pieces[0], Some(pieces[1]))?;
            ensure(
                v.local && !self.catalog()?.file_trashed(&f),
                Error::NotFound,
            )?;
            m.key.zeroize();
            ensure(m.size <= max, Error::QuotaExceeded)?;
            Ok((
                Source::Library {
                    file_id: m.id,
                    version_id: m.version,
                    size: m.size,
                    sha256: m.sha256,
                },
                m.size,
            ))
        }
    }

    pub(crate) fn secret_source_check(&self, source: &Source) -> Result<()> {
        match source {
            Source::Field { resolved } => ensure(
                self.secret_resolve(&resolved.reference, false)? == *resolved,
                failure("SECRET_VERSION_STALE"),
            ),
            Source::Library {
                file_id,
                version_id,
                size,
                sha256,
            } => {
                use crate::library::catalog::{FILE, FOLDER, FileRecord, Folder, ROOT};
                let keys = self.keys()?;
                let file: FileRecord = self
                    .workspace
                    .private(keys, &format!("{FILE}{file_id}"))?
                    .ok_or(Error::NotFound)?;
                ensure(
                    file.trashed_at.is_none() && file.versions.iter().any(|v| v.id == *version_id),
                    Error::NotFound,
                )?;
                let mut parent = file.folder;
                let mut visited = BTreeSet::new();
                while parent != ROOT {
                    ensure(
                        visited.len() < 64 && visited.insert(parent.clone()),
                        Error::IntegrityFailed,
                    )?;
                    let folder: Folder = self
                        .workspace
                        .private(keys, &format!("{FOLDER}{parent}"))?
                        .ok_or(Error::NotFound)?;
                    ensure(folder.trashed_at.is_none(), Error::NotFound)?;
                    parent = folder.parent;
                }
                let mut version = self.version(file_id, version_id)?;
                version.key.zeroize();
                ensure(
                    version.local && version.size == *size && version.sha256 == *sha256,
                    failure("SECRET_VERSION_STALE"),
                )
            }
        }
    }

    /// Create a high-risk request; return the claim secret only to its CLI.
    pub fn secret_process_request(&mut self, input: RunInput) -> Result<Value> {
        self.keys()?;
        let valid_identity = |v: &FileIdentity| {
            std::path::Path::new(&v.path).is_absolute()
                && v.path.len() <= 4096
                && !v.path.contains('\0')
                && hex_id(&v.sha256, 64)
                && !v.identity.is_empty()
                && v.identity.len() <= 128
        };
        ensure_valid(
            valid_identity(&input.executable)
                && input.refs_file.as_ref().is_none_or(valid_identity)
                && (1..=4096).contains(&input.arg0.len())
                && !input.arg0.contains('\0')
                && std::path::Path::new(&input.cwd).is_absolute()
                && input.cwd.len() <= 4096
                && !input.cwd.contains('\0')
                && input.argv.len() <= 256
                && input.argv.iter().map(String::len).sum::<usize>() <= 65536
                && input.argv.iter().all(|s| !s.contains('\0'))
                && input.files.len() <= 8,
            "run plan",
        )?;
        let refs = parse_refs(input.refs.as_bytes())?;
        ensure_valid(
            input
                .refs_file
                .as_ref()
                .is_none_or(|f| f.sha256 == hex::encode(Sha256::digest(input.refs.as_bytes())))
                && (input.refs.is_empty() || input.refs_file.is_some()),
            "refs digest",
        )?;
        let env = refs
            .into_iter()
            .map(|v| Ok((v.name, self.secret_resolve(&v.reference, false)?)))
            .collect::<Result<Vec<_>>>()?;
        let mut env_bytes = 0usize;
        for (name, resolved) in &env {
            let value = self.secret_value(&resolved.reference)?;
            ensure_valid(!value.contains('\0'), "environment value contains NUL")?;
            env_bytes += name.len() + value.len() + 2;
        }
        ensure(env_bytes <= 65536, Error::QuotaExceeded)?;
        let mut names: BTreeSet<_> = env.iter().map(|(n, _)| n.clone()).collect();
        let files = input
            .files
            .iter()
            .map(|f| {
                ensure_valid(
                    variable_name(&f.name) && names.insert(f.name.clone()),
                    "file variable",
                )?;
                let (source, size) = self.secret_source(&f.source, 8 * 1024 * 1024)?;
                Ok((f.name.clone(), source, size))
            })
            .collect::<Result<Vec<_>>>()?;
        ensure_valid(!env.is_empty() || !files.is_empty(), "empty run")?;
        let claim = b64(&crate::crypto::random_key());
        let plan = RunPlan {
            input,
            account_id: self.account()?.id.to_string(),
            device_id: self.secret_device(),
            boot_id: self.secret_boot.clone(),
            session_generation: self.session_generation,
            claim_digest: hex::encode(Sha256::digest(claim.as_bytes())),
            env,
            files,
        };
        let now = now_ms();
        let r = self.approval_create_at(
            "cli",
            Action::SecretProcess {
                plan: Box::new(plan),
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        Ok(json!({"request_id":r.id,"claim":claim,"approval":self.approval_view(&r)?}))
    }

    pub(crate) fn secret_process_check(&mut self, p: &RunPlan) -> Result<()> {
        self.keys()?;
        ensure(
            p.account_id == self.account()?.id.to_string()
                && p.device_id == self.secret_device()
                && p.boot_id == self.secret_boot
                && p.session_generation == self.session_generation,
            failure("SECRET_SESSION_ENDED"),
        )?;
        for (_, r) in &p.env {
            ensure(
                self.secret_resolve(&r.reference, false)? == *r,
                failure("SECRET_VERSION_STALE"),
            )?;
        }
        for (_, s, _) in &p.files {
            self.secret_source_check(s)?;
        }
        Ok(())
    }

    pub(crate) fn secret_process_approve(&mut self, r: &Request) -> Result<Value> {
        let Action::SecretProcess { plan } = &r.action else {
            return Err(Error::Forbidden);
        };
        self.secret_process_check(plan)?;
        self.running.insert(r.id.clone(), r.updated_at);
        self.approval_view(r)
    }

    /// A best-effort observation; cannot undo delivery or authorize a retry.
    pub fn secret_process_report(
        &mut self,
        id: &str,
        claim: &str,
        started: bool,
        exit_code: Option<i32>,
    ) -> Result<Value> {
        let mut r = self.approval_record(id)?;
        let Action::SecretProcess { plan } = &r.action else {
            return Err(Error::NotFound);
        };
        ensure(
            hex::encode(Sha256::digest(claim.as_bytes())) == plan.claim_digest
                && r.started
                && r.state == State::Done,
            Error::Forbidden,
        )?;
        ensure(r.delivery_report.is_none(), Error::Expired)?;
        r.delivery_report =
            Some(json!({"started":started,"exit_code":exit_code,"evidence":"cli_report"}));
        r.updated_at = now_ms();
        self.save_approval(&r)?;
        self.approval_view(&r)
    }
}

/// Read only an approved source in bounded synchronous segments. No plaintext file.
pub(crate) async fn source_bytes<H: Hold>(
    h: &H,
    source: &Source,
    max: u64,
) -> Result<Zeroizing<Vec<u8>>> {
    let bound = Unlocked::new(h).await?;
    match source {
        Source::Field { resolved } => {
            bound
                .with(|c| {
                    c.secret_source_check(source)?;
                    let v = c.secret_value(&resolved.reference)?;
                    ensure(v.len() as u64 <= max, Error::QuotaExceeded)?;
                    Ok(Zeroizing::new(v.as_bytes().to_vec()))
                })
                .await
        }
        Source::Library {
            file_id,
            version_id,
            size,
            sha256,
        } => {
            ensure(*size <= max, Error::QuotaExceeded)?;
            let mut bytes = Zeroizing::new(Vec::with_capacity(*size as usize));
            for i in 0..crate::files::chunk_count(*size) {
                let block = bound
                    .with(|c| {
                        c.secret_source_check(source)?;
                        let (_, _, mut m) = c.library_manifest(file_id, Some(version_id))?;
                        let b = crate::files::read_chunk(c.blobs()?, &m, i);
                        m.key.zeroize();
                        b
                    })
                    .await?;
                bytes.extend_from_slice(&block);
            }
            bound.with(|c| c.secret_source_check(source)).await?;
            ensure(
                bytes.len() as u64 == *size && hex::encode(Sha256::digest(&bytes)) == *sha256,
                Error::IntegrityFailed,
            )?;
            Ok(bytes)
        }
    }
}

/// Claim is committed before obtaining values. Errors or lost responses never replay it.
pub async fn claim<H: Hold>(h: &H, id: &str, claim: &str) -> Result<Value> {
    ensure_valid(claim.len() <= 128 && unb64(claim)?.len() == 32, "claim")?;
    let bound = Unlocked::new(h).await?;
    let plan = bound
        .with(|c| {
            let mut r = c.approval_record(id)?;
            let Action::SecretProcess { plan } = &r.action else {
                return Err(Error::NotFound);
            };
            ensure(
                r.state == State::Executing
                    && !r.started
                    && now_ms() < r.expires_at
                    && hex::encode(Sha256::digest(claim.as_bytes())) == plan.claim_digest,
                Error::Forbidden,
            )?;
            c.secret_process_check(plan)?;
            let plan = plan.clone();
            r.started = true;
            c.save_approval(&r)?;
            Ok(plan)
        })
        .await?;
    let mut values = Vec::new();
    for (name, resolved) in &plan.env {
        let value = bound
            .with(|c| {
                c.secret_process_check(&plan)?;
                c.secret_value(&resolved.reference)
            })
            .await?;
        ensure_valid(!value.contains('\0'), "environment value")?;
        values.push((name.clone(), value));
    }
    let mut files = Vec::new();
    for (name, source, _) in &plan.files {
        files.push((
            name.clone(),
            source_bytes(&bound, source, 8 * 1024 * 1024).await?,
        ));
    }
    bound
        .with(|c| {
            c.secret_process_check(&plan)?;
            let now = now_ms();
            let mut r = c.approval_record(id)?;
            ensure(
                r.state == State::Executing && r.started && now < r.expires_at,
                Error::Expired,
            )?;
            r.state = State::Done;
            r.updated_at = now;
            c.save_approval(&r)?;
            c.running.remove(id);
            Ok(())
        })
        .await?;
    // This response is permitted only on the owner CLI route, never renderer/events.
    Ok(
        json!({"env":values.iter().map(|(n,v)|json!({"name":n,"value":&**v})).collect::<Vec<_>>(),"files":files.iter().map(|(n,b)|json!({"name":n,"base64url":b64(b)})).collect::<Vec<_>>() }),
    )
}
