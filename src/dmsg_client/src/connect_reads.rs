//! Mailbox admission and delivery release Client ownership at every network wait.
use super::*;

/// Process sealed mailbox entries. Approved mutations keep their own journal runner.
pub async fn entries<H: Hold>(h: &H, fetched: u64) -> Result<ConnectReport> {
    let mut report = ConnectReport {
        fetched,
        ..Default::default()
    };
    let sealed = h
        .with(|c| {
            c.keys
                .is_none()
                .then(|| {
                    c.workspace
                        .store
                        .inbox_entries::<Entry>(true)
                        .map(|e| e.len() as u64)
                })
                .transpose()
        })
        .await?;
    if let Some(sealed) = sealed {
        report.sealed = sealed;
        return Ok(report);
    }
    let bound = crate::hold::Unlocked::new(h).await?;
    let h = &bound;
    let pending = h
        .with(|c| c.workspace.store.inbox_entries::<Entry>(true))
        .await?;
    let mut checks = if pending.is_empty() {
        None
    } else {
        Some(checks(h).await?)
    };
    for entry in pending {
        match handle(h, &entry, checks.as_mut().expect("checks")).await {
            Ok(Handled::Queued) => report.queued += 1,
            Ok(Handled::Refused(code)) => {
                report.refused += 1;
                report.refusals.push(code);
            }
            Ok(Handled::Replied) => report.replied += 1,
            Ok(Handled::Ignored) => report.ignored += 1,
            Err(error @ (Error::Unavailable(_) | Error::PolicyStale | Error::ExecutionUnknown)) => {
                report.deferred.push(error.to_string());
                continue;
            }
            Err(error) => return Err(error),
        }
        h.with(|c| {
            c.workspace
                .store
                .put_inbox(&hex::encode(*entry.request_id), entry.seq, &entry, true)
        })
        .await?;
    }
    for key in h.with(|c| c.connect_follow_local()).await? {
        let reconcile = h
            .with(|c| {
                let op = c.operation(&key)?;
                Ok(op.checkout.is_none()
                    && matches!(
                        op.state,
                        OpState::Authorizing | OpState::Authorized | OpState::ExecutionUnknown
                    ))
            })
            .await?;
        if reconcile {
            match reconcile_read(h, &key).await {
                Ok(()) | Err(Error::ExecutionUnknown) => {}
                Err(Error::Unavailable(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        report.replied += deliver(h, &key).await?;
    }
    Ok(report)
}

async fn checks<H: Hold>(h: &H) -> Result<Checks> {
    let path = h.with(|c| c.connect_path("/pairings")).await?;
    let data = h.get(&path).await?;
    h.with(|c| {
        let security = c.fresh()?;
        let now = now_ms();
        let mut pairings = BTreeMap::new();
        for p in data["pairings"].as_array().ok_or(Error::IntegrityFailed)? {
            if p["state"] != "active" {
                continue;
            }
            if let Ok((grant, _)) =
                c.check_grant(&security, p["grant"].as_str().unwrap_or_default(), now)
            {
                pairings.insert(
                    grant.pairing_id,
                    Pairing {
                        public: grant.pairing_pub.to_vec(),
                        label: grant.label,
                    },
                );
            }
        }
        Ok(Checks {
            pairings,
            commerce: None,
            apps: BTreeMap::new(),
        })
    })
    .await
}

async fn handle<H: Hold>(h: &H, entry: &Entry, checks: &mut Checks) -> Result<Handled> {
    let Some((request, channel, checked)) = h.with(|c| c.connect_decode(entry, checks)).await?
    else {
        return Ok(Handled::Ignored);
    };
    let key = operation_key(&request.origin, &request.command);
    if let Err(error) = checked {
        let reason = refusal(&error);
        let code = reason.0;
        refuse(h, &request, channel, reason, &key).await?;
        return Ok(Handled::Refused(code.into()));
    }
    if matches!(request.kind, RequestKind::Read | RequestKind::Open) {
        let answer = h
            .with(|c| {
                let device = c.workspace.meta.device_id;
                match c.operation(&key) {
                    Ok(op)
                        if op.command.public_key == request.command.public_key
                            && op.pinned.is_none_or(|p| p == device)
                            && op.state != OpState::Elsewhere =>
                    {
                        if request.kind == RequestKind::Open
                            && op.state == OpState::AwaitingUser
                            && let Ok(mut approval) = c.approval_record(&op.approval)
                        {
                            approval.updated_at = now_ms();
                            c.save_approval(&approval)?;
                        }
                        Ok((op.digest, ResponseState::Closed, Some(op.value())))
                    }
                    _ => Ok((Hash::new([0; 32]), ResponseState::Unknown, None)),
                }
            })
            .await?;
        reply(
            h,
            &mut channel.clone(),
            answer.0,
            answer.1,
            answer.2.as_ref(),
        )
        .await?;
        return Ok(Handled::Replied);
    }
    let existing = h
        .with(|c| {
            let Ok(mut op) = c.operation(&key) else {
                return Ok(None);
            };
            if op.digest != operation_digest(&request.origin, &request.command)? {
                return Ok(Some(false));
            }
            if !op
                .channels
                .iter()
                .any(|ch| ch.request_id == channel.request_id)
            {
                op.channels.push(channel.clone());
                c.save_operation(&op)?;
            }
            Ok(Some(true))
        })
        .await?;
    if let Some(matches) = existing {
        if matches {
            deliver(h, &key).await?;
            return Ok(Handled::Replied);
        }
        refuse(h, &request, channel, Refusal("IDEMPOTENCY_CONFLICT"), &key).await?;
        return Ok(Handled::Refused("IDEMPOTENCY_CONFLICT".into()));
    }
    if entry.state != "pending" {
        return Ok(Handled::Ignored);
    }
    let admitted: Result<Option<String>> = async {
        let app = if let Some(app) = checks.apps.get(&request.command.app_id) {
            app.clone()
        } else {
            let (home, environment) = h
                .with(|c| Ok((c.account()?.home, c.config.environment.clone())))
                .await?;
            let app = registration(
                &h.anonymous().await?,
                home,
                &environment,
                &mut checks.commerce,
                &request.command.app_id,
            )
            .await?;
            checks.apps.insert(app.app_id.clone(), app.clone());
            app
        };
        let state = if h.with(|c| Ok(c.session().is_ok())).await? {
            Some(crate::account::refreshed(h).await?)
        } else {
            None
        };
        h.with(|c| -> Result<Option<String>> {
            let pairing = checks
                .pairings
                .get(&entry.pairing_id)
                .ok_or(Error::Forbidden)?;
            let mut op = c.admit_checked(&request, pairing, &app, state.as_ref(), now_ms())?;
            match c.approval_create(
                &format!("connect:{}", request.origin),
                Action::Connect {
                    operation: op.key.clone(),
                    digest: op.digest,
                    checkout: None,
                },
                request.expires_at,
            ) {
                Ok(approval) => {
                    op.approval = approval.id;
                    c.save_operation(&op)?;
                    Ok(None)
                }
                Err(error) => {
                    let code = refusal(&error).0;
                    op.state = OpState::Rejected;
                    op.error = Some(code.into());
                    c.save_operation(&op)?;
                    Ok(Some(code.to_string()))
                }
            }
        })
        .await
    }
    .await;
    match admitted {
        Ok(None) => Ok(Handled::Queued),
        Ok(Some(code)) => {
            deliver(h, &key).await?;
            Ok(Handled::Refused(code))
        }
        Err(error @ Error::Unavailable(_)) => Err(error),
        Err(error) => {
            let reason = if error == Error::NotFound {
                Refusal("APP_NOT_REGISTERED")
            } else {
                admission_refusal(&error)
            };
            let code = reason.0;
            refuse(h, &request, channel, reason, &key).await?;
            Ok(Handled::Refused(code.into()))
        }
    }
}

async fn refuse<H: Hold>(
    h: &H,
    request: &ConnectRequest,
    mut channel: Channel,
    reason: Refusal,
    key: &str,
) -> Result<()> {
    let value = json!({"operationId":request.command.operation_id,"kind":operation_kind(&request.command.method).unwrap_or("document"),"state":"rejected","error":reason.0});
    let digest = operation_digest(&request.origin, &request.command)
        .unwrap_or_else(|_| digest("dmsg/connect-key/v1", &key));
    reply(h, &mut channel, digest, ResponseState::Closed, Some(&value)).await
}

async fn reply<H: Hold>(
    h: &H,
    channel: &mut Channel,
    operation: Hash,
    state: ResponseState,
    value: Option<&Json>,
) -> Result<()> {
    for attempt in 0..2 {
        let version = channel.version + 1;
        let (path, sealed) = h
            .with(|c| {
                let sealed = value
                    .map(|value| {
                        seal_response(
                            &ConnectResponse {
                                protocol: CONNECT_PROTOCOL.into(),
                                request_id: channel.request_id,
                                version,
                                operation_digest: operation,
                                value: serde_json::to_vec(value).expect("JSON").into(),
                            },
                            &c.account()?.id,
                            &channel.pairing_id,
                            &c.workspace.meta.device_id,
                            &c.keys()?.hpke,
                            &channel.result_key,
                        )
                        .map(|s| canonical(&s))
                    })
                    .transpose()?;
                Ok((
                    c.connect_path(&format!(
                        "/requests/{}/respond",
                        hex::encode(*channel.request_id)
                    ))?,
                    sealed,
                ))
            })
            .await?;
        let command = RespondCommand {
            request_id: channel.request_id,
            version,
            state,
            sealed_digest: sealed.as_ref().map(|s| sha256(s)),
        };
        let id = Hash::new(random_id());
        match h
            .send(|c| c.signed_post_sealed(&path, RESPOND_ACTION, &command, &id, sealed.as_deref()))
            .await
        {
            Ok(_) => {
                channel.version = version;
                channel.sent = value.and_then(|v| v["state"].as_str().map(str::to_string));
                channel.closed = state != ResponseState::Open;
                return Ok(());
            }
            Err(Error::VersionConflict) if attempt == 0 => {
                let path = h
                    .with(|c| {
                        c.connect_path(&format!("/requests/{}", hex::encode(*channel.request_id)))
                    })
                    .await?;
                channel.version = Entry::parse(&h.get(&path).await?)?.version;
            }
            Err(Error::Expired | Error::NotFound | Error::Forbidden | Error::ResultExpired) => {
                channel.closed = true;
                return Ok(());
            }
            Err(error) => return Err(error),
        }
    }
    Err(Error::VersionConflict)
}

pub(super) async fn deliver<H: Hold>(h: &H, key: &str) -> Result<u64> {
    let op = h.with(|c| c.operation(key)).await?;
    let state = if op.state.terminal() {
        ResponseState::Closed
    } else {
        ResponseState::Open
    };
    let value = op.value();
    let name = op.state.name();
    let mut written = 0;
    for (i, before) in op.channels.iter().enumerate() {
        if before.closed
            || before.sent.as_deref() == Some(name)
            || (i == 0 && op.state == OpState::AwaitingUser)
        {
            continue;
        }
        let mut channel = before.clone();
        reply(h, &mut channel, op.digest, state, Some(&value)).await?;
        h.with(|c| {
            let mut current = c.operation(key)?;
            if let Some(saved) = current
                .channels
                .iter_mut()
                .find(|ch| ch.request_id == channel.request_id)
                && saved.version <= channel.version
            {
                *saved = channel;
                c.save_operation(&current)?;
            }
            Ok(())
        })
        .await?;
        written += 1;
    }
    Ok(written)
}

/// Settle connection reads under the same operation lane as mailbox delivery.
/// Payment execution continues to use the account/payment journal runner.
pub async fn reconcile_reads<H: Hold>(h: &H) -> Result<usize> {
    let bound = crate::hold::Unlocked::new(h).await?;
    let h = &bound;
    let keys = h.with(|c| c.connect_pending_reconciliations()).await?;
    let count = keys.len();
    for key in keys {
        let outcome = reconcile_read(h, &key).await;
        h.with(|c| c.connect_reconciled_approval(&key, outcome))
            .await?;
    }
    Ok(count)
}

async fn reconcile_read<H: Hold>(h: &H, key: &str) -> Result<()> {
    let (op, device) = h
        .with(|c| Ok((c.operation(key)?, c.workspace.meta.device_id)))
        .await?;
    if op.state.terminal() {
        return match op.state {
            OpState::Signed | OpState::Returned => Ok(()),
            _ => Err(Error::Expired),
        };
    }
    if op.state == OpState::Authorizing {
        let request = op
            .channels
            .first()
            .ok_or(Error::IntegrityFailed)?
            .request_id;
        let path = h
            .with(|c| c.connect_path(&format!("/requests/{}", hex::encode(*request))))
            .await?;
        let entry = Entry::parse(&h.get(&path).await?)?;
        return h
            .with(|c| {
                let mut current = c.operation(key)?;
                if current.state != OpState::Authorizing {
                    return Ok(());
                }
                if entry.state == "authorized" && entry.pinned == Some(device) {
                    current.pinned = Some(device);
                    current.state = OpState::Authorized;
                } else if entry.state == "pending" {
                    current.state = OpState::AwaitingUser;
                } else {
                    current.state = OpState::Elsewhere;
                }
                c.save_operation(&current)
            })
            .await;
    }
    let outcome = match (&op.execution, &op.auth) {
        (Some(execution), _) => crate::attest::reconcile(h, execution)
            .await
            .and_then(|a| attestation_result(&a)),
        (None, Some(job)) => authentication_result(h, job).await,
        (None, None) => Err(Error::Expired),
    };
    let result = h
        .with(|c| {
            let current = c.operation(key)?;
            if current.state.terminal() || current.state == OpState::Elsewhere {
                return Ok(());
            }
            c.connect_outcome_local(key, outcome)
        })
        .await;
    let _ = deliver(h, key).await;
    result
}

pub(super) async fn authentication_result<H: Hold>(h: &H, job: &AuthJob) -> Result<Json> {
    h.ensure_login().await?;
    let (account, ic) = h
        .with(|c| Ok((c.account()?.clone(), c.session()?.ic.clone())))
        .await?;
    loop {
        let batch: Result<CertifiedBatch> = ic
            .query(
                account.home,
                "authentication_certificate",
                (account.id, job.request.operation_id),
            )
            .await?;
        let batch = match batch {
            Ok(batch) => batch,
            Err(Error::NotFound) if now_ms() < job.request.expires_at_ms => {
                h.with(|c| {
                    c.keys()?;
                    Ok(())
                })
                .await?;
                let reply: Result<dmsg_types::integration::AuthenticationResult> = ic
                    .update(
                        account.home,
                        "approve_authentication",
                        (account.id, &job.request, &job.approval),
                    )
                    .await?;
                reply?;
                continue;
            }
            Err(error) => return Err(error),
        };
        return h
            .with(|_| {
                let result = verify_authentication(
                    &batch,
                    &job.request,
                    account.home,
                    &ic.root_key(),
                    now_ms(),
                )?;
                ensure(result.account_id == account.id, Error::IntegrityFailed)?;
                Ok(json!({"proof": STANDARD.encode(canonical(&batch))}))
            })
            .await;
    }
}
