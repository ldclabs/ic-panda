//! Moving a native Taproot address to this device's key. The first leaf of
//! such an address commits to one device's key, so a new device gets an
//! address of its own and the coins move: register this device's key
//! (`AddCosigner`), add a Taproot address for it with the old address's
//! chain-key delay and recovery (`AddAddress`), put the new address in the
//! address book first when the amount's tier is book-only, then sweep the
//! old address in one transfer without change: through L1 while the old
//! device still signs, or through L2 once each output is old enough for the
//! chain key alone. Each step is a proposal the team approves as any other.
//! Once the old address holds nothing, this device marks it archived.
use super::*;

const MIGRATION: &str = "wallet-nc-migrate:";
const SWEEP: &str = "wallet-sweep:";
const ARCHIVED: &str = "wallet-archived:";
/// The smallest output the wallet builds, as Bitcoin relays it.
const DUST: u64 = 546;

/// A migration's proposals, kept on this device.
#[derive(Serialize, Deserialize, Clone, Default)]
struct Migration {
    register: Option<u64>,
    address: Option<u64>,
    book: Option<u64>,
    sweep: Option<u64>,
    /// Proposals made again after one ended without effect.
    attempt: u32,
}

/// How a sweep's plan is built, kept until its proposal executes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(super) struct Sweep {
    pub(super) utxos: Vec<Utxo>,
    pub(super) fee_rate: u64,
    pub(super) timelock: bool,
}

impl Client {
    fn migration(&self, wallet: Principal, from: u32) -> Result<Migration> {
        Ok(self
            .workspace
            .private(self.keys()?, &format!("{MIGRATION}{wallet}:{from}"))?
            .unwrap_or_default())
    }

    fn save_migration(&self, wallet: Principal, from: u32, m: &Migration) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{MIGRATION}{wallet}:{from}"), m)
    }

    /// The inputs and fee rate a sweep proposal's amount was set for.
    pub(super) fn sweep_params(&self, wallet: Principal, proposal: u64) -> Result<Option<Sweep>> {
        self.workspace
            .private(self.keys()?, &format!("{SWEEP}{wallet}:{proposal}"))
    }

    /// The addresses of `wallet` this device archived.
    pub(super) fn archived(&self, wallet: Principal) -> Result<Vec<u32>> {
        Ok(self
            .workspace
            .private(self.keys()?, &format!("{ARCHIVED}{wallet}"))?
            .unwrap_or_default())
    }
}

/// Virtual size of a transaction with `inputs` alike, each with a witness
/// of `witness` bytes, paying outputs with these script lengths; as the
/// wallet counts it.
fn vsize(inputs: usize, outputs: &[usize], witness: usize) -> u64 {
    let base = 4
        + compact_size(inputs).len()
        + inputs * 41
        + compact_size(outputs.len()).len()
        + outputs
            .iter()
            .map(|n| 8 + compact_size(*n).len() + n)
            .sum::<usize>()
        + 4;
    ((base * 4 + 2 + inputs * witness) as u64).div_ceil(4)
}

/// Witness bytes of one input of a native address through L1 (both keys)
/// or L2 (the chain key after the delay).
fn witness(
    view: &WalletView,
    a: &Address,
    control: &TaprootControl,
    timelock: bool,
) -> Result<usize> {
    let cosigner: [u8; 32] = view
        .cosigners
        .iter()
        .find(|c| c.id == control.cosigner)
        .and_then(|c| c.input.public_key.as_slice().try_into().ok())
        .ok_or(Error::NotFound)?;
    let tree = taproot_tree(&x_only(&a.public_key)?, &cosigner, control);
    let leaf = usize::from(timelock);
    let (_, odd) = taproot_output_key(&NUMS_KEY, Some(&tree.root))?;
    let script = &tree.leaves[leaf];
    let control_block = taproot_control_block(&tree, leaf, odd);
    Ok(1 + 65
        + usize::from(!timelock) * 65
        + compact_size(script.len()).len()
        + script.len()
        + 1
        + control_block.len())
}

/// The fee and amount that sweep `total` through `inputs` inputs into one
/// output, leaving no change; none when what is left would be dust.
fn sweep_amount(
    total: u64,
    inputs: usize,
    output: usize,
    witness: usize,
    fee_rate: u64,
) -> Option<(u64, u64)> {
    let fee = fee_rate.checked_mul(vsize(inputs, &[output], witness))?;
    let amount = total.checked_sub(fee)?;
    (amount >= DUST).then_some((fee, amount))
}

/// Whether a BTC spend of `amount` needs its recipient in the address
/// book; without an amount, whether the top tier does.
fn book_required(view: &WalletView, amount: Option<u128>) -> bool {
    view.policy.spendable_assets.contains(&AssetKey::Btc)
        && matches!(
            spend_requirement(
                &view.policy,
                &AssetKey::Btc,
                Voters::of(&view.members),
                amount.unwrap_or(u128::MAX),
                false,
            ),
            Err(Error::Forbidden)
        )
}

/// The book entry of `destination`, if any.
async fn book_entry<H: Hold>(
    h: &H,
    wallet: Principal,
    destination: &Destination,
) -> Result<Option<BookEntry>> {
    let (_, ic) = identity(h).await?;
    let mut after: Option<Destination> = None;
    loop {
        let page: Result<Vec<BookEntry>> =
            ic.query(wallet, "address_book", (after, 200u16)).await?;
        let page = page?;
        if let Some(entry) = page.iter().find(|e| e.destination == *destination) {
            return Ok(Some(entry.clone()));
        }
        if page.len() < 200 {
            return Ok(None);
        }
        after = page.last().map(|e| e.destination.clone());
    }
}

/// A stored step proposal that still waits to take effect.
async fn waiting<H: Hold>(h: &H, wallet: Principal, id: Option<u64>) -> Result<Option<Proposal>> {
    let Some(id) = id else { return Ok(None) };
    let p = proposal_raw(h, wallet, id).await?;
    Ok(matches!(
        p.status,
        ProposalStatus::Open
            | ProposalStatus::Delayed
            | ProposalStatus::Ready
            | ProposalStatus::Executing
            | ProposalStatus::Signed
            | ProposalStatus::Broadcast
    )
    .then_some(p))
}

/// The coins of the old address a sweep through `timelock` spends now: all
/// of them through L1, through L2 those `chain_key_after` blocks deep; at
/// most one plan's inputs, the largest first.
struct Coins {
    tip: u32,
    fee_rate: u64,
    listed: Vec<Json>,
    spendable: Vec<Utxo>,
}

async fn coins<H: Hold>(
    h: &H,
    old: &Address,
    control: &TaprootControl,
    timelock: bool,
) -> Result<Coins> {
    let chains = h.with(|c| c.chains()).await?;
    let (listed, tip, fee_rate) = futures_util::try_join!(
        chains.btc_utxos(&old.text),
        chains.btc_tip_height(),
        chains.btc_fee_rate()
    )?;
    let after = u32::from(control.chain_key_after);
    let mut spendable = vec![];
    let mut shown = vec![];
    for u in listed {
        let mut txid: [u8; 32] = hex::decode(&u.txid)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(Error::IntegrityFailed)?;
        let height = u.status.block_height.filter(|_| u.status.confirmed);
        // Mined at height h, an output spends through L2 in block h + delay.
        let spendable_at = height.map(|h| h.saturating_add(after));
        shown.push(json!({
            "txid": u.txid, "vout": u.vout, "value": u.value.to_string(),
            "height": height, "spendable_at": spendable_at,
        }));
        if !timelock || spendable_at.is_some_and(|at| at <= tip.saturating_add(1)) {
            txid.reverse();
            spendable.push(Utxo {
                txid: Hash::new(txid),
                vout: u.vout,
                value: u.value,
                address: old.id,
            });
        }
    }
    spendable.sort_by_key(|u| std::cmp::Reverse(u.value));
    spendable.truncate(MAX_BTC_INPUTS);
    Ok(Coins {
        tip,
        fee_rate,
        listed: shown,
        spendable,
    })
}

/// Where a migration stands.
struct Situation {
    view: WalletView,
    old: Address,
    control: TaprootControl,
    root: bool,
    key: Option<serde_bytes::ByteBuf>,
    cosigner: Option<u32>,
    to: Option<Address>,
    migration: Migration,
    archived: bool,
}

async fn situation<H: Hold>(h: &H, wallet: Principal, from: u32) -> Result<Situation> {
    let view = state(h, wallet).await?;
    let old = view
        .addresses
        .iter()
        .find(|a| a.id == from)
        .cloned()
        .ok_or(Error::NotFound)?;
    let AddressKind::Taproot(control) = old.kind.clone() else {
        return Err(dmsg_types::invalid("not a native taproot address"));
    };
    let ((root, key), migration, archived) = h
        .with(|c| {
            Ok((
                c.native_public_key(wallet)?,
                c.migration(wallet, from)?,
                c.archived(wallet)?.contains(&from),
            ))
        })
        .await?;
    let cosigner = view
        .cosigners
        .iter()
        .find(|c| c.input.kind == CosignerKind::Bip340 && Some(&c.input.public_key) == key.as_ref())
        .map(|c| c.id);
    let to = view
        .addresses
        .iter()
        .filter(|a| {
            a.id != from
                && matches!(&a.kind, AddressKind::Taproot(c) if Some(c.cosigner) == cosigner
                    && c.chain_key_after == control.chain_key_after
                    && c.recovery == control.recovery)
        })
        .max_by_key(|a| a.id)
        .cloned();
    Ok(Situation {
        view,
        old,
        control,
        root,
        key,
        cosigner,
        to,
        migration,
        archived,
    })
}

/// The next step and, while it waits on the team, its proposal.
async fn next<H: Hold>(
    h: &H,
    s: &Situation,
    amount: Option<u128>,
) -> Result<(&'static str, Option<Proposal>, Option<u64>)> {
    let wallet = s.view.wallet;
    if s.archived {
        return Ok(("done", None, None));
    }
    if !s.root {
        return Ok(("root", None, None));
    }
    if s.cosigner.is_none() {
        return Ok(match waiting(h, wallet, s.migration.register).await? {
            Some(p) => ("wait", Some(p), None),
            None => ("register", None, None),
        });
    }
    let Some(to) = &s.to else {
        return Ok(match waiting(h, wallet, s.migration.address).await? {
            Some(p) => ("wait", Some(p), None),
            None => ("address", None, None),
        });
    };
    if book_required(&s.view, amount) {
        match book_entry(h, wallet, &Destination::Bitcoin(to.text.clone())).await? {
            None => {
                return Ok(match waiting(h, wallet, s.migration.book).await? {
                    Some(p) => ("wait", Some(p), None),
                    None => ("book", None, None),
                });
            }
            Some(entry) => {
                let until = entry.added_at_ms + s.view.policy.address_book_cooldown_ms;
                if now_ms() < until {
                    return Ok(("cooldown", None, Some(until)));
                }
            }
        }
    }
    Ok(match waiting(h, wallet, s.migration.sweep).await? {
        Some(p) => ("wait", Some(p), None),
        None => ("sweep", None, None),
    })
}

fn view_of(s: &Situation, step: &str, pending: Option<&Proposal>, until: Option<u64>) -> Json {
    json!({
        "wallet": s.view.wallet.to_text(),
        "from": {
            "id": s.old.id, "text": s.old.text, "label": s.old.label,
            "cosigner": s.control.cosigner,
            "mine": Some(s.control.cosigner) == s.cosigner,
            "chain_key_after": s.control.chain_key_after,
            "recovery_after": s.control.recovery.as_ref().map(|r| r.after),
        },
        "key": s.key.as_ref().map(hex::encode),
        "cosigner": s.cosigner,
        "to": s.to.as_ref().map(|a| json!({"id": a.id, "text": a.text, "label": a.label})),
        "next": step,
        "pending": pending.map(|p| json!({"proposal": p.id, "status": format!("{:?}", p.status)})),
        "cooldown_until": until,
        "archived": s.archived,
    })
}

/// This account's native addresses of `wallet` whose key is another
/// device's and which this device has not archived: what a successor moves.
///
/// # Errors
/// As [`state`].
pub async fn to_move<H: Hold>(h: &H, wallet: Principal) -> Result<Vec<u32>> {
    let view = state(h, wallet).await?;
    h.with(|c| {
        let me = c.wallet_member()?;
        let held = c.held_cosigners(&view)?;
        let archived = c.archived(wallet)?;
        Ok(view
            .addresses
            .iter()
            .filter(|a| match &a.kind {
                AddressKind::Taproot(control) => {
                    !held.contains(&control.cosigner)
                        && !archived.contains(&a.id)
                        && view
                            .cosigners
                            .iter()
                            .any(|k| k.id == control.cosigner && k.input.member == me)
                }
                _ => false,
            })
            .map(|a| a.id)
            .collect())
    })
    .await
}

/// Where moving native address `from` to this device stands and what comes
/// next; with `timelock`, also the sweep through that path as it would be
/// proposed now, read from the chain.
///
/// # Errors
/// Returns `Error::NotFound` for an unknown address and `InvalidInput` for
/// one that is not a native Taproot address.
pub async fn status<H: Hold>(
    h: &H,
    wallet: Principal,
    from: u32,
    timelock: Option<bool>,
) -> Result<Json> {
    let s = situation(h, wallet, from).await?;
    let Some(timelock) = timelock else {
        let (step, pending, until) = next(h, &s, None).await?;
        return Ok(view_of(&s, step, pending.as_ref(), until));
    };
    let coins = coins(h, &s.old, &s.control, timelock).await?;
    let total: u64 = coins.spendable.iter().map(|u| u.value).sum();
    let swept = match &s.to {
        Some(to) => sweep_amount(
            total,
            coins.spendable.len(),
            btc_script(s.view.btc_network, &to.text)?.len(),
            witness(&s.view, &s.old, &s.control, timelock)?,
            coins.fee_rate,
        ),
        None => None,
    };
    let (step, pending, until) = next(h, &s, swept.map(|(_, amount)| u128::from(amount))).await?;
    // An address with nothing left only needs archiving.
    let step = match step {
        "sweep" if coins.listed.is_empty() => "archive",
        step => step,
    };
    let mut out = view_of(&s, step, pending.as_ref(), until);
    out["sweep"] = json!({
        "path": if timelock { "l2" } else { "l1" },
        "tip": coins.tip,
        "fee_rate": coins.fee_rate,
        "utxos": coins.listed,
        "inputs": coins.spendable.len(),
        "total": total.to_string(),
        "fee": swept.map(|(fee, _)| fee.to_string()),
        "amount": swept.map(|(_, amount)| amount.to_string()),
        "dust": s.to.is_some() && swept.is_none() && total > 0,
        "empty": coins.listed.is_empty(),
    });
    Ok(out)
}

/// Take the next step of moving native address `from` to this device:
/// propose what comes next, or archive the old address once it holds
/// nothing. A step that waits on the team or the cooldown proposes nothing.
/// Registering this device's key the first time asks for the person.
///
/// # Errors
/// As [`status`], `Error::NotFound` without this device's root,
/// `Error::Pending` while a step waits and `Error::FeeBlocked` when nothing
/// above dust can move through `timelock` yet.
pub async fn start<H: Hold>(h: &H, wallet: Principal, from: u32, timelock: bool) -> Result<Json> {
    let s = situation(h, wallet, from).await?;
    let mut migration = s.migration.clone();
    let (step, _, _) = next(h, &s, None).await?;
    let proposed = |p: &Json| p["id"].as_u64().ok_or(Error::IntegrityFailed);
    match step {
        "done" => return status(h, wallet, from, Some(timelock)).await,
        "root" => return Err(Error::NotFound),
        "wait" | "cooldown" => return Err(Error::Pending),
        "register" => {
            let device = h.with(|c| c.device_id()).await?;
            let label = format!("dMsg {}", &hex::encode(*device)[..8]);
            let made = native::key(h, wallet, &label).await?;
            let intent: Intent = serde_json::from_value(made["intent"].clone())
                .map_err(|e| Error::InvalidInput(e.to_string()))?;
            migration.attempt += u32::from(migration.register.is_some());
            let id = op_id(wallet, from, "register", migration.attempt, &());
            let p = ops::propose_with_id(h, wallet, intent, None, id).await?;
            migration.register = Some(proposed(&p)?);
        }
        "address" => {
            migration.attempt += u32::from(migration.address.is_some());
            let intent = Intent::AddAddress {
                chain: Chain::Bitcoin,
                kind: AddressKind::Taproot(TaprootControl {
                    cosigner: s.cosigner.ok_or(Error::NotFound)?,
                    chain_key_after: s.control.chain_key_after,
                    recovery: s.control.recovery.clone(),
                }),
                label: s.old.label.clone(),
                cosign: None,
            };
            let id = op_id(wallet, from, "address", migration.attempt, &());
            let p = ops::propose_with_id(h, wallet, intent, None, id).await?;
            migration.address = Some(proposed(&p)?);
        }
        "book" => {
            migration.attempt += u32::from(migration.book.is_some());
            let to = s.to.as_ref().ok_or(Error::NotFound)?;
            let intent = Intent::AddressBook(BookChange::Add {
                destination: Destination::Bitcoin(to.text.clone()),
                label: to.label.clone(),
            });
            let id = op_id(wallet, from, "book", migration.attempt, &());
            let p = ops::propose_with_id(h, wallet, intent, None, id).await?;
            migration.book = Some(proposed(&p)?);
        }
        _ => {
            let to = s.to.as_ref().ok_or(Error::NotFound)?;
            let coins = coins(h, &s.old, &s.control, timelock).await?;
            if coins.listed.is_empty() {
                // Nothing left: the old address is done on this device.
                h.with(|c| {
                    let mut archived = c.archived(wallet)?;
                    if !archived.contains(&from) {
                        archived.push(from);
                    }
                    c.workspace
                        .put_private(c.keys()?, &format!("{ARCHIVED}{wallet}"), &archived)
                })
                .await?;
                return status(h, wallet, from, Some(timelock)).await;
            }
            let total: u64 = coins.spendable.iter().map(|u| u.value).sum();
            let (fee, amount) = sweep_amount(
                total,
                coins.spendable.len(),
                btc_script(s.view.btc_network, &to.text)?.len(),
                witness(&s.view, &s.old, &s.control, timelock)?,
                coins.fee_rate,
            )
            .ok_or(Error::FeeBlocked)?;
            migration.attempt += u32::from(migration.sweep.is_some());
            let intent = Intent::Transfer {
                from,
                asset: AssetKey::Btc,
                to: Destination::Bitcoin(to.text.clone()),
                amount: u128::from(amount),
                max_fee: u128::from(fee),
                memo: None,
            };
            let sweep = Sweep {
                utxos: coins.spendable,
                fee_rate: coins.fee_rate,
                timelock,
            };
            // Another set of coins or rate is another proposal.
            let id = op_id(wallet, from, "sweep", migration.attempt, &sweep);
            let p = ops::propose_with_id(h, wallet, intent, None, id).await?;
            let proposal = proposed(&p)?;
            h.with(|c| {
                c.workspace
                    .put_private(c.keys()?, &format!("{SWEEP}{wallet}:{proposal}"), &sweep)
            })
            .await?;
            migration.sweep = Some(proposal);
        }
    }
    h.with(|c| c.save_migration(wallet, from, &migration))
        .await?;
    status(h, wallet, from, Some(timelock)).await
}

/// The operation ID of a migration step: the same while it is retried, a
/// new one once a proposal of the step ended without effect.
fn op_id<T: Serialize>(wallet: Principal, from: u32, step: &str, attempt: u32, detail: &T) -> Hash {
    dmsg_protocol::digest(
        "dmsg-wallet/nc-migrate/v1",
        &(wallet, from, step, attempt, detail),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vectors() -> (WalletView, Vec<(String, Intent, ExecutionPlan)>) {
        let vectors: Json = serde_json::from_str(include_str!(
            "../../../../tests/vectors/wallet/plans-v1.json"
        ))
        .unwrap();
        let bytes = |v: &Json| hex::decode(v.as_str().unwrap()).unwrap();
        let view = dmsg_protocol::decode_canonical(&bytes(&vectors["view_cbor_hex"])).unwrap();
        let plans = vectors["plans"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["name"].as_str().unwrap().to_string(),
                    dmsg_protocol::decode_canonical(&bytes(&c["intent_cbor_hex"])).unwrap(),
                    dmsg_protocol::decode_canonical(&bytes(&c["plan_cbor_hex"])).unwrap(),
                )
            })
            .collect();
        (view, plans)
    }

    /// The fee the wallet charged for both paths of a native address is
    /// the rate times the size counted here.
    #[test]
    fn sizes_match_the_wallet_plans_of_both_paths() {
        let (view, plans) = vectors();
        let mut checked = 0;
        for (name, intent, plan) in plans {
            let ExecutionParams::Bitcoin {
                utxos,
                fee_rate,
                timelock,
                ..
            } = &plan.params
            else {
                continue;
            };
            let (from, _) = external_source(&intent).unwrap();
            let a = view.addresses.iter().find(|a| a.id == from).unwrap();
            let AddressKind::Taproot(control) = &a.kind else {
                continue;
            };
            let tx = decode::parse_btc(&plan.unsigned).unwrap();
            let outputs: Vec<usize> = tx.outputs.iter().map(|(_, s)| s.len()).collect();
            let size = vsize(
                utxos.len(),
                &outputs,
                witness(&view, a, control, *timelock).unwrap(),
            );
            assert_eq!(u128::from(fee_rate * size), plan.summary.fee, "{name}");
            checked += 1;
        }
        assert_eq!(checked, 2);
    }

    #[test]
    fn a_sweep_leaves_no_change_and_never_dust() {
        let (view, _) = vectors();
        let a = view
            .addresses
            .iter()
            .find(|a| matches!(a.kind, AddressKind::Taproot(_)))
            .unwrap();
        let AddressKind::Taproot(control) = &a.kind else {
            unreachable!()
        };
        let w = witness(&view, a, control, false).unwrap();
        let fee = 3 * vsize(2, &[34], w);
        assert_eq!(sweep_amount(fee + 50_000, 2, 34, w, 3), Some((fee, 50_000)));
        assert_eq!(sweep_amount(fee + DUST, 2, 34, w, 3), Some((fee, DUST)));
        assert_eq!(sweep_amount(fee + DUST - 1, 2, 34, w, 3), None);
        assert_eq!(sweep_amount(fee - 1, 2, 34, w, 3), None);
        // What is left is exactly the fee, which no change output can share:
        // the wallet adds change only when some remains after its own size.
        let with_change = 3 * vsize(2, &[34, 34], w);
        assert!(with_change > fee);
        // L2 needs one signature, so it costs less than L1.
        assert!(vsize(2, &[34], witness(&view, a, control, true).unwrap()) < vsize(2, &[34], w));
    }

    #[test]
    fn the_book_comes_first_only_for_a_book_only_tier() {
        let (mut view, _) = vectors();
        view.policy.spendable_assets = vec![AssetKey::Btc];
        view.policy.tiers = vec![
            Tier {
                asset: AssetKey::Btc,
                up_to: 100_000,
                approvals: 1,
                delay_ms: 0,
                book_only: false,
            },
            Tier {
                asset: AssetKey::Btc,
                up_to: 1_000_000,
                approvals: 1,
                delay_ms: 0,
                book_only: true,
            },
        ];
        assert!(!book_required(&view, Some(50_000)));
        assert!(book_required(&view, Some(500_000)));
        // Above every tier the top one applies; without an amount, so does it.
        assert!(book_required(&view, Some(5_000_000)));
        assert!(book_required(&view, None));
        view.policy.spendable_assets.clear();
        assert!(!book_required(&view, Some(500_000)));
    }
}
