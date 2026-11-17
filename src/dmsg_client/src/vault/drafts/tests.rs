use super::*;
use crate::testing::{self, TempDir};

fn draft() -> Draft {
    Draft {
        id: "a1".repeat(32),
        item_id: None,
        base_revision: None,
        revision: 1,
        form: r#"{"title":"","password":"unsaved-private-value","tags":"unfinished,"}"#.into(),
        active: true,
    }
}

fn changes(title: &str) -> Changes {
    Changes {
        title: Some(title.into()),
        notes: Some("authored note".into()),
        ..Default::default()
    }
}

#[test]
fn incomplete_draft_is_encrypted_and_survives_lock_restart_in_its_account() {
    let dir = TempDir::new();
    let mut c = testing::unlocked(&dir.0, false, None);
    let input = draft();
    assert!(c.checkpoint_vault_draft(&input).unwrap());
    assert!(c.vault_list(&Filter::default()).unwrap().is_empty());
    let path = dir
        .0
        .join("accounts")
        .join(testing::account().id.to_string());
    c.lock();
    assert!(matches!(c.vault_drafts(), Err(Error::Locked)));
    drop(c);
    assert!(path.join("workspace.sqlite").is_file());
    // Inspect every database/WAL byte, not merely the encrypted value's API.
    for name in ["workspace.sqlite", "workspace.sqlite-wal"] {
        if let Ok(bytes) = std::fs::read(path.join(name)) {
            assert!(!bytes.windows(21).any(|w| w == b"unsaved-private-value"));
        }
    }
    let mut c = Client::open(testing::config(false), &dir.0, None).unwrap();
    c.select(crate::client::Slot::Account(testing::account().id))
        .unwrap();
    c.keys = Some(c.workspace.unlock(&testing::SECRET).unwrap());
    assert_eq!(c.vault_drafts().unwrap()[0].form, input.form);
    c.select(crate::client::Slot::Account(
        "080g2081040g2081040g".parse().unwrap(),
    ))
    .unwrap();
    c.keys = Some(c.workspace.unlock_provisional().unwrap());
    assert!(c.vault_drafts().unwrap().is_empty());
}

#[test]
fn failed_checkpoint_consumption_rolls_back_the_item_and_sync_job() {
    let dir = TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    let input = draft();
    c.checkpoint_vault_draft(&input).unwrap();
    let path = dir
        .0
        .join("accounts")
        .join(testing::account().id.to_string())
        .join("workspace.sqlite");
    let database = rusqlite::Connection::open(path).unwrap();
    database.execute_batch("CREATE TRIGGER fail_draft BEFORE INSERT ON sealed WHEN NEW.key LIKE 'vault-draft:%' BEGIN SELECT RAISE(ABORT, 'injected draft write failure'); END").unwrap();
    assert!(
        c.commit_vault_draft(&input, "note", &changes("Must roll back"))
            .is_err()
    );
    assert!(c.vault_list(&Filter::default()).unwrap().is_empty());
    assert_eq!(
        database
            .query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(c.vault_drafts().unwrap()[0].form, input.form);
}

#[test]
fn stale_writes_cannot_overwrite_or_revive_finished_drafts() {
    let dir = TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    let old = draft();
    let mut next = old.clone();
    next.revision = 2;
    next.form = r#"{"title":"latest"}"#.into();
    assert!(c.checkpoint_vault_draft(&next).unwrap());
    assert!(!c.checkpoint_vault_draft(&old).unwrap());
    assert_eq!(c.vault_drafts().unwrap()[0].form, next.form);
    next.form = "{}".into();
    assert!(matches!(
        c.checkpoint_vault_draft(&next),
        Err(Error::IdempotencyConflict)
    ));
    c.discard_vault_draft(&next.id).unwrap();
    next.revision += 1;
    assert!(!c.checkpoint_vault_draft(&next).unwrap());
    assert!(c.vault_drafts().unwrap().is_empty());
}

#[test]
fn failed_save_retains_input_and_success_is_consumed_and_idempotent() {
    let dir = TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    let input = draft();
    assert!(c.commit_vault_draft(&input, "note", &changes("")).is_err());
    assert_eq!(c.vault_drafts().unwrap().len(), 1);
    let first = c
        .commit_vault_draft(&input, "note", &changes("Saved"))
        .unwrap();
    let again = c
        .commit_vault_draft(&input, "note", &changes("Saved"))
        .unwrap();
    assert_eq!(first.revision, again.revision);
    assert!(c.vault_drafts().unwrap().is_empty());
    assert_eq!(c.vault_list(&Filter::default()).unwrap().len(), 1);
    assert!(!c.checkpoint_vault_draft(&input).unwrap());
    assert!(
        c.commit_vault_draft(&input, "note", &changes("Different"))
            .is_err()
    );
}

#[test]
fn resumed_edit_keeps_its_base_and_creates_a_conflict_after_remote_change() {
    let dir = TempDir::new();
    let mut c = testing::unlocked(&dir.0, false, None);
    let base = c.vault_create("note", &changes("Original")).unwrap();
    let mut input = draft();
    input.item_id = Some(base.id.clone());
    input.base_revision = Some(base.revision.clone());
    c.checkpoint_vault_draft(&input).unwrap();
    let remote = c
        .vault_edit(&base.id, Some(&base.revision), &changes("Remote"))
        .unwrap();
    let saved = c
        .commit_vault_draft(&input, "note", &changes("Local draft"))
        .unwrap();
    assert!(saved.conflict);
    assert_eq!(c.vault_show(&base.id).unwrap().revision, remote.revision);
    assert_eq!(saved.item.title, "Local draft");
}

#[test]
fn closing_an_editor_keeps_its_draft_without_reopening_it_or_closing_another() {
    let dir = TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    let mut first = draft();
    c.checkpoint_vault_draft(&first).unwrap();
    let mut second = draft();
    second.id = "b2".repeat(32);
    c.checkpoint_vault_draft(&second).unwrap();
    first.revision += 1;
    first.active = false;
    c.checkpoint_vault_draft(&first).unwrap();
    let drafts = c.vault_drafts().unwrap();
    assert!(!drafts.iter().find(|d| d.id == first.id).unwrap().active);
    assert!(drafts.iter().find(|d| d.id == second.id).unwrap().active);
}

#[test]
fn saving_an_unchanged_edit_consumes_the_draft_without_an_extra_version() {
    let dir = TempDir::new();
    let mut c = testing::unlocked(&dir.0, false, None);
    let original = c.vault_create("note", &changes("Original")).unwrap();
    let mut input = draft();
    input.item_id = Some(original.id.clone());
    input.base_revision = Some(original.revision.clone());
    let saved = c
        .commit_vault_draft(&input, "note", &changes("Original"))
        .unwrap();
    assert_eq!(saved.revision, original.revision);
    assert!(c.vault_drafts().unwrap().is_empty());
}
