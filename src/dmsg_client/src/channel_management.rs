//! Recoverable channel setup and controls. Network waits never own the client.
use super::*;

const CREATION: &str = "channel-creation:";

/// A locally sealed creation intent. Its channel and genesis nonce never change.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Creation {
    /// Stable original attempt ID.
    pub request: String,
    /// Fixed future channel ID.
    pub channel: String,
    /// Reviewed local display name.
    pub name: String,
    /// Reviewed publishing policy.
    pub kind: ChannelType,
    nonce: String,
    /// Unix milliseconds when prepared.
    pub created_at: u64,
    /// First epoch verified and active, in Unix milliseconds.
    pub completed_at: Option<u64>,
}

impl Client {
    /// Persist the reviewed name/type before any online request.
    pub fn channel_prepare_creation(
        &self,
        name: &str,
        kind: ChannelType,
        request: &str,
    ) -> Result<Creation> {
        ensure_valid(is_hash(request), "request ID")?;
        ensure_valid(
            !name.is_empty()
                && name.chars().count() <= MAX_NAME
                && !name.chars().any(char::is_control),
            "name",
        )?;
        let key = format!("{CREATION}{request}");
        if let Some(existing) = self.workspace.private::<Creation>(self.keys()?, &key)? {
            ensure(
                existing.name == name && existing.kind == kind,
                Error::IdempotencyConflict,
            )?;
            return Ok(existing);
        }
        let now = now_ms();
        let mut pending = 0;
        for old in self.workspace.store.sealed_keys(CREATION)? {
            let existing: Creation = self
                .workspace
                .private(self.keys()?, &old)?
                .ok_or(Error::IntegrityFailed)?;
            if existing
                .completed_at
                .is_some_and(|at| now.saturating_sub(at) > 30 * dmsg_types::DAY)
            {
                self.workspace.store.delete_sealed(&old)?;
            } else if existing.completed_at.is_none() {
                pending += 1;
            }
        }
        ensure_valid(pending < 32, "pending creations")?;
        let intent = Creation {
            request: request.into(),
            channel: random_hex(),
            name: name.into(),
            kind,
            nonce: random_hex(),
            created_at: now,
            completed_at: None,
        };
        self.workspace.put_private(self.keys()?, &key, &intent)?;
        Ok(intent)
    }

    /// Original attempts still waiting for verified activation.
    pub fn channel_creations(&self) -> Result<Vec<Creation>> {
        let mut result = Vec::new();
        for key in self.workspace.store.sealed_keys(CREATION)? {
            let intent: Creation = self
                .workspace
                .private(self.keys()?, &key)?
                .ok_or(Error::IntegrityFailed)?;
            if intent.completed_at.is_none() {
                result.push(intent);
            }
        }
        result.sort_by_key(|i| i.created_at);
        Ok(result)
    }

    /// A signed creation may already exist remotely and must be reconciled.
    pub fn channel_discard_creation(&self, request: &str) -> Result<()> {
        let intent = self.channel_creation(request)?;
        ensure(
            self.job::<Operation>(&intent.channel, "genesis")?.is_none(),
            Error::ExecutionUnknown,
        )?;
        self.workspace
            .store
            .delete_sealed(&format!("{CREATION}{request}"))
    }

    fn channel_creation(&self, request: &str) -> Result<Creation> {
        ensure_valid(is_hash(request), "request ID")?;
        self.workspace
            .private(self.keys()?, &format!("{CREATION}{request}"))?
            .ok_or(Error::NotFound)
    }
}

/// Continue the original creation, including its first epoch activation.
pub async fn create<H: Hold>(h: &H, request: &str) -> Result<String> {
    let intent = h.with(|c| c.channel_creation(request)).await?;
    if intent.completed_at.is_some() {
        return Ok(intent.channel);
    }
    let channel = &intent.channel;
    let payload = h
        .signed(|c| {
            c.root_now()?;
            let current = c.channel_creation(request)?;
            ensure(current == intent, Error::VersionConflict)?;
            if let Some(job) = c.job::<Operation>(channel, "genesis")? {
                return Ok(job.payload);
            }
            let genesis = Genesis {
                channel_id: channel.clone(),
                nonce: intent.nonce.clone(),
                kind: intent.kind,
            };
            let job = c.sign_now(GENESIS, base(channel)?, json(&genesis), request.into())?;
            c.save_job(channel, "genesis", &job)?;
            // A crash before this content write is repaired from the original signed job below.
            Ok(job.payload)
        })
        .await?
        .1;
    h.with(|c| {
        if matches!(c.keyring(channel), Err(Error::NotFound)) {
            let job: Operation = c.job(channel, "genesis")?.ok_or(Error::IntegrityFailed)?;
            c.save_keyring(
                None,
                &Keyring {
                    format: FORMAT.into(),
                    id: channel.clone(),
                    name: intent.name.clone(),
                    genesis: hex_sha256(&unb64(&job.cose)?),
                    keys: BTreeMap::new(),
                    muted: false,
                    hidden: false,
                },
            )?;
        }
        c.keyring(channel).map(|_| ())
    })
    .await?;
    post(h, channel, "genesis", GENESIS, payload, "").await?;
    pull(h, channel, None, None).await?;
    rotate(h, channel).await?;
    h.with(|c| {
        let mut current = c.channel_creation(request)?;
        current.completed_at = Some(now_ms());
        c.workspace
            .put_private(c.keys()?, &format!("{CREATION}{request}"), &current)
    })
    .await?;
    Ok(channel.clone())
}

/// Join only the invited account, trusting the provided genesis/head and verifying its log.
pub async fn join<H: Hold>(h: &H, invitation: &Invitation) -> Result<Ledger> {
    let me = h
        .with(|c| {
            let me = c.me()?;
            ensure(
                invitation.format == "dmsg-channel-invitation/1"
                    && invitation.account == me
                    && invitation.expires_at > now_ms()
                    && is_hash(&invitation.invitation)
                    && is_hash(&invitation.genesis)
                    && is_hash(&invitation.head)
                    && is_hash(&invitation.channel),
                Error::Forbidden,
            )?;
            Ok(me)
        })
        .await?;
    h.signed(Client::root_now).await?;
    h.with(|c| {
        ensure(invitation.expires_at > now_ms(), Error::Expired)?;
        match c.keyring(&invitation.channel) {
            Ok((_, ring)) => ensure(ring.genesis == invitation.genesis, Error::IntegrityFailed),
            Err(Error::NotFound) => c.save_keyring(
                None,
                &Keyring {
                    format: FORMAT.into(),
                    id: invitation.channel.clone(),
                    name: invitation.name.chars().take(MAX_NAME).collect(),
                    genesis: invitation.genesis.clone(),
                    keys: BTreeMap::new(),
                    muted: false,
                    hidden: false,
                },
            ),
            Err(error) => Err(error),
        }
    })
    .await?;
    let ledger = pull(
        h,
        &invitation.channel,
        Some(&invitation.invitation),
        Some(&invitation.head),
    )
    .await?;
    if !ledger.members.contains_key(&me) {
        let key = format!("accept:{}", invitation.invitation);
        let payload = h
            .with(|c| {
                if let Some(job) = c.job::<Operation>(&invitation.channel, &key)? {
                    return Ok(job.payload);
                }
                c.check_channel_head(&invitation.channel, &ledger)?;
                ensure(
                    ledger
                        .invitations
                        .get(&invitation.invitation)
                        .is_some_and(|i| i.account == me && i.expires_at > now_ms()),
                    Error::Forbidden,
                )?;
                Ok(json(&Control {
                    expected_head: ledger.head.clone(),
                    expected_acl_version: ledger.acl_version,
                    action: Action::Accept {
                        invitation_id: invitation.invitation.clone(),
                    },
                }))
            })
            .await?;
        post(h, &invitation.channel, &key, CONTROL, payload, "/control").await?;
    }
    let ledger = pull_changed(h, &invitation.channel).await?;
    if ledger.epoch > 0
        && ledger
            .members
            .get(&me)
            .is_some_and(|m| m.joined_epoch == ledger.epoch)
    {
        open_epoch(h, &invitation.channel, ledger.epoch).await?;
    }
    h.with(|c| {
        ensure(ledger.members.contains_key(&c.me()?), Error::Forbidden)?;
        let mut local = c.local(&invitation.channel)?;
        local.left = false;
        c.save_local(&invitation.channel, &local)?;
        c.channel_settings(&invitation.channel, None, None, Some(false))
            .map(|_| ())
    })
    .await?;
    Ok(ledger)
}

/// Mutate membership using the stored control on retry, never a new expected head.
pub async fn control<H: Hold>(h: &H, channel: &str, action: Action, operation: &str) -> Result<()> {
    ensure_valid(
        !operation.is_empty() && operation.len() <= 128,
        "operation ID",
    )?;
    ensure_valid(
        !matches!(
            action,
            Action::Accept { .. } | Action::Transfer { .. } | Action::History { .. }
        ),
        "action",
    )?;
    let key = format!("control:{operation}");
    let existing = h.with(|c| c.job::<Operation>(channel, &key)).await?;
    let payload = if let Some(job) = existing {
        let previous: Control = from(&job.payload)?;
        ensure(previous.action == action, Error::IdempotencyConflict)?;
        job.payload
    } else {
        let ledger = pull(h, channel, None, None).await?;
        h.with(|c| {
            c.check_channel_head(channel, &ledger)?;
            let control = Control {
                expected_head: ledger.head.clone(),
                expected_acl_version: ledger.acl_version,
                action: action.clone(),
            };
            advance(&ledger, &control, &c.me()?, &"0".repeat(64), now_ms())?;
            Ok(json(&control))
        })
        .await?
    };
    post(h, channel, &key, CONTROL, payload, "/control").await?;
    if matches!(action, Action::Leave {}) {
        h.with(|c| record_leave(c, channel)).await?;
    } else {
        pull_changed(h, channel).await?;
    }
    Ok(())
}

fn record_leave(c: &mut Client, channel: &str) -> Result<()> {
    let mut local = c.local(channel)?;
    local.left = true;
    c.save_local(channel, &local)?;
    c.channel_settings(channel, None, None, Some(true))
        .map(|_| ())
}

/// Reconcile only the stored path, payload and request ID.
pub async fn resume<H: Hold>(h: &H, channel: &str, key: &str) -> Result<Json> {
    let operation = h
        .with(|c| c.job::<Operation>(channel, key)?.ok_or(Error::NotFound))
        .await?;
    let prefix = base(channel)?;
    let suffix = operation
        .path
        .strip_prefix(&prefix)
        .ok_or(Error::IntegrityFailed)?;
    let leaving = operation.action == CONTROL
        && matches!(
            from::<Control>(&operation.payload)?.action,
            Action::Leave {}
        );
    let result = post(
        h,
        channel,
        key,
        &operation.action,
        operation.payload,
        suffix,
    )
    .await?;
    if leaving {
        h.with(|c| record_leave(c, channel)).await?;
    } else {
        pull_changed(h, channel).await?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creation_intent_survives_reopen_and_cannot_change_terms_or_discard_a_signed_genesis() {
        let dir = crate::testing::TempDir::new();
        let request = "a1".repeat(32);
        let secure = std::sync::Arc::new(crate::testing::MemoryStorage::default());
        let first = {
            let mut c = crate::testing::unlocked(&dir.0, false, Some(secure.clone()));
            c.enable_fast_unlock().unwrap();
            c.channel_prepare_creation("Reviewed channel", ChannelType::Distribution, &request)
                .unwrap()
        };
        let mut c = Client::open(crate::testing::config(false), &dir.0, Some(secure)).unwrap();
        c.unlock_fast().await.unwrap();
        let retry = c
            .channel_prepare_creation("Reviewed channel", ChannelType::Distribution, &request)
            .unwrap();
        assert_eq!(first, retry);
        assert!(matches!(
            c.channel_prepare_creation("Changed", ChannelType::Distribution, &request),
            Err(Error::IdempotencyConflict)
        ));
        assert!(matches!(
            c.channel_prepare_creation("Reviewed channel", ChannelType::Collaboration, &request),
            Err(Error::IdempotencyConflict)
        ));
        c.save_job(
            &first.channel,
            "genesis",
            &Operation {
                action: GENESIS.into(),
                path: base(&first.channel).unwrap(),
                request_id: request.clone(),
                deadline: now_ms() + 60_000,
                cose: "signed command".into(),
                payload: json!({}),
                result: None,
                fixed: false,
                notification: None,
            },
        )
        .unwrap();
        assert!(matches!(
            c.channel_discard_creation(&request),
            Err(Error::ExecutionUnknown)
        ));
        assert_eq!(c.channel_creations().unwrap(), vec![first]);
        let draft = c
            .channel_prepare_creation(
                "Offline draft",
                ChannelType::Collaboration,
                &"b1".repeat(32),
            )
            .unwrap();
        c.channel_discard_creation(&draft.request).unwrap();
        assert_eq!(c.channel_creations().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn retrying_a_control_cannot_replace_the_original_action() {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let channel = "ab".repeat(32);
        crate::testing::joined_channel(&mut c, &channel);
        let request = "cd".repeat(32);
        c.save_job(
            &channel,
            &format!("control:{request}"),
            &Operation {
                action: CONTROL.into(),
                path: format!("{}/control", base(&channel).unwrap()),
                request_id: request.clone(),
                deadline: now_ms() + 60_000,
                cose: "original signed command".into(),
                payload: json(&Control {
                    expected_head: "ef".repeat(32),
                    expected_acl_version: 1,
                    action: Action::Archive {},
                }),
                result: Some(json!({"committed":true})),
                fixed: false,
                notification: None,
            },
        )
        .unwrap();
        assert!(matches!(
            control(&c.hold(), &channel, Action::Rekey {}, &request).await,
            Err(Error::IdempotencyConflict)
        ));
    }
}

impl Client {
    /// Original invitations prepared on this device, including attempts before posting.
    pub fn channel_invitation_attempts(&self, channel: &str) -> Result<Vec<Json>> {
        base(channel)?;
        let prefix = format!("{JOB}{channel}:invite-intent:");
        let local = self.local(channel)?;
        let mut attempts = Vec::new();
        for key in self.workspace.store.sealed_keys(&prefix)? {
            let request = key.strip_prefix(&prefix).ok_or(Error::IntegrityFailed)?;
            let intent: Inviting = self
                .workspace
                .private(self.keys()?, &key)?
                .ok_or(Error::IntegrityFailed)?;
            let link = local
                .invites
                .get(request)
                .map(|packet| {
                    serde_json::to_vec(packet)
                        .map(|bytes| format!("dmsg://channel/join#{}", b64(&bytes)))
                        .map_err(|_| Error::IntegrityFailed)
                })
                .transpose()?;
            attempts.push(json!({"request":request,"account":intent.account,"role":intent.role,"expires_at":intent.expires_at,"link":link}));
        }
        attempts.sort_by_key(|value| std::cmp::Reverse(value["expires_at"].as_u64()));
        Ok(attempts)
    }
}

#[cfg(test)]
mod invitation_recovery_tests {
    use super::*;

    #[tokio::test]
    async fn an_invitation_is_discoverable_after_restart_even_before_its_first_post() {
        let dir = crate::testing::TempDir::new();
        let secure = std::sync::Arc::new(crate::testing::MemoryStorage::default());
        let channel = "ab".repeat(32);
        let request = "cd".repeat(32);
        let target = "040g2082040g2082040g".parse::<AccountId>().unwrap();
        {
            let mut c = crate::testing::unlocked(&dir.0, false, Some(secure.clone()));
            crate::testing::joined_channel(&mut c, &channel);
            c.enable_fast_unlock().unwrap();
            assert!(
                invite_named(&c.hold(), &channel, &target, Role::Member, &request)
                    .await
                    .is_err()
            );
            assert!(
                c.job::<Operation>(&channel, &format!("control:{request}"))
                    .unwrap()
                    .is_none()
            );
        }
        let mut c = Client::open(crate::testing::config(false), &dir.0, Some(secure)).unwrap();
        c.unlock_fast().await.unwrap();
        let original = c.channel_invitation_attempts(&channel).unwrap();
        assert_eq!(original.len(), 1);
        assert_eq!(original[0]["request"], request);
        assert_eq!(original[0]["account"], target.to_string());
        assert!(
            invite_named(&c.hold(), &channel, &target, Role::Member, &request)
                .await
                .is_err()
        );
        assert_eq!(c.channel_invitation_attempts(&channel).unwrap(), original);
    }
}
