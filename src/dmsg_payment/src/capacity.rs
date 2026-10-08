//! Host-built capacity images: the canister's own stable records and
//! certification tree, which `payment_capacity_profile` loads into PocketIC.
use crate::{model, store::*};
use candid::Principal;
use dmsg_protocol::digest;
use dmsg_runtime::{ledger::VerifiedTransfer, storage::MapExt};
use dmsg_types::{
    payment::*,
    profiles::delivery::{OpenEscrow, Quote},
    *,
};
use icrc_ledger_types::icrc1::account::Account;

/// Quotes are created a day before this time, so every `accept_by` has passed
/// when the profile starts PocketIC's clock here.
pub const AT: u64 = 1_800_000_000_000;
const SIZES: [u64; 2] = [100_000, 1_000_000];
/// Every STRIDE-th escrow stays undecided with a late, refundable deposit.
const STRIDE: u64 = 1_000;
const FEE: u128 = 10_000;

fn p(n: u8) -> Principal {
    Principal::from_slice(&[n, 1])
}

fn account(owner: Principal) -> Account {
    Account {
        owner,
        subaccount: None,
    }
}

/// The payer of escrow `i`, matching the profile.
fn payer(i: u64) -> Principal {
    Principal::self_authenticating(i.to_be_bytes())
}

fn input(i: u64) -> OpenEscrow {
    let created_at = AT - DAY;
    let quote = Quote {
        quote_id: digest("capacity quote", &i),
        home_payment: p(5),
        payer: account(payer(i)),
        offer_digest: Hash::new([2; 32]),
        quote_scope: Hash::new([3; 32]),
        ledger: p(6),
        recipient: account(p(7)),
        recipient_net: 1_000_000,
        platform: account(p(8)),
        fee_policy_version: 1,
        service_fee: 50_000,
        fee_reserve: 60_000,
        amount: 1_110_000,
        max_network_fee: 20_000,
        max_bytes: 8_192,
        retain_ms: 30 * DAY,
        envelope_digest: digest("capacity envelope", &i),
        signer_epoch: 1,
        created_at,
        fund_by: created_at + 15 * MINUTE,
        accept_by: created_at + 45 * MINUTE,
    };
    OpenEscrow {
        op_id: digest("capacity operation", &i),
        offer: SignedOffer {
            offer: PaymentOffer {
                account_id: AccountId([1; 12]),
                device_id: Hash::new([4; 32]),
                security_epoch: 1,
                home_payment: quote.home_payment,
                offer_id: Hash::new([5; 32]),
                ledger: quote.ledger,
                recipient: quote.recipient,
                recipient_net: quote.recipient_net,
                quote_scope: quote.quote_scope,
                version: 1,
                issued_at: created_at,
                expires_at: quote.accept_by,
            },
            signature: Default::default(),
        },
        quote,
        quote_signature: Default::default(),
    }
}

/// Deposit block of escrow `i`; outgoing blocks follow it.
pub fn block(i: u64) -> u64 {
    i * 4
}

fn deposit(e: &crate::state::Escrow, i: u64, late: bool) -> VerifiedTransfer {
    VerifiedTransfer {
        block: block(i),
        from: e.quote.payer,
        to: Account {
            owner: p(5),
            subaccount: Some(e.subaccount.into_array()),
        },
        amount: e.quote.amount,
        fee: Some(FEE),
        committed_at: if late {
            e.quote.fund_by
        } else {
            e.quote.fund_by - MINUTE
        },
        memo: None,
        created_at_time: None,
        spender: None,
    }
}

fn complete(e: &mut crate::state::Escrow, mut leg: TransferLeg, block: u64) {
    model::complete_leg(e, &mut leg, block).expect("payout");
    OUTGOING.with_borrow_mut(|t| t.put(&block.to_be_bytes(), &(e.escrow_id, leg.leg_id)));
    put_leg(&leg);
}

/// Escrow `i` through `open_escrow`, `check_funding`, `finalize_receipt`, two
/// payouts and the reserve refund, as the 2026-10-02 scale profile built them
/// through the canister; every STRIDE-th escrow instead keeps a late deposit
/// and waits for its expiry refund.
fn escrow(i: u64) {
    let input = input(i);
    let payer = payer(i);
    let id = digest("dmsg/escrow-id/v1", &(p(5), payer, input.op_id));
    let mut e = model::escrow(
        p(5),
        id,
        payer,
        &input,
        digest("dmsg/quote/v2", &input.quote),
    );
    QUOTES.with_borrow_mut(|t| t.put(input.quote.quote_id.as_slice(), &()));
    index_payer(&e);
    let undecided = i.is_multiple_of(STRIDE);
    let tx = deposit(&e, i, undecided);
    let d = model::accept_deposit(&mut e, p(5), &tx).expect("deposit");
    FUNDING.with_borrow_mut(|t| t.put(&tx.block.to_be_bytes(), &id));
    DEPOSITS.with_borrow_mut(|t| t.put(&key(id, tx.block), &d));
    if !undecided {
        let at = e.quote.fund_by;
        model::settle(&mut e, digest("capacity receipt", &i));
        for (n, leg) in model::prepare_settlement(&mut e, FEE, at)
            .expect("settlement")
            .into_iter()
            .enumerate()
        {
            complete(&mut e, leg, tx.block + 1 + n as u64);
        }
        let leg = model::claim_refund(&mut e, &mut [], true, FEE, payer, at).expect("reserve");
        complete(&mut e, leg, tx.block + 3);
        release_payer(&e);
    }
    save(&e);
}

/// Writes `payment-<escrows>.bin` images of growing size to DMSG_PAYMENT_IMAGE_DIR.
#[test]
#[ignore = "host-built stable images for payment_capacity_profile"]
fn capacity_image() {
    let dir =
        std::path::PathBuf::from(std::env::var_os("DMSG_PAYMENT_IMAGE_DIR").expect("image dir"));
    let signer = ReceiptSigner {
        epoch: 1,
        public_key: Hash::new([9; 32]),
        valid_from: 0,
        valid_until: AT + 365 * DAY,
        revoked: false,
    };
    let fee_policy = DeliveryFeePolicy {
        version: 1,
        effective_at_ms: 0,
        rate_bps: 500,
        minimum_atomic: 50_000,
    };
    SIGNERS.with_borrow_mut(|t| t.put(&signer.epoch.to_be_bytes(), &signer));
    certify_signer(&signer);
    FEE_POLICIES.with_borrow_mut(|t| t.put(&fee_policy.version.to_be_bytes(), &fee_policy));
    certify_fee_policy(&fee_policy);
    save_cfg(Config::new(PaymentInit {
        environment: Environment::Local,
        issuer_namespace: "https://dmsg.test/u/".into(),
        user_homes: vec![p(4)],
        ledger: p(6),
        platform: account(p(8)),
        governance: p(9),
        fee_policy,
        ledger_fee: FEE,
        max_fee: 20_000,
        signer,
        limits: PaymentLimits {
            max_escrows: MAX_PAYMENT_ESCROWS,
            daily_orders: MAX_DAILY_ORDERS,
            max_open_per_payer: MAX_OPEN_PER_PAYER,
            authorizations_per_minute: MAX_CALLS_PER_MINUTE,
            ledger_reads_per_minute: MAX_CALLS_PER_MINUTE,
            ledger_writes_per_minute: MAX_CALLS_PER_MINUTE,
            ledger_calls_per_caller: MAX_CALLS_PER_MINUTE,
        },
        enabled: true,
    }));
    with_cfg(certify_config);
    let mut escrows = 0;
    for target in SIZES {
        let started = std::time::Instant::now();
        while escrows < target {
            escrow(escrows);
            escrows += 1;
        }
        RAW.with(|m| {
            let bytes = m.borrow();
            std::fs::write(dir.join(format!("payment-{escrows}.bin")), &*bytes).expect("image");
            println!(
                "payment_image escrows={escrows} stable_bytes={} host_build_s={}",
                bytes.len(),
                started.elapsed().as_secs()
            );
        });
    }
}
