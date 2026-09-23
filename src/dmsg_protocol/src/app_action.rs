//! Closed application-action encoding and validation, without product authority claims.
use crate::{authenticated, canonical, digest, ensure_valid, integration, nonzero};
use dmsg_types::{app_action::*, integration::*, *};

/// Experimental COSE profile, distinct from each existing document profile.
pub const APP_ACTION_PROFILE: &str = "application/vnd.dmsg.app-action+cose;v=1";
/// Maximum deterministic CBOR action body, leaving space for protected claims.
pub const MAX_APP_ACTION_BYTES: usize = 48 * 1024;

fn text(s: &str, max: usize, empty: bool) -> Result<()> {
    ensure_valid(
        (empty || !s.trim().is_empty())
            && s.len() <= max
            && !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t'),
        "action text",
    )
}

fn media(s: &str) -> Result<()> {
    ensure_valid(
        s.len() <= 128 && s.parse::<mime::Mime>().is_ok(),
        "action media type",
    )
}

/// Validate a reference without fetching its URI or claiming content verification.
pub fn validate_action_artifact(a: &ActionArtifact) -> Result<()> {
    crate::validate_uri(&a.uri)?;
    ensure_valid(a.uri.len() <= 4096, "artifact URI size")?;
    ensure_valid(
        a.uri.starts_with("https://") || a.uri.starts_with("ipfs://"),
        "artifact scheme",
    )?;
    nonzero(&a.sha256[..])?;
    media(&a.content_type)?;
    ensure_valid(a.size > 0 && a.size <= 256 * 1024 * 1024, "artifact size")
}

/// Validate a canonical file manifest; the containing action provides project and purpose.
pub fn validate_action_files(files: &[ActionFile]) -> Result<()> {
    ensure(files.len() <= 64, Error::QuotaExceeded)?;
    let mut last: Option<&str> = None;
    for file in files {
        text(&file.file_id, 128, false)?;
        ensure_valid(
            file.file_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_:.-".contains(&b)),
            "file locator",
        )?;
        ensure_valid(
            last.is_none_or(|id| id < file.file_id.as_str()),
            "file order/duplicate",
        )?;
        last = Some(&file.file_id);
        ensure_valid(
            file.revision > 0 && file.byte_length <= 256 * 1024 * 1024,
            "file revision/size",
        )?;
        nonzero(&file.sha256[..])?;
        media(&file.media_type)?;
        if let Some(name) = &file.display_name {
            text(name, 512, false)?;
        }
    }
    Ok(())
}

/// Validate all currently understood command semantics and resource bounds.
/// Does not prove that the command was prepared by the named product.
pub fn validate_action_command(command: &AppActionCommand) -> Result<()> {
    use AppActionCommand::*;
    let (project, object) = match command {
        TokenListCertifyDisclosure {
            project_id,
            contract_id,
            revision,
        } => {
            ensure_valid(*revision > 0, "draft revision")?;
            (*project_id, *contract_id)
        }
        TokenListDecideReview {
            project_id,
            case_id,
            round,
            changes,
            rationale,
            ..
        } => {
            ensure_valid(*round > 0 && *round <= u64::from(u32::MAX), "review round")?;
            ensure(changes.len() <= 64, Error::QuotaExceeded)?;
            text(rationale, 4096, false)?;
            for change in changes {
                text(&change.locator, 128, false)?;
                text(&change.detail, 4096, false)?;
            }
            (*project_id, *case_id)
        }
        TokenListCertifyTransition {
            project_id,
            transition_id,
            statement_hash,
            rationale,
            analysis,
        } => {
            nonzero(&statement_hash[..])?;
            text(rationale, 4096, false)?;
            if let Some(artifact) = analysis {
                validate_action_artifact(artifact)?;
            }
            (*project_id, *transition_id)
        }
        TokenListApproveTransition {
            project_id,
            transition_id,
            statement_hash,
            rationale,
            ..
        } => {
            nonzero(&statement_hash[..])?;
            text(rationale, 4096, false)?;
            (*project_id, *transition_id)
        }
    };
    ensure_valid(project > 0 && object > 0, "product object")
}

/// Check content independent of wall-clock time so historical signatures remain verifiable.
pub fn validate_app_action(action: &AppAction) -> Result<()> {
    ensure(action.version == 1, Error::UnsupportedProtocol)?;
    integration::validate_identifier(&action.app_id)?;
    integration::validate_origin(&action.origin, &action.environment)?;
    authenticated(action.receiver)?;
    nonzero(action.actor_id.as_slice())?;
    nonzero(action.signing_account.as_slice())?;
    ensure_valid(action.app_config_version > 0, "application revision")?;
    for hash in [
        &action.operation_id,
        &action.intent_hash,
        &action.input_hash,
        &action.subject_hash,
        &action.precondition_hash,
        &action.role_snapshot_hash,
        &action.signing_policy_hash,
        &action.rule_set_hash,
    ] {
        nonzero(hash.as_slice())?;
    }
    ensure_valid(
        action.issued_at_ms < action.expires_at_ms
            && action.expires_at_ms - action.issued_at_ms <= AUTH_TTL_MS,
        "action window",
    )?;
    validate_action_command(&action.command)?;
    ensure(
        action.input_hash == action_input_hash(&action.command),
        Error::IntegrityFailed,
    )?;
    validate_action_files(&action.files)?;
    ensure(
        canonical(action).len() <= MAX_APP_ACTION_BYTES,
        Error::QuotaExceeded,
    )
}

/// Admission against trusted app registration. Receiver/actor/intent authority is separate.
pub fn validate_action_admission(
    action: &AppAction,
    app: &AppRegistration,
    now_ms: u64,
) -> Result<()> {
    validate_app_action(action)?;
    integration::validate_app(app)?;
    ensure(!app.paused, Error::Locked)?;
    ensure(
        action.environment == app.environment
            && action.app_id == app.app_id
            && action.app_config_version == app.config_version
            && app.origins.contains(&action.origin)
            && app.capabilities.contains(&AppCapability::SignAction)
            && app.profiles.contains(&SigningProfile::AppActionV1),
        Error::Forbidden,
    )?;
    ensure(
        action.issued_at_ms <= now_ms && now_ms < action.expires_at_ms,
        Error::Expired,
    )
}

/// Complete commitment, including every display field, file and receiver.
/// Hashing alone does not authenticate a product preparation.
pub fn app_action_digest(action: &AppAction) -> Hash {
    digest("dmsg/app-action/v1", action)
}

#[cfg(test)]
#[path = "app_action_tests.rs"]
mod tests;

/// Exact TokenList command projection, preserving the original product's field names.
/// This prevents the human-readable decision from contradicting input_hash.
pub fn action_input_hash(command: &AppActionCommand) -> Hash {
    #[derive(serde::Serialize)]
    enum Input<'a> {
        CertifyDisclosure {
            contract: u64,
            expected_revision: u64,
        },
        DecideReviewCase {
            case: u64,
            outcome: &'a ActionReviewOutcome,
            changes: &'a [ActionRequestedChange],
            rationale: &'a str,
        },
        CertifyTransition {
            transition: u64,
            statement_hash: &'a Hash,
            rationale: &'a str,
            analysis: &'a Option<ActionArtifact>,
        },
        ApproveTransition {
            transition: u64,
            approve: bool,
            statement_hash: &'a Hash,
            rationale: &'a str,
        },
    }
    use AppActionCommand::*;
    let input = match command {
        TokenListCertifyDisclosure {
            contract_id,
            revision,
            ..
        } => Input::CertifyDisclosure {
            contract: *contract_id,
            expected_revision: *revision,
        },
        TokenListDecideReview {
            case_id,
            outcome,
            changes,
            rationale,
            ..
        } => Input::DecideReviewCase {
            case: *case_id,
            outcome,
            changes,
            rationale,
        },
        TokenListCertifyTransition {
            transition_id,
            statement_hash,
            rationale,
            analysis,
            ..
        } => Input::CertifyTransition {
            transition: *transition_id,
            statement_hash,
            rationale,
            analysis,
        },
        TokenListApproveTransition {
            transition_id,
            approve,
            statement_hash,
            rationale,
            ..
        } => Input::ApproveTransition {
            transition: *transition_id,
            approve: *approve,
            statement_hash,
            rationale,
        },
    };
    let domain = b"tokenlisting:signable-action:v1";
    let bytes = [&[domain.len() as u8][..], domain, &canonical(&input)].concat();
    crate::sha256(&bytes)
}
