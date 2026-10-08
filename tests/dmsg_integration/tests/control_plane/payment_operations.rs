use super::*;

fn limits(f: &Fixture) -> PaymentLimits {
    let config: PaymentInit = query(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "payment_config",
        (),
    );
    config.limits
}

fn stats(f: &Fixture) -> PaymentStats {
    query(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "payment_stats",
        (),
    )
}

fn open(f: &Fixture, recipient: &AccountId, nonce: u8) -> Result<EscrowInfo> {
    update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (f.order(recipient, 2, nonce),),
    )
}

/// Upgrade as an emergency would, without running the old module's
/// `pre_upgrade`, so only what was already persisted survives.
fn upgrade_skipping_pre_upgrade(f: &Fixture) {
    #[derive(CandidType, Deserialize)]
    struct Flags {
        skip_pre_upgrade: Option<bool>,
    }
    #[derive(CandidType, Deserialize)]
    enum Mode {
        #[serde(rename = "upgrade")]
        Upgrade(Option<Flags>),
    }
    #[derive(CandidType)]
    struct InstallCode {
        mode: Mode,
        canister_id: Principal,
        wasm_module: ByteBuf,
        arg: ByteBuf,
    }
    use std::io::Write;
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gzip.write_all(&wasm("dmsg_payment")).unwrap();
    let install =
        f.ic.submit_call_with_effective_principal(
            Principal::management_canister(),
            pocket_ic::common::rest::RawEffectivePrincipal::CanisterId(
                f.payment.as_slice().to_vec(),
            ),
            Principal::anonymous(),
            "install_code",
            candid::encode_one(InstallCode {
                mode: Mode::Upgrade(Some(Flags {
                    skip_pre_upgrade: Some(true),
                })),
                canister_id: f.payment,
                wasm_module: gzip.finish().unwrap().into(),
                arg: candid::encode_args(()).unwrap().into(),
            })
            .unwrap(),
        )
        .unwrap();
    f.ic.await_call(install).unwrap();
}

#[test]
fn governed_limits_bound_admission_and_admin_changes_persist_without_pre_upgrade() {
    let f = Fixture::new();
    let governance = Principal::from_slice(&[90]);
    let recipient = f.create(2);
    open(&f, &recipient, 1).unwrap();
    let mut next = limits(&f);
    next.max_escrows = 2;
    next.ledger_calls_per_caller = 1_000;
    let invalid = PaymentLimits {
        max_escrows: MAX_PAYMENT_ESCROWS + 1,
        ..next.clone()
    };
    let rejected: std::result::Result<String, String> = query(
        &f.ic,
        f.payment,
        person(9),
        "validate_admin_set_limits",
        (&invalid,),
    );
    assert!(rejected.unwrap_err().starts_with("InvalidInput"));
    let rendered: std::result::Result<String, String> = query(
        &f.ic,
        f.payment,
        person(9),
        "validate_admin_set_limits",
        (&next,),
    );
    assert!(rendered
        .unwrap()
        .starts_with("Set payment limits: 2 escrows"));
    assert_denied(&f.ic, f.payment, person(9), "admin_set_limits", (&next,));
    let set: Result<()> = update(&f.ic, f.payment, governance, "admin_set_limits", (&next,));
    set.unwrap();
    // Other administrative changes, which `pre_upgrade` used to carry alone.
    let disabled: Result<()> = update(&f.ic, f.payment, governance, "set_orders_enabled", (false,));
    disabled.unwrap();
    let fee: Result<()> = update(&f.ic, f.payment, governance, "set_ledger_fee", (15u128,));
    fee.unwrap();

    upgrade_skipping_pre_upgrade(&f);
    let config: PaymentInit = query(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "payment_config",
        (),
    );
    assert_eq!(
        (config.limits, config.enabled, config.ledger_fee),
        (next, false, 15)
    );
    let certified: Result<CertifiedBatch> = query(
        &f.ic,
        f.payment,
        person(9),
        "get_configuration_certified",
        (None::<u64>, None::<u64>),
    );
    let leaf: PaymentConfiguration =
        cbor2::from_slice(certified.unwrap().entries[0].value.as_ref().unwrap()).unwrap();
    assert_eq!((leaf.max_escrows, leaf.enabled), (2, false));

    let enabled: Result<()> = update(&f.ic, f.payment, governance, "set_orders_enabled", (true,));
    enabled.unwrap();
    open(&f, &recipient, 2).unwrap();
    // The limit counts every escrow ever admitted; only new orders stop.
    assert_eq!(open(&f, &recipient, 3), Err(Error::QuotaExceeded));
    let s = stats(&f);
    assert_eq!((s.escrows, s.orders_today, s.open_escrows), (2, 2, 2));
}

#[test]
fn expired_undecided_escrows_release_their_payer_slots_on_the_next_open() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let abandoned: Vec<EscrowInfo> = (1..=4).map(|n| open(&f, &recipient, n).unwrap()).collect();
    assert_eq!(open(&f, &recipient, 5), Err(Error::QuotaExceeded));
    f.ic.advance_time(Duration::from_millis(45 * MINUTE));
    f.ic.tick();
    let fresh = open(&f, &recipient, 6).unwrap();
    for e in abandoned {
        let decided: Result<EscrowInfo> =
            query(&f.ic, f.payment, person(9), "get_escrow", (e.escrow_id,));
        assert_eq!(decided.unwrap().decision, FundsDecision::RefundCommitted);
    }
    assert_eq!(fresh.decision, FundsDecision::Pending);
    assert_eq!(stats(&f).open_escrows, 1);
}

#[test]
fn pending_transfers_page_settlement_payouts_until_they_succeed() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let e = open(&f, &recipient, 1).unwrap();
    let block = f.fund(&e, e.quote.amount);
    let funded: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    funded.unwrap();
    let decided: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(99),
        "finalize_receipt",
        (f.receipt(&e),),
    );
    decided.unwrap();
    let pending: Vec<TransferLeg> = query(
        &f.ic,
        f.payment,
        person(9),
        "list_pending_transfers",
        (None::<(u64, Hash, u64)>,),
    );
    assert_eq!(
        pending.iter().map(|l| &l.kind).collect::<Vec<_>>(),
        [&LegKind::Recipient, &LegKind::Platform]
    );
    let first = &pending[0];
    let rest: Vec<TransferLeg> = query(
        &f.ic,
        f.payment,
        person(9),
        "list_pending_transfers",
        (Some((first.created_at_time, first.escrow_id, first.leg_id)),),
    );
    assert_eq!(rest, pending[1..]);
    assert_eq!(stats(&f).pending_transfers, 2);
    // A dispatcher with no stake in the escrow drives every payout.
    for leg in &pending {
        let sent: Result<TransferLeg> = update(
            &f.ic,
            f.payment,
            person(77),
            "process_transfer",
            (leg.escrow_id, leg.leg_id),
        );
        assert_eq!(sent.unwrap().status, LegStatus::Succeeded);
    }
    let pending: Vec<TransferLeg> = query(
        &f.ic,
        f.payment,
        person(9),
        "list_pending_transfers",
        (None::<(u64, Hash, u64)>,),
    );
    assert!(pending.is_empty());
    assert_eq!(stats(&f).pending_transfers, 0);
}

#[test]
fn ingress_without_standing_is_refused_before_execution() {
    let f = Fixture::new();
    let recipient = f.create(2);
    for (caller, method, args) in [
        (
            person(9),
            "set_orders_enabled",
            candid::encode_args((false,)).unwrap(),
        ),
        (
            Principal::anonymous(),
            "open_escrow",
            candid::encode_args((f.order(&recipient, 2, 1),)).unwrap(),
        ),
        (
            Principal::anonymous(),
            "claim_refund",
            candid::encode_args((Hash::new([1; 32]), Vec::<u64>::new(), true)).unwrap(),
        ),
    ] {
        let reject =
            f.ic.update_call(f.payment, caller, method, args)
                .unwrap_err();
        assert_eq!(
            reject.error_code,
            pocket_ic::ErrorCode::CanisterRejectedMessage,
            "{method}"
        );
    }
    // Anyone may still trigger expiry, funding and payouts.
    let missing: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "expiry_refund",
        (Hash::new([1; 32]),),
    );
    assert_eq!(missing, Err(Error::NotFound));
}

/// Loads host-built images written by dmsg_payment's `capacity_image` from
/// DMSG_PAYMENT_IMAGE_DIR and measures the upgrade, a certified read and the
/// writes of an expiry refund and a refund claim at that size.
#[test]
#[ignore = "100k/1M-escrow capacity; build the images with dmsg_payment capacity_image first"]
fn payment_capacity_profile() {
    let dir = PathBuf::from(std::env::var_os("DMSG_PAYMENT_IMAGE_DIR").expect("image dir"));
    // Matches dmsg_payment::capacity: clock, payers, deposits and the
    // undecided escrow at every thousandth position.
    let at = 1_800_000_000_000u64;
    let payer = |i: u64| Principal::self_authenticating(i.to_be_bytes());
    let escrow_id = |i: u64| {
        digest(
            "dmsg/escrow-id/v1",
            &(
                Principal::from_slice(&[5, 1]),
                payer(i),
                digest("capacity operation", &i),
            ),
        )
    };
    for escrows in [100_000u64, 1_000_000] {
        let Ok(image) = std::fs::read(dir.join(format!("payment-{escrows}.bin"))) else {
            continue;
        };
        let ic = PocketIcBuilder::new().with_application_subnet().build();
        ic.set_time(pocket_ic::Time::from_nanos_since_unix_epoch(
            millis_to_nanos(at).unwrap(),
        ));
        let payment = ic.create_canister();
        ic.add_cycles(payment, 100_000_000_000_000_000);
        // An empty module has no pre_upgrade that could write over the image.
        ic.install_canister(payment, b"\0asm\x01\0\0\0".to_vec(), vec![], None);
        let image_bytes = image.len();
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gzip, &image).unwrap();
        drop(image);
        ic.set_stable_memory(
            payment,
            gzip.finish().unwrap(),
            pocket_ic::common::rest::BlobCompression::Gzip,
        );
        let cycles = ic.cycle_balance(payment);
        ic.upgrade_canister(
            payment,
            wasm("dmsg_payment"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
        let upgrade_cycles = cycles - ic.cycle_balance(payment);
        let upgrade = ic
            .fetch_canister_logs(payment, Principal::anonymous())
            .unwrap()
            .into_iter()
            .map(|l| String::from_utf8_lossy(&l.content).into_owned())
            .find(|l| l.contains("payment_upgrade"))
            .unwrap();
        let ids: Vec<Hash> = (0..32).map(|n| escrow_id(n * (escrows / 32) + 1)).collect();
        let began = std::time::Instant::now();
        let certified: Result<CertifiedBatch> =
            query(&ic, payment, person(1), "get_escrow_certified", (ids,));
        let elapsed = began.elapsed();
        let certified = certified.unwrap();
        assert!(certified.entries.iter().all(|e| e.value.is_some()));
        let batch_bytes = candid::encode_one(Ok::<_, Error>(&certified))
            .unwrap()
            .len();
        let spent = |method: &str, sender: Principal, args: Vec<u8>| -> (u128, Vec<u8>) {
            let cycles = ic.cycle_balance(payment);
            let reply = ic.update_call(payment, sender, method, args).unwrap();
            (cycles - ic.cycle_balance(payment), reply)
        };
        let undecided = escrows / 2 / 1_000 * 1_000;
        let id = escrow_id(undecided);
        let (expiry_cycles, reply) = spent(
            "expiry_refund",
            person(1),
            candid::encode_args((id,)).unwrap(),
        );
        let expired: Result<EscrowInfo> = candid::decode_one(&reply).unwrap();
        assert_eq!(expired.unwrap().decision, FundsDecision::RefundCommitted);
        let (claim_cycles, reply) = spent(
            "claim_refund",
            payer(undecided),
            candid::encode_args((id, vec![undecided * 4], true)).unwrap(),
        );
        let leg: Result<TransferLeg> = candid::decode_one(&reply).unwrap();
        let leg = leg.unwrap();
        let pending: Vec<TransferLeg> = query(
            &ic,
            payment,
            person(1),
            "list_pending_transfers",
            (None::<(u64, Hash, u64)>,),
        );
        assert_eq!(pending, [leg]);
        let s: PaymentStats = query(&ic, payment, person(1), "payment_stats", ());
        assert_eq!(
            (s.escrows, s.open_escrows, s.pending_transfers),
            (escrows, escrows / 1_000 - 1, 1)
        );
        let status = ic.canister_status(payment, None).unwrap();
        println!("{upgrade}");
        println!(
            "payment_capacity escrows={escrows} image_bytes={image_bytes} upgrade_cycles={upgrade_cycles} expiry_refund_cycles={expiry_cycles} claim_refund_cycles={claim_cycles} batch32_bytes={batch_bytes} host_query_ms={} heap_bytes={} stable_bytes={}",
            elapsed.as_millis(),
            status.memory_metrics.wasm_memory_size,
            status.memory_metrics.stable_memory_size,
        );
    }
}
