//! Application-action encoding and validation against a registered command
//! schema, without product authority claims.
use crate::{authenticated, canonical, digest, integration, nonzero};
use dmsg_types::{app_action::*, integration::*, *};

/// Experimental COSE profile, distinct from each existing document profile.
pub const APP_ACTION_PROFILE: &str = "application/vnd.dmsg.app-action+cose;v=1";
/// Maximum deterministic CBOR action body, leaving space for protected claims.
pub const MAX_APP_ACTION_BYTES: usize = 48 * 1024;
/// Maximum deterministic CBOR schema carried by an app registration.
pub const MAX_ACTION_SCHEMA_BYTES: usize = 16 * 1024;
/// Maximum value and field-type nesting; a top-level argument is level one.
pub const MAX_ACTION_DEPTH: usize = 3;
const MAX_FIELDS: usize = 32;
const MAX_ITEMS: usize = 64;
const MAX_TEXT: usize = 4096;
const MAX_LABEL: usize = 256;
const MAX_LOCALES: usize = 8;

fn text(s: &str, max: usize, multiline: bool) -> Result<()> {
    ensure_valid(
        !s.trim().is_empty()
            && s.len() <= max
            && !s
                .chars()
                .any(|c| c.is_control() && !(multiline && (c == '\n' || c == '\t'))),
        "action text",
    )
}

fn media(s: &str) -> Result<()> {
    ensure_valid(
        s.len() <= 128 && s.parse::<mime::Mime>().is_ok(),
        "action media type",
    )
}

/// Command, field and choice names: an ASCII letter, then letters, digits or `_`.
fn name(s: &str) -> Result<()> {
    ensure_valid(
        s.len() <= 64
            && s.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
            && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "action name",
    )
}

fn unique_names<'a>(names: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for n in names {
        name(n)?;
        ensure_valid(seen.insert(n), "duplicate action name")?;
    }
    Ok(())
}

fn labels(labels: &[ActionLabel]) -> Result<()> {
    ensure_valid(
        !labels.is_empty() && labels.len() <= MAX_LOCALES,
        "schema labels",
    )?;
    let mut seen = std::collections::BTreeSet::new();
    for label in labels {
        ensure_valid(
            !label.locale.is_empty()
                && label.locale.len() <= 35
                && label
                    .locale
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && seen.insert(label.locale.as_str()),
            "schema locale",
        )?;
        text(&label.text, MAX_LABEL, false)?;
    }
    Ok(())
}

/// Labels that tell commands, fields or values apart share no text in any
/// locale, so the confirmation page never renders two of them alike.
fn distinct<'a>(sets: impl Iterator<Item = &'a [ActionLabel]>) -> Result<()> {
    let mut owner = std::collections::BTreeMap::new();
    for (i, labels) in sets.enumerate() {
        for label in labels {
            ensure_valid(
                *owner.entry(label.text.as_str()).or_insert(i) == i,
                "ambiguous labels",
            )?;
        }
    }
    Ok(())
}

fn schema_fields(fields: &[FieldSchema], depth: usize) -> Result<()> {
    ensure_valid(fields.len() <= MAX_FIELDS, "schema fields")?;
    unique_names(fields.iter().map(|f| f.name.as_str()))?;
    for field in fields {
        labels(&field.label)?;
        field_type(&field.ty, depth)?;
    }
    distinct(fields.iter().map(|f| f.label.as_slice()))
}

fn field_type(ty: &FieldType, depth: usize) -> Result<()> {
    ensure_valid(depth <= MAX_ACTION_DEPTH, "schema depth")?;
    match ty {
        FieldType::Nat { min, max } => ensure_valid(min <= max, "schema range"),
        FieldType::Bool { yes, no } => {
            labels(yes)?;
            labels(no)?;
            distinct([yes.as_slice(), no.as_slice()].into_iter())
        }
        FieldType::Text { max_bytes, .. } => {
            ensure_valid((1..=MAX_TEXT as u64).contains(max_bytes), "schema text")
        }
        FieldType::Hash | FieldType::Principal | FieldType::Artifact => Ok(()),
        FieldType::Choice { options } => {
            ensure_valid(
                !options.is_empty() && options.len() <= MAX_FIELDS,
                "schema choice",
            )?;
            unique_names(options.iter().map(|o| o.value.as_str()))?;
            options.iter().try_for_each(|o| labels(&o.label))?;
            distinct(options.iter().map(|o| o.label.as_slice()))
        }
        FieldType::Optional { item } => {
            ensure_valid(
                !matches!(**item, FieldType::Optional { .. }),
                "schema optional",
            )?;
            field_type(item, depth + 1)
        }
        FieldType::List { item, max_items } => {
            ensure_valid((1..=MAX_ITEMS as u64).contains(max_items), "schema list")?;
            field_type(item, depth + 1)
        }
        FieldType::Record { fields } => {
            ensure_valid(!fields.0.is_empty(), "schema record")?;
            schema_fields(&fields.0, depth + 1)
        }
    }
}

/// Validate a schema before it is registered. Request checks rely on accepted schemas.
pub fn validate_action_schema(schema: &ActionSchema) -> Result<()> {
    ensure(schema.version == 1, Error::UnsupportedProtocol)?;
    ensure_valid(
        !schema.commands.is_empty() && schema.commands.len() <= MAX_FIELDS,
        "schema commands",
    )?;
    unique_names(schema.commands.iter().map(|c| c.name.as_str()))?;
    for command in &schema.commands {
        labels(&command.title)?;
        schema_fields(&command.fields.0, 1)?;
    }
    distinct(schema.commands.iter().map(|c| c.title.as_slice()))?;
    ensure(
        canonical(schema).len() <= MAX_ACTION_SCHEMA_BYTES,
        Error::QuotaExceeded,
    )
}

/// Digest naming one exact schema; every action signs the digest of its schema.
pub fn action_schema_hash(schema: &ActionSchema) -> Hash {
    digest("dmsg/action-schema/v1", schema)
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
            text(name, 512, true)?;
        }
    }
    Ok(())
}

/// Bounds of the closed value model, shared by every schema.
fn args(args: &[ActionArg], depth: usize) -> Result<()> {
    ensure_valid(args.len() <= MAX_FIELDS, "action arguments")?;
    for arg in args {
        name(&arg.name)?;
        value(&arg.value, depth)?;
    }
    Ok(())
}

fn value(v: &ActionValue, depth: usize) -> Result<()> {
    ensure_valid(depth <= MAX_ACTION_DEPTH, "action depth")?;
    match v {
        ActionValue::Text(s) => text(s, MAX_TEXT, true),
        ActionValue::Choice(c) => name(c),
        ActionValue::List(items) => {
            ensure_valid(items.len() <= MAX_ITEMS, "action list")?;
            items.iter().try_for_each(|item| value(item, depth + 1))
        }
        ActionValue::Record(fields) => args(&fields.0, depth + 1),
        _ => Ok(()),
    }
}

fn conform_fields(fields: &[FieldSchema], args: &[ActionArg]) -> Result<()> {
    ensure_valid(fields.len() == args.len(), "action arguments")?;
    for (field, arg) in fields.iter().zip(args) {
        ensure_valid(field.name == arg.name, "action argument name")?;
        conform(&field.ty, &arg.value)?;
    }
    Ok(())
}

fn conform(ty: &FieldType, v: &ActionValue) -> Result<()> {
    use ActionValue as V;
    use FieldType as T;
    match (ty, v) {
        (T::Nat { min, max }, V::Nat(n)) => ensure_valid(min <= n && n <= max, "action number"),
        (T::Bool { .. }, V::Bool(_)) => Ok(()),
        (
            T::Text {
                max_bytes,
                multiline,
            },
            V::Text(s),
        ) => text(s, *max_bytes as usize, *multiline),
        (T::Hash, V::Hash(h)) => nonzero(&h[..]),
        (T::Principal, V::Principal(p)) => authenticated(*p),
        (T::Choice { options }, V::Choice(c)) => {
            ensure_valid(options.iter().any(|o| o.value == *c), "action choice")
        }
        (T::Artifact, V::Artifact(a)) => validate_action_artifact(a),
        (T::Optional { .. }, V::Null) => Ok(()),
        (T::Optional { item }, v) => conform(item, v),
        (T::List { item, max_items }, V::List(items)) => {
            ensure_valid(items.len() as u64 <= *max_items, "action list")?;
            items.iter().try_for_each(|v| conform(item, v))
        }
        (T::Record { fields }, V::Record(args)) => conform_fields(&fields.0, &args.0),
        _ => Err(invalid("action argument type")),
    }
}

/// Check content independent of wall-clock time and of the schema, so historical
/// signatures remain verifiable. Use [`validate_action_command`] for the meaning.
pub fn validate_app_action(action: &AppAction) -> Result<()> {
    ensure(action.version == 1, Error::UnsupportedProtocol)?;
    integration::validate_identifier(&action.app_id)?;
    crate::validate_origin(&action.origin, &action.environment)?;
    authenticated(action.receiver)?;
    ensure_valid((1..=64).contains(&action.actor.len()), "action actor")?;
    nonzero(&action.actor)?;
    nonzero(action.signing_account.as_slice())?;
    ensure_valid(action.app_config_version > 0, "application revision")?;
    for hash in [
        &action.operation_id,
        &action.intent_hash,
        &action.schema_hash,
    ] {
        nonzero(hash.as_slice())?;
    }
    ensure_valid(
        action.issued_at_ms < action.expires_at_ms
            && action.expires_at_ms - action.issued_at_ms <= AUTH_TTL_MS,
        "action window",
    )?;
    name(&action.command.name)?;
    args(&action.command.args.0, 1)?;
    validate_action_files(&action.files)?;
    ensure(
        canonical(action).len() <= MAX_APP_ACTION_BYTES,
        Error::QuotaExceeded,
    )
}

/// Check the command against the exact schema it signs. `schema` must have
/// been accepted by [`validate_action_schema`]. Does not prove that the
/// command was prepared by the named product.
pub fn validate_action_command(action: &AppAction, schema: &ActionSchema) -> Result<()> {
    ensure(
        action.schema_hash == action_schema_hash(schema),
        Error::IntegrityFailed,
    )?;
    let command = schema
        .commands
        .iter()
        .find(|c| c.name == action.command.name)
        .ok_or(Error::UnsupportedProtocol)?;
    conform_fields(&command.fields.0, &action.command.args.0)
}

/// Admission against an app registration already accepted by `validate_app`.
/// Receiver/actor/intent authority is separate.
pub fn validate_action_admission(
    action: &AppAction,
    app: &AppRegistration,
    now_ms: u64,
) -> Result<()> {
    validate_app_action(action)?;
    ensure(!app.paused, Error::Locked)?;
    let schema = app.action_schema.as_ref().ok_or(Error::Forbidden)?;
    ensure(
        action.environment == app.environment
            && action.app_id == app.app_id
            && action.app_config_version == app.config_version
            && app.origins.contains(&action.origin)
            && app.capabilities.contains(&AppCapability::SignAction)
            && app.profiles.contains(&SigningProfile::AppActionV1),
        Error::Forbidden,
    )?;
    validate_action_command(action, schema)?;
    ensure(
        action.issued_at_ms <= now_ms && now_ms < action.expires_at_ms,
        Error::Expired,
    )
}

#[cfg(test)]
#[path = "app_action_tests.rs"]
mod tests;
