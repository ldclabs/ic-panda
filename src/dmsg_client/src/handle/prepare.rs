//! Name preparation performs reads outside the shared client. No name or
//! payment is committed until its existing approval flow executes the journal.

use super::*;

/// Prepare a legacy claim without holding the client during queries.
/// # Errors
/// As [`Client::handle_claim_prepare`].
pub async fn claim_prepare<H: Hold>(h: &H, name: &str) -> Result<HandleJob> {
    let h = Unlocked::new(h).await?;
    let name = normalize_handle(name)?;
    let (registry, account, ic) = h
        .with(|c| Ok((c.config.handle, c.account()?.id, c.session()?.ic.clone())))
        .await?;
    let (progress, reservation): (SnapshotProgress, Result<Option<LegacyReservation>>) = futures_util::try_join!(
        ic.query(registry, "snapshot_progress", ()),
        ic.query(registry, "get_legacy_reservation", (&name,))
    )?;
    let snapshot = progress
        .snapshot
        .filter(|_| progress.sealed)
        .ok_or(Error::Pending)?;
    let reservation = reservation?.ok_or(Error::NotFound)?;
    ensure(reservation.handle == name, Error::IntegrityFailed)?;
    ensure(!reservation.quarantined, Error::Locked)?;
    h.with(|c| {
        let now = now_ms();
        let caller = c.claim_login(&reservation, now).ok_or(Error::Forbidden)?;
        let intent = HandleIntent {
            handle_canister: c.config.handle,
            action: HandleAction::ClaimLegacy,
            account_id: account,
            target_account: None,
            handle: name.clone(),
            expected_version: 0,
            op_id: Hash::new(crate::crypto::random_id()),
            terms_digest: digest(
                "dmsg/legacy-claim/v1",
                &(snapshot.snapshot_id, &reservation, &account),
            ),
        };
        let job = HandleJob {
            id: hex::encode(*intent.op_id),
            name,
            caller,
            intent,
            kind: Kind::Claim {
                snapshot: snapshot.snapshot_id,
            },
            phase: Phase::Prepared,
            created_at_ms: Some(now),
            until_ms: None,
        };
        c.save_handle_job(&job)?;
        Ok(job)
    })
    .await
}

/// Prepare registration while leaving local reads and locking available.
/// # Errors
/// As [`Client::handle_register_prepare`].
pub async fn register_prepare<H: Hold>(h: &H, name: &str) -> Result<HandleJob> {
    let h = Unlocked::new(h).await?;
    let display_handle = name.to_string();
    let name = normalize_handle(name)?;
    let registry = h.with(|c| Ok(c.config.handle)).await?;
    let ic = h.reader().await?;
    let names = [name.clone()];
    let (owners, progress, reserved, config): (
        _,
        SnapshotProgress,
        Result<Option<LegacyReservation>>,
        HandleConfig,
    ) = futures_util::try_join!(
        certified_records(&ic, registry, &names),
        ic.query(registry, "snapshot_progress", ()),
        ic.query(registry, "get_legacy_reservation", (&name,)),
        ic.query(registry, "get_handle_config", ())
    )?;
    ensure(
        owners.into_iter().all(|r| r.is_none()),
        Error::IdempotencyConflict,
    )?;
    ensure(progress.sealed, Error::LegacyWriteDisabled)?;
    ensure(reserved?.is_none(), Error::VersionConflict)?;
    let total = price(&name);
    ensure(total > config.ledger_fee, Error::FeeBlocked)?;
    let (payer, _) = crate::payer::payer(&h).await?;
    h.with(|c| {
        c.ensure_payer(payer)?;
        ensure(
            config.user_homes.contains(&c.account()?.home),
            Error::IntegrityFailed,
        )?;
        let account = Account {
            owner: payer,
            subaccount: None,
        };
        let intent = HandleIntent {
            handle_canister: c.config.handle,
            action: HandleAction::Register,
            account_id: c.account()?.id,
            target_account: None,
            handle: name.clone(),
            expected_version: 0,
            op_id: Hash::new(crate::crypto::random_id()),
            terms_digest: registration_terms_digest(
                config.ledger,
                &account,
                total - config.ledger_fee,
                config.ledger_fee,
                Some(&display_handle),
            ),
        };
        let job = HandleJob {
            id: hex::encode(*intent.op_id),
            name,
            caller: payer,
            intent: intent.clone(),
            kind: Kind::Register {
                registration: Box::new(Registration {
                    intent,
                    payer: account,
                    fee: config.ledger_fee,
                    display_handle: Some(display_handle),
                }),
                ledger: config.ledger,
                total,
            },
            phase: Phase::Prepared,
            created_at_ms: Some(now_ms()),
            until_ms: None,
        };
        c.save_handle_job(&job)?;
        Ok(job)
    })
    .await
}

/// Prepare a transfer token after proving the current owner.
/// # Errors
/// As [`Client::handle_transfer_prepare`].
pub async fn transfer_prepare<H: Hold>(
    h: &H,
    name: &str,
    target: &AccountId,
) -> Result<(HandleJob, String)> {
    let h = Unlocked::new(h).await?;
    let name = normalize_handle(name)?;
    let owner = h.with(|c| Ok(c.account()?.id)).await?;
    let record = resolve(&h, &name).await?.ok_or(Error::NotFound)?;
    ensure(
        record.owner_account == owner && *target != owner,
        Error::Forbidden,
    )?;
    h.with(|c| {
        let op_id = Hash::new(crate::crypto::random_id());
        let registry = c.config.handle;
        let from = HandleIntent {
            handle_canister: registry,
            action: HandleAction::Transfer,
            account_id: owner,
            target_account: Some(*target),
            handle: name.clone(),
            expected_version: record.version,
            op_id,
            terms_digest: transfer_terms(registry, &name, &owner, target, record.version, op_id),
        };
        let accept = HandleIntent {
            action: HandleAction::AcceptTransfer,
            account_id: *target,
            target_account: Some(owner),
            ..from.clone()
        };
        let token = transfer_token(&from, &accept);
        let job = HandleJob {
            id: hex::encode(*op_id),
            name,
            caller: c.session()?.principal,
            intent: from.clone(),
            kind: Kind::Transfer {
                from: Box::new(from),
                accept: Box::new(accept),
            },
            phase: Phase::Prepared,
            created_at_ms: Some(now_ms()),
            until_ms: None,
        };
        c.save_handle_job(&job)?;
        Ok((job, token))
    })
    .await
}

/// Prepare accepting a transfer after checking its token and certified owner.
/// # Errors
/// As [`Client::handle_accept_prepare`].
pub async fn accept_prepare<H: Hold>(h: &H, token: &str) -> Result<HandleJob> {
    let h = Unlocked::new(h).await?;

    ensure(token.len() <= 4096, Error::QuotaExceeded)?;
    let packet: TransferPacket = crate::codec::decode(&crate::codec::unb64(token)?, 4096)?;
    let (from, accept) = (packet.from, packet.accept);
    let registry = h.with(|c| Ok(c.config.handle)).await?;
    let terms = transfer_terms(
        registry,
        &from.handle,
        &from.account_id,
        &accept.account_id,
        from.expected_version,
        from.op_id,
    );
    ensure(
        packet.format == TRANSFER
            && from.action == HandleAction::Transfer
            && accept.action == HandleAction::AcceptTransfer
            && from.handle_canister == registry
            && accept.handle_canister == registry
            && normalize_handle(&from.handle)? == from.handle
            && from.handle == accept.handle
            && from.expected_version == accept.expected_version
            && from.op_id == accept.op_id
            && from.target_account == Some(accept.account_id)
            && accept.target_account == Some(from.account_id)
            && from.terms_digest == terms
            && accept.terms_digest == terms,
        Error::IntegrityFailed,
    )?;
    ensure(
        accept.account_id == h.with(|c| Ok(c.account()?.id)).await?,
        Error::Forbidden,
    )?;
    let record = resolve(&h, &from.handle).await?.ok_or(Error::NotFound)?;
    ensure(
        record.owner_account == from.account_id && record.version == from.expected_version,
        Error::VersionConflict,
    )?;
    h.with(|c| {
        let job = HandleJob {
            id: hex::encode(*accept.op_id),
            name: from.handle.clone(),
            caller: c.session()?.principal,
            intent: accept.clone(),
            kind: Kind::Transfer {
                from: Box::new(from),
                accept: Box::new(accept),
            },
            phase: Phase::Prepared,
            created_at_ms: Some(now_ms()),
            until_ms: None,
        };
        c.save_handle_job(&job)?;
        Ok(job)
    })
    .await
}
