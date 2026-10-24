//! Value-free discovery reports and an explicitly approved, atomic import.
use super::*;
use crate::vault::{Field, Item};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const PREFIX: &str = "secret-scan:";

/// One bounded candidate. The file digest binds consent, never a credential-value hash.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    /// Opaque device-local identifier.
    pub id: String,
    /// Display path relative to the selected root.
    pub relative_path: String,
    /// OS file identity observed while reading.
    pub file_identity: String,
    /// SHA-256 of the complete source file, binding the selection.
    pub file_digest: String,
    /// Exact byte length.
    pub size: u64,
    /// Recognized source category.
    pub kind: String,
    /// Variable names only; no values.
    pub variables: Vec<String>,
    /// Public SSH algorithm when recognized.
    pub algorithm: Option<String>,
    /// Stable reasons requiring attention or preventing import.
    pub warnings: Vec<String>,
}
/// Completed traversal metadata. False `complete` is never reported as a clean directory.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Report {
    /// Bounded metadata-only candidates.
    pub candidates: Vec<Candidate>,
    /// Number of filesystem entries visited.
    pub files: u64,
    /// Bytes inspected, including unsuccessful reads.
    pub bytes: u64,
    /// Entries excluded or not inspected.
    pub skipped: u64,
    /// False when limits or access errors left scope uninspected.
    pub complete: bool,
    /// Stable reasons requiring attention or preventing import.
    pub warnings: Vec<String>,
}
/// Sealed source selection, bound to this daemon/unlock lifetime.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scan {
    /// Opaque device-local identifier.
    pub id: String,
    /// Explicitly selected canonical directory.
    pub root: String,
    /// OS identity of that directory.
    pub root_identity: String,
    /// Bounded traversal observations.
    pub report: Report,
    /// Current local workflow state.
    pub state: String,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
    /// Daemon lifetime that owns this work.
    pub boot_id: String,
    /// Unlocked session that owns this work.
    pub session_generation: u64,
    /// Selection digest to original approval, retained for duplicate requests.
    pub imports: std::collections::BTreeMap<String, String>,
}
/// The precise selection shown in the approval. It contains no file values.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportPlan {
    /// Exact source scan reviewed by the owner.
    pub scan: Scan,
    /// Explicit selected candidate identities in import order.
    pub candidate_ids: Vec<String>,
}
/// Data for the daemon's blocking filesystem worker, returned only after approval.
pub struct ImportWork {
    /// Original approval request identifier.
    pub request: String,
    /// Immutable metadata approved for this operation.
    pub plan: ImportPlan,
}

/// Parse literal dotenv values without interpolation or executing any expression.
/// Unsupported quoting/expressions fail as a whole; no silently partial import.
pub fn dotenv(bytes: &[u8]) -> Result<Vec<(String, Zeroizing<String>)>> {
    ensure(bytes.len() <= 1024 * 1024, Error::QuotaExceeded)?;
    let text =
        std::str::from_utf8(bytes).map_err(|_| Error::InvalidInput("dotenv UTF-8".into()))?;
    let mut result: Vec<(String, Zeroizing<String>)> = Vec::new();
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (name, value) = line
            .split_once('=')
            .ok_or(Error::InvalidInput("dotenv syntax".into()))?;
        let name = name.trim();
        ensure_valid(
            process::variable_name(name) && names.insert(name.to_owned()) && result.len() < 64,
            "dotenv variable",
        )?;
        let value = value.trim();
        let value = if value.starts_with(['\'', '"']) {
            let quote = value.as_bytes()[0] as char;
            let inner = value
                .strip_prefix(quote)
                .and_then(|v| v.strip_suffix(quote))
                .ok_or(Error::InvalidInput("dotenv quoting".into()))?;
            ensure_valid(!inner.contains(quote), "dotenv quoting")?;
            inner
        } else {
            ensure_valid(
                !value.contains('#') && !value.contains(char::is_whitespace),
                "dotenv literal",
            )?;
            value
        };
        ensure_valid(
            !value.chars().any(|c| matches!(c, '$' | '`' | '\\' | '\0'))
                && value.chars().count() <= 10000,
            "dotenv expressions are not imported",
        )?;
        result.push((name.into(), Zeroizing::new(value.into())));
    }
    ensure_valid(!result.is_empty(), "empty dotenv")?;
    ensure(
        result
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum::<usize>()
            <= 65536,
        Error::QuotaExceeded,
    )?;
    Ok(result)
}

/// Inspect a candidate in a worker; only names and public algorithm information leave it.
pub fn inspect(path: &str, identity: String, bytes: &[u8]) -> Option<Candidate> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (kind, variables, algorithm, warnings) = if name == ".env.refs" {
        match process::parse_refs(bytes) {
            Ok(refs) => (
                "env_refs",
                refs.into_iter().map(|r| r.name).collect(),
                None,
                vec!["reference_file_not_imported".into()],
            ),
            Err(_) => (
                "env_refs",
                vec![],
                None,
                vec!["invalid_reference_file".into()],
            ),
        }
    } else if name == ".env" || (name.starts_with(".env.") && !name.ends_with(".refs")) {
        match dotenv(bytes) {
            Ok(values) => (
                "dotenv",
                values.into_iter().map(|(n, _)| n).collect(),
                None,
                vec![],
            ),
            Err(_) => (
                "dotenv",
                vec![],
                None,
                vec!["unsupported_dotenv_syntax".into()],
            ),
        }
    } else if bytes.starts_with(b"-----BEGIN OPENSSH PRIVATE KEY-----")
        || bytes.starts_with(b"-----BEGIN RSA PRIVATE KEY-----")
        || bytes.starts_with(b"-----BEGIN EC PRIVATE KEY-----")
    {
        let value = std::str::from_utf8(bytes).ok()?;
        let algorithm = crate::vault::ssh::inspect(value)
            .and_then(|(public, _)| public.split_whitespace().next().map(str::to_owned))
            .or_else(|| Some("unsupported_private_key_format".into()));
        ("ssh_private_key", vec![], algorithm, vec![])
    } else {
        return None;
    };
    Some(Candidate {
        id: hex::encode(random_id()),
        relative_path: path.into(),
        file_identity: identity,
        file_digest: hex::encode(Sha256::digest(bytes)),
        size: bytes.len() as u64,
        kind: kind.into(),
        variables,
        algorithm,
        warnings,
    })
}

/// Produce a new item from exactly the approved bytes; never deletes or rewrites the source.
pub fn import_candidate(
    candidate: &Candidate,
    bytes: &[u8],
) -> Result<(Item, Vec<(String, String)>)> {
    ensure(
        bytes.len() as u64 == candidate.size
            && hex::encode(Sha256::digest(bytes)) == candidate.file_digest,
        Error::VersionConflict,
    )?;
    let title = candidate
        .relative_path
        .chars()
        .take(200)
        .collect::<String>();
    let mut item = Item::new(
        if candidate.kind == "dotenv" {
            "api"
        } else {
            "ssh"
        },
        &title,
        0,
    );
    let mut refs = Vec::new();
    if candidate.kind == "dotenv" {
        item.fields = dotenv(bytes)?
            .into_iter()
            .enumerate()
            .map(|(i, (name, value))| {
                let id = format!("f{i}");
                refs.push((name.clone(), id.clone()));
                Field {
                    id,
                    kind: "concealed".into(),
                    label: name.chars().take(100).collect(),
                    section: String::new(),
                    value: (*value).clone(),
                }
            })
            .collect();
    } else {
        ensure_valid(candidate.kind == "ssh_private_key", "candidate kind")?;
        let value = std::str::from_utf8(bytes)
            .map_err(|_| Error::InvalidInput("private key UTF-8".into()))?;
        item.fields = vec![
            Field::new("private_key", "concealed", value),
            Field::new("public_key", "text", ""),
            Field::new("fingerprint", "text", ""),
        ];
    }
    item.validate()?;
    Ok((item, refs))
}

impl Client {
    /// Register one explicit selected root before starting its worker.
    pub fn secret_discovery_begin(
        &mut self,
        root: String,
        identity: String,
    ) -> Result<(String, Arc<AtomicBool>)> {
        self.keys()?;
        ensure(
            self.secret_scans.len() < 2 && self.workspace.store.sealed_keys(PREFIX)?.len() < 128,
            Error::QuotaExceeded,
        )?;
        let id = hex::encode(random_id());
        let cancel = Arc::new(AtomicBool::new(false));
        let scan = Scan {
            id: id.clone(),
            root,
            root_identity: identity,
            report: Report::default(),
            state: "running".into(),
            created_at: now_ms(),
            boot_id: self.secret_boot.clone(),
            session_generation: self.session_generation,
            imports: Default::default(),
        };
        self.workspace
            .put_private(self.keys()?, &format!("{PREFIX}{id}"), &scan)?;
        self.secret_scans.insert(id.clone(), cancel.clone());
        Ok((id, cancel))
    }

    /// A worker may publish metadata only while its original unlocked session is active.
    pub fn secret_discovery_finish(&mut self, id: &str, report: Report) -> Result<()> {
        let mut scan = self.secret_discovery_scan(id)?;
        ensure(
            scan.boot_id == self.secret_boot
                && scan.session_generation == self.session_generation
                && self
                    .secret_scans
                    .get(id)
                    .is_some_and(|c| !c.load(Ordering::Acquire)),
            Error::Locked,
        )?;
        scan.report = report;
        scan.state = "done".into();
        self.workspace
            .put_private(self.keys()?, &format!("{PREFIX}{id}"), &scan)?;
        self.secret_scans.remove(id);
        Ok(())
    }

    pub(crate) fn secret_discovery_scan(&self, id: &str) -> Result<Scan> {
        ensure_valid(hex_id(id, 64), "scan id")?;
        self.workspace
            .private(self.keys()?, &format!("{PREFIX}{id}"))?
            .ok_or(Error::NotFound)
    }

    /// Cancel the selected worker and leave its original request identity visible.
    pub fn secret_discovery_cancel(&mut self, id: &str) -> Result<Value> {
        let mut scan = self.secret_discovery_scan(id)?;
        if let Some(cancel) = self.secret_scans.remove(id) {
            cancel.store(true, Ordering::Release);
        }
        scan.state = "cancelled".into();
        self.workspace
            .put_private(self.keys()?, &format!("{PREFIX}{id}"), &scan)?;
        self.secret_discovery_status(id)
    }

    /// Value-free, device-local progress and candidates.
    pub fn secret_discovery_status(&self, id: &str) -> Result<Value> {
        let mut scan = self.secret_discovery_scan(id)?;
        if scan.boot_id != self.secret_boot || scan.session_generation != self.session_generation {
            scan.state = "cancelled".into();
        }
        Ok(json!(scan))
    }

    /// Prepare a high-risk import; plain filesystem reads happen only in its approved runner.
    pub fn secret_discovery_request(
        &mut self,
        source: &str,
        id: &str,
        candidates: Vec<String>,
    ) -> Result<Value> {
        ensure(matches!(source, "cli" | "gui"), Error::Forbidden)?;
        let mut scan = self.secret_discovery_scan(id)?;
        ensure(
            scan.state == "done"
                && scan.boot_id == self.secret_boot
                && scan.session_generation == self.session_generation,
            Error::Expired,
        )?;
        ensure_valid(
            !candidates.is_empty()
                && candidates.len() <= 64
                && candidates.iter().collect::<BTreeSet<_>>().len() == candidates.len()
                && candidates.iter().all(|id| {
                    scan.report
                        .candidates
                        .iter()
                        .any(|c| &c.id == id && c.warnings.is_empty())
                }),
            "scan selection",
        )?;
        let selection = hex::encode(Sha256::digest(canonical(&candidates)));
        if let Some(id) = scan.imports.get(&selection) {
            let r = self.approval_record(id)?;
            if r.state == State::Done || !r.state.terminal() {
                return self.approval_view(&r);
            }
        }
        ensure(scan.imports.len() < 64, Error::QuotaExceeded)?;
        let now = now_ms();
        let r = self.approval_create_at(
            source,
            Action::SecretDiscoveryImport {
                plan: Box::new(ImportPlan {
                    scan: scan.clone(),
                    candidate_ids: candidates,
                }),
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        scan.imports.insert(selection, r.id.clone());
        self.workspace
            .put_private(self.keys()?, &format!("{PREFIX}{id}"), &scan)?;
        self.approval_view(&r)
    }

    pub(crate) fn secret_discovery_work(&mut self, r: &Request) -> Result<ImportWork> {
        let Action::SecretDiscoveryImport { plan } = &r.action else {
            return Err(Error::Forbidden);
        };
        ensure(
            plan.scan.boot_id == self.secret_boot
                && plan.scan.session_generation == self.session_generation,
            Error::Expired,
        )?;
        let current = self.secret_discovery_scan(&plan.scan.id)?;
        let selection = hex::encode(Sha256::digest(canonical(&plan.candidate_ids)));
        ensure(
            current.imports.get(&selection) == Some(&r.id),
            Error::Forbidden,
        )?;
        self.running.insert(r.id.clone(), r.updated_at);
        Ok(ImportWork {
            request: r.id.clone(),
            plan: (**plan).clone(),
        })
    }

    /// Validate the original approval lifetime before a host worker reads plaintext.
    pub fn secret_discovery_check_work(&self, work: &ImportWork) -> Result<()> {
        self.keys()?;
        ensure(
            work.plan.scan.boot_id == self.secret_boot
                && work.plan.scan.session_generation == self.session_generation,
            failure("SECRET_SESSION_ENDED"),
        )?;
        let r = self.approval_record(&work.request)?;
        ensure(
            r.state == State::Executing && !r.started && now_ms() < r.expires_at,
            Error::Expired,
        )
    }

    /// Settle a host-side read failure without importing anything.
    pub fn secret_discovery_fail(&mut self, id: &str, error: Error) -> Result<Value> {
        self.secret_fail_before_send(id, error)
    }

    /// Commit all items, sync jobs and the approval receipt together. Generated refs contain no values.
    pub fn secret_discovery_commit(
        &mut self,
        work: ImportWork,
        items: Vec<Item>,
        references: Vec<Vec<(String, String)>>,
    ) -> Result<Value> {
        let now = now_ms();
        ensure(
            work.plan.scan.boot_id == self.secret_boot
                && work.plan.scan.session_generation == self.session_generation,
            Error::Expired,
        )?;
        let mut r = self.approval_record(&work.request)?;
        ensure(
            r.state == State::Executing && !r.started && now < r.expires_at,
            Error::Expired,
        )?;
        ensure(
            items.len() == work.plan.candidate_ids.len() && references.len() == items.len(),
            Error::IntegrityFailed,
        )?;
        self.secret_import_commit_with(items, vec![], "root", |c, ids| {
            let files = ids
                .iter()
                .zip(&references)
                .map(|(id, variables)| {
                    variables
                        .iter()
                        .map(|(name, field)| {
                            format!(
                                "{name}={}\n",
                                SecretRef {
                                    schema_version: 1,
                                    account_id: c
                                        .account()
                                        .expect("account checked")
                                        .id
                                        .to_string(),
                                    item_id: id.clone(),
                                    field_id: field.clone()
                                }
                                .uri()
                            )
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>();
            r.state = State::Done;
            r.started = true;
            r.updated_at = now;
            r.delivery_report =
                Some(json!({"ids":ids,"env_refs":files,"source_files_unchanged":true}));
            c.write_approval(&r)
        })?;
        self.running.remove(&r.id);
        self.approval_view(&r)
    }
}
