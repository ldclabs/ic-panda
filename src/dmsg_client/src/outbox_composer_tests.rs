use super::*;
use crate::testing::{MemoryStorage, TempDir, joined_channel, unlocked};
use std::sync::Arc;

fn attachments(client: &mut Client) -> Vec<Attachment> {
    ["first.txt", "second.txt"]
        .into_iter()
        .map(|name| {
            let file = client
                .files_write("root", name, b"QA composer original bytes")
                .unwrap();
            Attachment {
                file: file.file.id,
                version: file.version,
                name: name.into(),
            }
        })
        .collect()
}

fn stamp(revision: u64) -> DraftStamp {
    DraftStamp {
        writer: "ab".repeat(16),
        revision,
    }
}

#[tokio::test]
async fn complete_composer_survives_restart_and_is_cleared_only_with_the_whole_batch() {
    let dir = TempDir::new();
    let secure = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(secure.clone()));
    let channel = "a1".repeat(32);
    joined_channel(&mut client, &channel);
    client.enable_fast_unlock().unwrap();
    let draft = ComposerDraft {
        text: "Both attachments reviewed".into(),
        attachments: attachments(&mut client),
        reply_to: None,
    };
    client
        .save_stamped_composer(&channel, &draft, &stamp(1))
        .unwrap();
    drop(client);
    let mut client = Client::open(crate::testing::config(false), &dir.0, Some(secure)).unwrap();
    client.unlock_fast().await.unwrap();
    let restored: ComposerDraft =
        serde_json::from_value(client.message_composers().unwrap()[0]["draft"].clone()).unwrap();
    assert_eq!(restored, draft);
    // Updating the current file must not move a selected historical version.
    client
        .files_write("root", "first.txt", b"Later content, not selected")
        .unwrap();
    let ids = client
        .queue_stamped_composer(&channel, &draft, &stamp(1))
        .unwrap();
    let pending = client.message_intents(&channel).unwrap();
    assert_eq!(
        pending.iter().map(|i| &i.id).collect::<Vec<_>>(),
        ids.iter().collect::<Vec<_>>()
    );
    assert_eq!(pending[0].text, draft.text);
    assert!(pending[1].text.is_empty());
    assert_eq!(pending[0].attachment, Some(draft.attachments[0].clone()));
    assert!(client.message_composers().unwrap().is_empty());
    assert!(client.channel_view(&channel).unwrap().draft.is_empty());
    client.forget_message_intent(&channel, &ids[0]).unwrap();
    assert!(
        !client
            .save_stamped_composer(&channel, &draft, &stamp(1))
            .unwrap()
    );
    assert_eq!(
        client
            .queue_stamped_composer(&channel, &draft, &stamp(1))
            .unwrap(),
        ids
    );
    assert_eq!(client.message_intents(&channel).unwrap().len(), 1);
    let newer = ComposerDraft {
        text: "Next draft, after the uncertain queue".into(),
        ..Default::default()
    };
    client
        .save_stamped_composer(&channel, &newer, &stamp(2))
        .unwrap();
    assert_eq!(
        client
            .queue_stamped_composer(&channel, &draft, &stamp(1))
            .unwrap(),
        ids
    );
    assert_eq!(client.channel_view(&channel).unwrap().draft, newer.text);
    assert!(
        !client
            .save_stamped_composer(&channel, &draft, &stamp(1))
            .unwrap()
    );
}

#[test]
fn missing_second_attachment_never_queues_the_first_or_clears_the_draft() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let channel = "a1".repeat(32);
    joined_channel(&mut client, &channel);
    let mut draft = ComposerDraft {
        text: "Keep everything".into(),
        attachments: attachments(&mut client),
        reply_to: None,
    };
    draft.attachments[1].version = "ff".repeat(32);
    client
        .save_stamped_composer(&channel, &draft, &stamp(1))
        .unwrap();
    assert!(
        client
            .queue_stamped_composer(&channel, &draft, &stamp(1))
            .is_err()
    );
    assert!(client.message_intents(&channel).unwrap().is_empty());
    assert_eq!(client.channel_view(&channel).unwrap().draft, draft.text);
    let restored: ComposerDraft =
        serde_json::from_value(client.message_composers().unwrap()[0]["draft"].clone()).unwrap();
    assert_eq!(restored, draft);
}

#[test]
fn one_revision_cannot_swap_attachments_or_reply_and_a_full_queue_keeps_the_review() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let channel = "a1".repeat(32);
    joined_channel(&mut client, &channel);
    let draft = ComposerDraft {
        text: "Fixed review".into(),
        attachments: attachments(&mut client),
        reply_to: None,
    };
    client
        .save_stamped_composer(&channel, &draft, &stamp(1))
        .unwrap();
    let mut changed = draft.clone();
    changed.attachments.reverse();
    assert_eq!(
        client.queue_stamped_composer(&channel, &changed, &stamp(1)),
        Err(Error::IdempotencyConflict)
    );
    changed = draft.clone();
    changed.reply_to = Some(channel::MessageRef {
        seq: 7,
        digest: "ca".repeat(32),
    });
    assert_eq!(
        client.save_stamped_composer(&channel, &changed, &stamp(1)),
        Err(Error::IdempotencyConflict)
    );
    for _ in 0..LIMIT - 1 {
        client
            .queue_message(&channel, "Earlier queued message")
            .unwrap();
    }
    assert_eq!(
        client.queue_stamped_composer(&channel, &draft, &stamp(1)),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(client.message_intents(&channel).unwrap().len(), LIMIT - 1);
    assert_eq!(client.message_composers().unwrap().len(), 1);
}
