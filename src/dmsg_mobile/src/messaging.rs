//! Typed messaging actions; no arbitrary RPC or unprotected draft storage.
use super::*;
use dmsg_client::channel_log;

pub(super) fn can_invite_to_channel(ledger: &channel_log::Ledger, account: &str) -> bool {
    // Invitations can finish a required rotation before sealing the invitee's key.
    ledger.status != channel_log::Status::Archived
        && ledger.members.get(account).is_some_and(|member| {
            matches!(
                member.role,
                channel_log::Role::Owner | channel_log::Role::Admin
            )
        })
}

#[derive(uniffi::Record)]
pub struct LetterDraftInput {
    pub id: String,
    pub recipient: String,
    pub text: String,
    pub invitation: Option<String>,
}

#[uniffi::export]
impl MobileEngine {
    /// Local only; the sends an interruption left open, oldest first.
    pub async fn recoverable_messages(&self) -> Result<Vec<RecoverableMessage>> {
        self.host
            .run(|c| {
                Box::pin(async {
                    Ok(c.recoverable_intents(dmsg_client::now_ms())?
                        .into_iter()
                        .map(|i| RecoverableMessage {
                            channel: i.channel.clone(),
                            message: i.into(),
                        })
                        .collect())
                })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn saved_attachment(&self, channel: String, sequence: u64) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move { c.channel_saved_attachment(&channel, sequence) })
        })
        .await
    }

    pub async fn letter_history(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.letter_history() }))
            .await
    }

    pub async fn letter_drafts(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.letter_drafts() })).await
    }

    pub async fn save_letter_draft(
        &self,
        id: String,
        recipient: String,
        text: String,
        invitation: Option<String>,
    ) -> Result<()> {
        self.host
            .run(move |c| {
                Box::pin(async move { c.save_letter_draft(&id, &recipient, &text, invitation) })
            })
            .await?;
        Ok(())
    }

    pub async fn discard_letter_draft(&self, id: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.discard_letter_draft(&id) }))
            .await?;
        Ok(())
    }

    pub async fn edit_unsent_letter(&self, id: String) -> Result<()> {
        let turn = self.read_turns.lane(&format!("letter:{id}"));
        self.held(move |h| {
            Box::pin(async move {
                let _turn = turn.lock().await;
                h.with(|c| c.edit_unsent_letter(&id)).await
            })
        })
        .await
    }

    pub async fn refresh_sent_letter(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane(&format!("letter:{id}"));
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    dmsg_client::letters::refresh(h, &id).await
                })
            }))
            .await
    }

    pub async fn reopen_letter_contact(
        &self,
        order: String,
        read: bool,
        archived: bool,
    ) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane(&format!("inbox-mark:{order}"));
        self.held_object(move |h| {
            Box::pin(async move {
                let _turn = turn.lock().await;
                dmsg_client::inbox::mark_contact(h, &order, read, archived, true).await
            })
        })
        .await
    }

    pub async fn messaging_accounts(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                let mut accounts = std::collections::BTreeSet::new();
                for channel in c.channel_list()? {
                    if let Some(ledger) = channel.ledger {
                        accounts.extend(ledger.members.into_keys());
                    }
                }
                accounts.extend(c.inbox_cached()?.into_iter().map(|l| l.sender));
                for row in c.letter_history()? {
                    if let Some(id) = row["recipient"].as_str() {
                        accounts.insert(id.to_string());
                    }
                }
                if let Some(policy) = c.inbox_cached_policy()?
                    && let Some(allow) = policy["allow"].as_array()
                {
                    accounts.extend(allow.iter().filter_map(|v| v.as_str().map(str::to_string)));
                }
                if let Some(me) = c.session_state().account {
                    accounts.remove(&me);
                }
                Ok(accounts
                    .into_iter()
                    .filter(|v| v.parse::<dmsg_types::AccountId>().is_ok())
                    .collect::<Vec<_>>())
            })
        })
        .await
    }

    pub async fn search_messages_page(
        &self,
        channel: String,
        query: String,
        before: Option<u64>,
    ) -> Result<MessagePage> {
        let generation = self.host.generation();
        let mut items = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    c.channel_search_before(&channel, &query, before.unwrap_or(u64::MAX), 101)
                })
            })
            .await?;
        let has_more = items.len() > 100;
        items.truncate(100);
        let next_before = has_more.then(|| items.last().map_or(0, |m| m.seq));
        items.reverse();
        Ok(MessagePage {
            generation,
            items: items.into_iter().map(MessageItem::from).collect(),
            has_more,
            next_before,
        })
    }

    pub async fn queue_attachment_reply(
        &self,
        channel: String,
        file: String,
        text: String,
        sequence: Option<u64>,
        digest: Option<String>,
    ) -> Result<String> {
        self.host
            .run(move |c| {
                Box::pin(async move {
                    let reference = match (sequence, digest) {
                        (Some(seq), Some(digest)) => {
                            Some(dmsg_client::channel::MessageRef { seq, digest })
                        }
                        (None, None) => None,
                        _ => return Err(dmsg_types::invalid("reply")),
                    };
                    c.queue_attachment_reply(&channel, &file, &text, reference)
                        .map(|i| i.id)
                })
            })
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use channel_log::{Action, ChannelType, Control, Genesis, Role, Status};

    #[test]
    fn invitation_availability_matches_control_permissions_during_rotation() {
        let account = "040g2081040g2081040g";
        let mut ledger = channel_log::start(
            &Genesis {
                channel_id: "a1".repeat(32),
                nonce: "b1".repeat(32),
                kind: ChannelType::Collaboration,
            },
            account,
            &"c1".repeat(32),
        )
        .unwrap();
        let control = Control {
            expected_head: ledger.head.clone(),
            expected_acl_version: ledger.acl_version,
            action: Action::Invite {
                invitation_id: "d1".repeat(32),
                account: "040g2082040g2082040g".into(),
                role: Role::Member,
                expires_at: 2_000,
                envelope_upload: None,
            },
        };
        for status in [Status::Active, Status::RotationRequired, Status::Archived] {
            ledger.status = status;
            for role in [Role::Owner, Role::Admin, Role::Member, Role::Publisher] {
                ledger.members.get_mut(account).unwrap().role = role;
                let permitted =
                    channel_log::advance(&ledger, &control, account, &"e1".repeat(32), 1_000)
                        .is_ok();
                assert_eq!(can_invite_to_channel(&ledger, account), permitted);
                if status == Status::RotationRequired {
                    assert!(!channel_log::may_publish(&ledger, account));
                }
            }
        }
        assert!(!can_invite_to_channel(&ledger, "040g2082040g2082040g"));
    }
}
