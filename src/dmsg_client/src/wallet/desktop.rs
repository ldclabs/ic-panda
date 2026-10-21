//! Structured wallet forms, asset metadata and local chain connection settings.
//! Renderer inputs never become arbitrary Candid or arbitrary canister calls.
use super::*;
use crate::config::WalletSettings;
use base64::{Engine, engine::general_purpose::STANDARD};
use dmsg_types::ensure_valid;
use sha2::{Digest, Sha224};

/// A stable key for an asset in forms and cached metadata.
pub fn asset_key(asset: &AssetKey) -> String {
    match asset {
        AssetKey::Icp => "icp".into(),
        AssetKey::Icrc(p) => format!("icrc:{p}"),
        AssetKey::Btc => "btc".into(),
        AssetKey::Evm { chain_id, token } => match token {
            Some(t) => format!("evm:{chain_id}:{}", eip55(&t.into_array())),
            None => format!("evm:{chain_id}"),
        },
        AssetKey::Sol { mint } => match mint {
            Some(m) => format!("sol:{}", sol_address(&m.into_array())),
            None => "sol".into(),
        },
    }
}

/// Convert a decimal input without floats, rounding or accepting exponent notation.
pub fn atomic(text: &str, decimals: u8) -> Result<u128> {
    ensure_valid(decimals <= 38, "asset decimals")?;
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    ensure_valid(
        !whole.is_empty()
            && whole.bytes().all(|b| b.is_ascii_digit())
            && fraction.bytes().all(|b| b.is_ascii_digit())
            && fraction.len() <= usize::from(decimals)
            && text.len() <= 80
            && !text.ends_with('.'),
        "amount",
    )?;
    let digits = format!(
        "{whole}{fraction}{}",
        "0".repeat(usize::from(decimals) - fraction.len())
    );
    digits
        .parse()
        .map_err(|_| dmsg_types::invalid("amount overflow"))
}

/// RPC URLs are local, account-scoped preferences; they never change the IC trust root.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcSettings {
    /// Esplora endpoint.
    pub bitcoin_api: String,
    /// EVM chain and JSON-RPC endpoint pairs.
    pub evm_rpc: Vec<(u64, String)>,
    /// Solana endpoint.
    pub solana_rpc: String,
}

pub(super) fn settings(c: &Client) -> Result<WalletSettings> {
    let mut s = c.config.wallet.clone();
    if let Some(saved) = c
        .workspace
        .private::<RpcSettings>(c.keys()?, "wallet-rpc")?
    {
        s.bitcoin_api = saved.bitcoin_api;
        s.evm_rpc = saved.evm_rpc;
        s.solana_rpc = saved.solana_rpc;
    }
    Ok(s)
}

/// Read or replace only this account's chain endpoints.
pub fn rpc_settings(c: &mut Client, input: Option<RpcSettings>) -> Result<Json> {
    if let Some(input) = input {
        ensure_valid(input.evm_rpc.len() <= 32, "EVM endpoints")?;
        let mut ids = std::collections::BTreeSet::new();
        ensure_valid(
            input
                .evm_rpc
                .iter()
                .all(|(id, _)| *id > 0 && ids.insert(*id)),
            "EVM chain IDs",
        )?;
        let mut config = c.config.clone();
        config.wallet.bitcoin_api = input.bitcoin_api.clone();
        config.wallet.evm_rpc = input.evm_rpc.clone();
        config.wallet.solana_rpc = input.solana_rpc.clone();
        config.validate()?;
        c.workspace.put_private(c.keys()?, "wallet-rpc", &input)?;
        c.chains = None;
    }
    let s = settings(c)?;
    Ok(json!({"bitcoin_api": s.bitcoin_api, "evm_rpc": s.evm_rpc, "solana_rpc": s.solana_rpc}))
}

/// Check the configured endpoints without moving funds.
pub async fn rpc_check<H: Hold>(h: &H) -> Result<Json> {
    let (s, chains) = h.with(|c| Ok((settings(c)?, c.chains()?))).await?;
    let btc = async {
        json!({"chain":"Bitcoin", "source":s.bitcoin_api, "result":shown(chains.btc_fee_rate().await)})
    };
    let sol = async {
        json!({"chain":"Solana", "source":s.solana_rpc, "result":shown(chains.sol("getSlot", json!([])).await.map(|v| v.to_string()))})
    };
    let chains = &chains;
    let evm = join_all(s.evm_rpc.iter().map(|(id, url)| async move {
        let read = chains
            .evm(*id, "eth_chainId", json!([]))
            .await
            .and_then(|v| {
                let found =
                    u64::from_str_radix(v.as_str().unwrap_or("").trim_start_matches("0x"), 16)
                        .map_err(|_| Error::IntegrityFailed)?;
                ensure(found == *id, Error::IntegrityFailed)?;
                Ok(found)
            });
        json!({"chain":format!("EVM {id}"), "source":url, "result":shown(read)})
    }));
    let ((btc, sol), mut evm) = join(join(btc, sol), evm).await;
    evm.insert(0, btc);
    evm.push(sol);
    Ok(json!({"read_at":now_ms(), "endpoints":evm}))
}

/// Metadata of one asset. Failed metadata stays unknown, never guessed.
pub async fn metadata<H: Hold>(h: &H, asset: &AssetKey, refresh: bool) -> Result<Json> {
    let key = asset_key(asset);
    let cache = format!("wallet-asset:{key}");
    if !refresh
        && let Some(v) = h
            .with(|c| c.workspace.private::<Json>(c.keys()?, &cache))
            .await?
        && v["read_at"]
            .as_u64()
            .is_some_and(|at| now_ms().saturating_sub(at) < dmsg_types::DAY)
    {
        return Ok(v);
    }
    let (s, chains) = h.with(|c| Ok((settings(c)?, c.chains()?))).await?;
    let reader = h.anonymous().await?;
    let mut out = asset_view(asset);
    out["key"] = json!(key);
    let read: Result<(String, u8, String)> = match asset {
        AssetKey::Icp => Ok(("ICP".into(), 8, "ICP ledger".into())),
        AssetKey::Btc => Ok(("BTC".into(), 8, s.bitcoin_api)),
        AssetKey::Evm {
            chain_id,
            token: None,
        } => Ok((
            format!("Native · {chain_id}"),
            18,
            s.evm_rpc
                .iter()
                .find(|(id, _)| id == chain_id)
                .map(|(_, s)| s.clone())
                .unwrap_or_default(),
        )),
        AssetKey::Sol { mint: None } => Ok(("SOL".into(), 9, s.solana_rpc)),
        AssetKey::Icrc(ledger) => {
            let (symbol, decimals) = join(
                reader.query::<_, String>(*ledger, "icrc1_symbol", ()),
                reader.query::<_, u8>(*ledger, "icrc1_decimals", ()),
            )
            .await;
            symbol.and_then(|symbol| decimals.map(|d| (symbol, d, ledger.to_text())))
        }
        AssetKey::Evm {
            chain_id,
            token: Some(token),
        } => {
            let token = eip55(&token.into_array());
            let (symbol, decimals) = join(
                chains.evm(
                    *chain_id,
                    "eth_call",
                    json!([{"to":token,"data":"0x95d89b41"},"latest"]),
                ),
                chains.evm(
                    *chain_id,
                    "eth_call",
                    json!([{"to":token,"data":"0x313ce567"},"latest"]),
                ),
            )
            .await;
            decimals.and_then(|d| {
                let d = u8::from_str_radix(d.as_str().unwrap_or("").trim_start_matches("0x"), 16)
                    .map_err(|_| Error::IntegrityFailed)?;
                let symbol = symbol
                    .ok()
                    .and_then(|v| abi_text(v.as_str()?))
                    .unwrap_or_else(|| "ERC-20".into());
                Ok((
                    symbol,
                    d,
                    s.evm_rpc
                        .iter()
                        .find(|(id, _)| id == chain_id)
                        .map(|(_, s)| s.clone())
                        .unwrap_or_default(),
                ))
            })
        }
        AssetKey::Sol { mint: Some(mint) } => chains
            .sol(
                "getTokenSupply",
                json!([sol_address(&mint.into_array()),{"commitment":"confirmed"}]),
            )
            .await
            .and_then(|v| {
                let d = v["value"]["decimals"]
                    .as_u64()
                    .and_then(|n| u8::try_from(n).ok())
                    .ok_or(Error::IntegrityFailed)?;
                Ok(("SPL".into(), d, s.solana_rpc))
            }),
    };
    match read {
        Ok((symbol, decimals, source)) if decimals <= 38 => {
            out["symbol"] = json!(
                symbol
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(32)
                    .collect::<String>()
            );
            if matches!(
                asset,
                AssetKey::Evm {
                    chain_id: 1 | 8453,
                    token: None
                }
            ) {
                out["symbol"] = json!("ETH");
            }
            out["decimals"] = json!(decimals);
            out["source"] = json!(source);
            out["read_at"] = json!(now_ms());
            h.with(|c| c.workspace.put_private(c.keys()?, &cache, &out))
                .await?;
        }
        Ok(_) => out["error"] = json!("unsupported asset decimals"),
        Err(error) => out["error"] = json!(error.to_string()),
    }
    Ok(out)
}

fn abi_text(text: &str) -> Option<String> {
    let bytes = hex::decode(text.strip_prefix("0x")?).ok()?;
    let value = if bytes.len() == 32 {
        bytes.as_slice()
    } else {
        if bytes.len() < 64
            || bytes[..31].iter().any(|b| *b != 0)
            || bytes[31] != 32
            || bytes[32..63].iter().any(|b| *b != 0)
        {
            return None;
        }
        bytes.get(64..64 + usize::from(bytes[63]))?
    };
    String::from_utf8(value.iter().copied().take_while(|b| *b != 0).collect()).ok()
}

/// Metadata for all policy assets, fetched side by side.
pub async fn assets<H: Hold>(h: &H, wallet: Principal, refresh: bool) -> Result<Json> {
    let view = state(h, wallet).await?;
    Ok(
        json!({"wallet":wallet.to_text(),"assets":try_join_all(view.policy.spendable_assets.iter().map(|a|metadata(h,a,refresh))).await?}),
    )
}

/// A page of the wallet address book, with an opaque canonical cursor.
pub async fn book<H: Hold>(h: &H, wallet: Principal, after: Option<&str>) -> Result<Json> {
    let after = after
        .map(|s| {
            ensure_valid(s.len() <= 2048, "address book cursor")?;
            let bytes = hex::decode(s).map_err(|_| dmsg_types::invalid("address book cursor"))?;
            dmsg_protocol::decode_canonical::<Destination>(&bytes)
        })
        .transpose()?;
    let (_, ic) = identity(h).await?;
    let page: Result<Vec<BookEntry>> = ic.query(wallet, "address_book", (after, 200u16)).await?;
    let page = page?;
    let next = (page.len() == 200)
        .then(|| {
            page.last()
                .map(|e| hex::encode(dmsg_protocol::canonical(&e.destination)))
        })
        .flatten();
    Ok(
        json!({"wallet":wallet.to_text(),"next":next,"entries":page.iter().map(|e|json!({"destination":destination_text(&e.destination),"chain":match e.destination {Destination::Icrc(_)=>"Icp",Destination::Bitcoin(_)=>"Bitcoin",Destination::Evm(_)=>"Evm",Destination::Solana(_)=>"Solana"},"label":e.label,"added_at":e.added_at_ms})).collect::<Vec<_>>()}),
    )
}

/// Preflight common to creation, joining and deciding, before any payment.
pub async fn readiness<H: Hold>(h: &H, funding: bool) -> Result<Json> {
    let state = crate::account::refreshed(h).await?;
    let id = h
        .with(|c| {
            c.keys()?;
            Ok(c.workspace.meta.device_id)
        })
        .await?;
    let mut blockers = vec![];
    if state.info.sensitive_policy.frozen {
        blockers.push("WALLET_ACCOUNT_FROZEN");
    }
    match state.active(&id) {
        Ok(device)
            if device
                .input
                .capabilities
                .contains(&Capability::WalletApprove) => {}
        _ => blockers.push("WALLET_APPROVE_REQUIRED"),
    }
    if let Err(error) = ops::registrations(h).await {
        blockers.push(match error {
            Error::NotFound => "WALLET_REGISTRATION_REQUIRED",
            _ => "WALLET_REGISTRATION_UNAVAILABLE",
        });
    }
    let mut out = json!({"blockers":blockers,"read_at":now_ms()});
    if funding {
        let hub = h.with(|c| c.wallet_hub()).await?;
        let config: HubInit = h
            .anonymous()
            .await?
            .query(hub, "get_hub_config", ())
            .await?;
        let balance = crate::payer::payer_balance(h, config.icp_ledger).await?;
        out["payer"] = json!({"principal":balance["principal"]});
        out["balance"] = balance;
    }
    Ok(out)
}

/// Member identity as entered in a structured form.
pub fn member_id(text: &str) -> Result<MemberId> {
    match text.parse() {
        Ok(id) => Ok(MemberId::Account(id)),
        Err(_) => {
            let p = parse_wallet(text)?;
            dmsg_protocol::authenticated(p)?;
            Ok(MemberId::Principal(p))
        }
    }
}

/// A structured policy form. Amounts remain decimal strings of atomic units.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyInput {
    version: u64,
    approver_threshold: u8,
    admin_threshold: u8,
    tiers: Vec<TierInput>,
    daily_limits: Vec<LimitInput>,
    spendable_assets: Vec<String>,
    call_targets: Vec<String>,
    evm_chains: Vec<u64>,
    address_book_cooldown_ms: u64,
    proposal_ttl_ms: u64,
    unfreeze_delay_ms: u64,
    daily_signatures: u32,
    refuel: Option<RefuelInput>,
    hardware_cosigners: bool,
    public: bool,
}
#[derive(Deserialize)]
struct TierInput {
    asset: String,
    up_to: String,
    approvals: u8,
    delay_ms: u64,
    book_only: bool,
}
#[derive(Deserialize)]
struct LimitInput {
    asset: String,
    amount: String,
}
#[derive(Deserialize)]
struct RefuelInput {
    from: u32,
    min_cycles: String,
    icp_e8s: String,
    max_per_day: u8,
}

fn integer<T: std::str::FromStr>(text: &str) -> Result<T> {
    text.parse()
        .map_err(|_| dmsg_types::invalid("integer amount"))
}

impl PolicyInput {
    fn policy(self) -> Result<Policy> {
        Ok(Policy {
            version: self.version,
            approver_threshold: self.approver_threshold,
            admin_threshold: self.admin_threshold,
            tiers: self
                .tiers
                .into_iter()
                .map(|t| {
                    Ok(Tier {
                        asset: parse_asset(&t.asset)?,
                        up_to: integer(&t.up_to)?,
                        approvals: t.approvals,
                        delay_ms: t.delay_ms,
                        book_only: t.book_only,
                    })
                })
                .collect::<Result<_>>()?,
            daily_limits: self
                .daily_limits
                .into_iter()
                .map(|l| {
                    Ok(AssetLimit {
                        asset: parse_asset(&l.asset)?,
                        amount: integer(&l.amount)?,
                    })
                })
                .collect::<Result<_>>()?,
            spendable_assets: self
                .spendable_assets
                .iter()
                .map(|a| parse_asset(a))
                .collect::<Result<_>>()?,
            call_targets: self
                .call_targets
                .iter()
                .map(|p| parse_wallet(p))
                .collect::<Result<_>>()?,
            evm_chains: self.evm_chains,
            address_book_cooldown_ms: self.address_book_cooldown_ms,
            proposal_ttl_ms: self.proposal_ttl_ms,
            unfreeze_delay_ms: self.unfreeze_delay_ms,
            daily_signatures: self.daily_signatures,
            refuel: self
                .refuel
                .map(|r| {
                    Ok(RefuelRule {
                        from: r.from,
                        min_cycles: integer(&r.min_cycles)?,
                        icp_e8s: integer(&r.icp_e8s)?,
                        max_per_day: r.max_per_day,
                    })
                })
                .transpose()?,
            hardware_cosigners: self.hardware_cosigners,
            public: self.public,
        })
    }
}

/// A bounded management action; no renderer-provided Candid is executed directly.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[allow(missing_docs)] // Field names are the documented structured form contract.
pub enum Change {
    /// Invite or update a member.
    Member {
        id: String,
        role: Role,
        label: String,
        agent_ids: Vec<String>,
        update: bool,
    },
    /// Remove a member.
    RemoveMember { id: String },
    /// Replace the policy.
    Policy { policy: PolicyInput },
    /// Add or remove an address book entry.
    Book {
        asset: String,
        destination: String,
        label: String,
        remove: bool,
    },
    /// Add a chain-key or Segwit address.
    Address {
        chain: Chain,
        label: String,
        segwit: bool,
        cosign: Option<CosignPolicy>,
    },
    /// Change address cosigners.
    Cosign {
        address: u32,
        cosign: Option<CosignPolicy>,
    },
    /// Register a key already generated on this device.
    Cosigner { label: String },
    /// Remove a registered cosigner.
    RemoveCosigner { id: u32 },
    /// Unfreeze after the policy delay.
    Unfreeze,
    /// Top up from an ICP address.
    Refuel { from: u32, amount: String },
    /// Accept a registered module.
    Upgrade { version: u32, module_hash: String },
    /// Permanently remove upgrade authority.
    LockCode,
    /// Cancel an EVM transaction using its pending nonce.
    CancelNonce {
        address: u32,
        chain_id: u64,
        max_fee: String,
    },
    /// Release a pending nonce after independent reconciliation.
    ReleaseNonce { address: u32, chain_id: u64 },
    /// Initialize a Solana durable nonce.
    SolNonce { address: u32, max_fee: String },
}

/// Construct and submit one management proposal using current certified state.
pub async fn change<H: Hold>(h: &H, wallet: Principal, input: Change) -> Result<Json> {
    let view = state(h, wallet).await?;
    let intent = match input {
        Change::Member {
            id,
            role,
            label,
            agent_ids,
            update,
        } => {
            let m = MemberInput {
                id: member_id(&id)?,
                role,
                label,
                agent_ids,
            };
            if update {
                Intent::UpdateMember(m)
            } else {
                Intent::AddMember(m)
            }
        }
        Change::RemoveMember { id } => Intent::RemoveMember(member_id(&id)?),
        Change::Policy { policy } => {
            ensure(
                policy.version == view.policy.version + 1,
                Error::VersionConflict,
            )?;
            Intent::SetPolicy(policy.policy()?)
        }
        Change::Book {
            asset,
            destination,
            label,
            remove,
        } => {
            let destination =
                parse_destination(&parse_asset(&asset)?, &destination, view.btc_network)?;
            Intent::AddressBook(if remove {
                BookChange::Remove { destination }
            } else {
                BookChange::Add { destination, label }
            })
        }
        Change::Address {
            chain,
            label,
            segwit,
            cosign,
        } => {
            ensure_valid(!segwit || chain == Chain::Bitcoin, "Segwit chain")?;
            Intent::AddAddress {
                chain,
                kind: if segwit {
                    AddressKind::Segwit
                } else {
                    AddressKind::ChainKey
                },
                label,
                cosign,
            }
        }
        Change::Cosign { address, cosign } => Intent::SetCosign { address, cosign },
        Change::Cosigner { label } => {
            h.with(|c| {
                let key = c
                    .wallet_cosigner_keys()?
                    .into_iter()
                    .find(|k| k.label == label)
                    .ok_or(Error::NotFound)?;
                Ok(Intent::AddCosigner(key.input(c.wallet_member()?)))
            })
            .await?
        }
        Change::RemoveCosigner { id } => Intent::RemoveCosigner(id),
        Change::Unfreeze => Intent::Unfreeze,
        Change::Refuel { from, amount } => Intent::Refuel {
            from,
            icp_e8s: u64::try_from(atomic(&amount, 8)?)
                .map_err(|_| dmsg_types::invalid("ICP amount"))?,
        },
        Change::Upgrade {
            version,
            module_hash,
        } => Intent::AcceptUpgrade {
            version,
            module_hash: Hash::new(
                hex::decode(module_hash)
                    .ok()
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| dmsg_types::invalid("module hash"))?,
            ),
        },
        Change::LockCode => Intent::LockCode,
        Change::CancelNonce {
            address,
            chain_id,
            max_fee,
        } => Intent::CancelEvmNonce {
            address,
            chain_id,
            max_fee: integer(&max_fee)?,
        },
        Change::ReleaseNonce { address, chain_id } => Intent::ReleaseEvmNonce { address, chain_id },
        Change::SolNonce { address, max_fee } => Intent::InitSolNonce {
            address,
            max_fee: integer(&max_fee)?,
        },
    };
    ops::propose(h, wallet, intent, None).await
}

/// Registered versions and the wallet's current hub record.
pub async fn versions<H: Hold>(h: &H, wallet: Principal) -> Result<Json> {
    let hub = h.with(|c| c.wallet_hub()).await?;
    let ic = h.anonymous().await?;
    let (versions, record) = join(
        ic.query::<_, Vec<WalletVersion>>(hub, "list_versions", ()),
        ic.query::<_, Option<WalletRecord>>(hub, "get_wallet_record", (wallet,)),
    )
    .await;
    Ok(
        json!({"record":record?,"versions":versions?.iter().map(|v|json!({"version":v.version,"module_hash":hex::encode(*v.module_hash),"notes":v.notes,"retired":v.retired,"registered_at":v.registered_at_ms})).collect::<Vec<_>>()}),
    )
}

/// Run a hub action only after the wallet's own proposal accepted it.
pub async fn maintain<H: Hold>(h: &H, wallet: Principal, lock: bool) -> Result<Json> {
    let view = state(h, wallet).await?;
    ensure(
        if lock {
            matches!(view.code, CodeAcceptance::Lock)
        } else {
            matches!(view.code, CodeAcceptance::Upgrade { .. })
        },
        Error::Forbidden,
    )?;
    let hub = h.with(|c| c.wallet_hub()).await?;
    let (_, ic) = identity(h).await?;
    let record: Result<WalletRecord> = ic
        .update(
            hub,
            if lock {
                "lock_wallet"
            } else {
                "upgrade_wallet"
            },
            (wallet,),
        )
        .await?;
    Ok(json!({"record":record?}))
}

/// Live canister health, including a defensible idle-cycle estimate.
pub async fn health<H: Hold>(h: &H, wallet: Principal) -> Result<Json> {
    let hub = h.with(|c| c.wallet_hub()).await?;
    let (_, ic) = identity(h).await?;
    let health: Result<WalletHealth> = ic.update(hub, "wallet_health", (wallet,)).await?;
    let s = health?;
    Ok(
        json!({"cycles":s.cycles.to_string(),"memory_bytes":s.memory_bytes.to_string(),"idle_cycles_per_day":s.idle_cycles_per_day.to_string(),"idle_days":s.cycles.checked_div(s.idle_cycles_per_day).map(|days|days.to_string()),"running":s.running,"read_at":now_ms()}),
    )
}

/// A receive address, its QR image and, for ICP, both ledger address formats.
pub async fn receive<H: Hold>(h: &H, wallet: Principal, id: u32) -> Result<Json> {
    let view = state(h, wallet).await?;
    let a = view
        .addresses
        .iter()
        .find(|a| a.id == id)
        .ok_or(Error::NotFound)?;
    ensure(address_verified(&view, a), Error::IntegrityFailed)?;
    let code = qrcode::QrCode::new(a.text.as_bytes()).map_err(|_| Error::IntegrityFailed)?;
    let size = code.width();
    let scale = 6;
    let quiet = 4;
    let colors = code.to_colors();
    let img = image::GrayImage::from_fn(
        ((size + quiet * 2) * scale) as u32,
        ((size + quiet * 2) * scale) as u32,
        |x, y| {
            let x = x as usize / scale;
            let y = y as usize / scale;
            image::Luma([
                if x >= quiet
                    && y >= quiet
                    && x < size + quiet
                    && y < size + quiet
                    && colors[(y - quiet) * size + x - quiet] == qrcode::Color::Dark
                {
                    0
                } else {
                    255
                },
            ])
        },
    );
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| Error::IntegrityFailed)?;
    let mut out = json!({"wallet":wallet.to_text(),"address":id,"text":a.text,"chain":format!("{:?}",a.chain),"network":if a.chain==Chain::Bitcoin {format!("{:?}",view.btc_network)}else{String::new()},"verified":true,"qr":format!("data:image/png;base64,{}",STANDARD.encode(png.into_inner()))});
    if a.chain == Chain::Icp {
        let account = icp_account(wallet, id);
        let subaccount = account.subaccount.unwrap_or([0; 32]);
        let hash =
            Sha224::digest([b"\x0Aaccount-id".as_slice(), wallet.as_slice(), &subaccount].concat());
        let mut crc = 0xffff_ffffu32;
        for b in &hash {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320u32.wrapping_mul(crc & 1));
            }
        }
        out["owner"] = json!(wallet.to_text());
        out["subaccount"] = json!(hex::encode(subaccount));
        out["account_identifier"] = json!(hex::encode(
            [(!crc).to_be_bytes().as_slice(), &hash].concat()
        ));
    }
    Ok(out)
}

/// A transfer form. Amounts and fee are atomic integers serialized as text.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferInput {
    /// Source address.
    pub from: u32,
    /// Asset key.
    pub asset: String,
    /// Recipients.
    pub outputs: Vec<OutputInput>,
    /// Approved fee ceiling, absent only while estimating.
    pub max_fee: Option<String>,
    /// Optional ICRC memo in hex.
    pub memo: Option<String>,
}

/// A recipient and an exact atomic amount.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputInput {
    /// Recipient in the selected chain’s canonical address format.
    pub to: String,
    /// Exact positive integer in the asset’s atomic units.
    pub amount: String,
}

fn transfer_intent(input: &TransferInput, view: &WalletView, fee: u128) -> Result<Intent> {
    let asset = parse_asset(&input.asset)?;
    ensure(
        view.policy.spendable_assets.contains(&asset),
        Error::Forbidden,
    )?;
    let address = view
        .addresses
        .iter()
        .find(|a| a.id == input.from)
        .ok_or(Error::NotFound)?;
    let chain = match asset {
        AssetKey::Icp | AssetKey::Icrc(_) => Chain::Icp,
        AssetKey::Btc => Chain::Bitcoin,
        AssetKey::Evm { .. } => Chain::Evm,
        AssetKey::Sol { .. } => Chain::Solana,
    };
    ensure(
        address.chain == chain && address_verified(view, address),
        Error::IntegrityFailed,
    )?;
    let items = input
        .outputs
        .iter()
        .map(|o| {
            Ok(BatchItem {
                to: parse_destination(&asset, o.to.trim(), view.btc_network)?,
                amount: integer(&o.amount)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ensure_valid(!items.is_empty() && items.len() <= 64, "recipients")?;
    let intent = if items.len() == 1 {
        let memo = input
            .memo
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| {
                hex::decode(s)
                    .map(Into::into)
                    .map_err(|_| dmsg_types::invalid("memo hex"))
            })
            .transpose()?;
        Intent::Transfer {
            from: input.from,
            asset,
            to: items[0].to.clone(),
            amount: items[0].amount,
            max_fee: fee,
            memo,
        }
    } else {
        ensure_valid(
            input.memo.as_deref().is_none_or(str::is_empty),
            "batch memo",
        )?;
        Intent::Batch {
            from: input.from,
            asset,
            items,
            max_fee: fee,
        }
    };
    validate_intent(&intent)?;
    Ok(intent)
}

/// Estimate a transfer and ask the wallet for its current policy requirement.
/// The final proposal and execution still re-evaluate current state.
pub async fn preview<H: Hold>(h: &H, wallet: Principal, input: TransferInput) -> Result<Json> {
    let view = state(h, wallet).await?;
    let asset = parse_asset(&input.asset)?;
    let _provisional = transfer_intent(&input, &view, u128::MAX)?;
    let address = view
        .addresses
        .iter()
        .find(|a| a.id == input.from)
        .ok_or(Error::NotFound)?;
    let (_, ic) = identity(h).await?;
    let chains = h.with(|c| c.chains()).await?;
    let hub = h.with(|c| c.wallet_hub()).await?;
    let config: HubInit = h
        .anonymous()
        .await?
        .query(hub, "get_hub_config", ())
        .await?;
    let (estimate, native, decimals, balance, extra) = match &asset {
        AssetKey::Icp | AssetKey::Icrc(_) => {
            let ledger = match asset {
                AssetKey::Icrc(p) => p,
                _ => config.icp_ledger,
            };
            let fee = ledger_fee(&ic, ledger).await?;
            let b: Nat = ic
                .query(
                    ledger,
                    "icrc1_balance_of",
                    (icp_account(wallet, input.from),),
                )
                .await?;
            let meta = metadata(h, &asset, false).await?;
            (
                fee.checked_mul(input.outputs.len() as u128)
                    .ok_or_else(|| dmsg_types::invalid("fee overflow"))?,
                meta["symbol"].as_str().unwrap_or("ICRC").to_string(),
                meta["decimals"].as_u64().unwrap_or(0) as u8,
                u128::try_from(b.0).map_err(|_| Error::IntegrityFailed)?,
                0,
            )
        }
        AssetKey::Btc => {
            let utxos = chains.btc_utxos(&address.text).await?;
            let count = utxos.len().min(MAX_BTC_INPUTS);
            let balance = utxos
                .iter()
                .take(MAX_BTC_INPUTS)
                .try_fold(0u128, |sum, u| {
                    sum.checked_add(u128::from(u.value))
                        .ok_or(Error::IntegrityFailed)
                })?;
            let rate = chains.btc_fee_rate().await?;
            (
                u128::from(rate)
                    * (count as u128 * 110 + (input.outputs.len() as u128 + 1) * 43 + 16),
                "BTC".into(),
                8,
                balance,
                0,
            )
        }
        AssetKey::Evm { chain_id, token } => {
            let amount: u128 = integer(&input.outputs[0].amount)?;
            let destination = eip55(&parse_evm_address(&input.outputs[0].to)?);
            let (to, data, value) = match token {
                Some(token) => (
                    eip55(&token.into_array()),
                    [
                        &[0xa9, 0x05, 0x9c, 0xbb][..],
                        &[0; 12],
                        &parse_evm_address(&destination)?,
                        &[0; 16],
                        &amount.to_be_bytes(),
                    ]
                    .concat(),
                    0,
                ),
                None => (destination, vec![], amount),
            };
            let gas = chains
                .evm_gas(*chain_id, &address.text, &to, &data, value)
                .await?;
            let (fee, _) = chains.evm_fees(*chain_id).await?;
            let balance = chains.evm_balance(*chain_id, &address.text, None).await?;
            (
                fee.checked_mul(u128::from(gas))
                    .ok_or_else(|| dmsg_types::invalid("fee overflow"))?,
                if [1, 8453].contains(chain_id) {
                    "ETH".into()
                } else {
                    format!("Native · {chain_id}")
                },
                18,
                balance,
                0,
            )
        }
        AssetKey::Sol { mint } => {
            let mut extra = 0u128;
            if let Some(mint) = mint {
                let rent = chains
                    .sol("getMinimumBalanceForRentExemption", json!([165]))
                    .await?
                    .as_u64()
                    .ok_or(Error::IntegrityFailed)? as u128;
                for output in &input.outputs {
                    let owner = parse_sol_address(&output.to)?;
                    let ata = decode::associated_token_account(&owner, &mint.into_array());
                    if !chains.sol_exists(&sol_address(&ata)).await? {
                        extra = extra.checked_add(rent).ok_or(Error::IntegrityFailed)?;
                    }
                }
            }
            (
                5_000 + 60 * input.outputs.len() as u128,
                "SOL".into(),
                9,
                chains.sol_balance(&address.text).await?,
                extra,
            )
        }
    };
    let recommended = estimate
        .checked_add(estimate / 5)
        .ok_or_else(|| dmsg_types::invalid("fee overflow"))?;
    let ceiling = input
        .max_fee
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(integer)
        .transpose()?
        .unwrap_or(recommended);
    let intent = transfer_intent(&input, &view, ceiling)?;
    let total = input.outputs.iter().try_fold(0u128, |n, o| {
        n.checked_add(integer::<u128>(&o.amount)?)
            .ok_or_else(|| dmsg_types::invalid("total overflow"))
    })?;
    let native_asset = matches!(
        asset,
        AssetKey::Icp
            | AssetKey::Icrc(_)
            | AssetKey::Btc
            | AssetKey::Evm { token: None, .. }
            | AssetKey::Sol { mint: None }
    );
    let mut blockers = vec![];
    if ceiling < estimate {
        blockers.push("FEE_TOO_LOW");
    }
    if balance
        < ceiling
            .saturating_add(extra)
            .saturating_add(if native_asset { total } else { 0 })
    {
        blockers.push("INSUFFICIENT_NATIVE_BALANCE");
    }
    if !native_asset {
        let b = match &asset {
            AssetKey::Evm {
                chain_id,
                token: Some(token),
            } => {
                chains
                    .evm_balance(*chain_id, &address.text, Some(&eip55(&token.into_array())))
                    .await?
            }
            AssetKey::Sol { mint: Some(mint) } => {
                chains
                    .sol_token(&address.text, &sol_address(&mint.into_array()))
                    .await?
                    .0
            }
            _ => 0,
        };
        if b < total {
            blockers.push("INSUFFICIENT_ASSET_BALANCE");
        }
    }
    let requirement: Result<Requirement> = ic.query(wallet, "preview_intent", (&intent,)).await?;
    let requirement = requirement?;
    let requirement = json!({"approvals":requirement.approvals,"admin":requirement.admin,"delay_ms":requirement.delay_ms,"window_amount":requirement.window_amount.to_string()});
    Ok(json!({
        "wallet":wallet.to_text(), "policy_version":view.policy.version,
        "intent":intent_view(&intent), "estimated_fee":estimate.to_string(),
        "recommended_fee":recommended.to_string(), "max_fee":ceiling.to_string(),
        "fee_asset":native, "fee_decimals":decimals, "additional_reserve":extra.to_string(),
        "native_balance":balance.to_string(), "requirement":requirement,
        "blockers":blockers, "read_at":now_ms()
    }))
}

/// Submit exactly the reviewed transfer, without silently replacing its fee ceiling.
pub async fn transfer<H: Hold>(
    h: &H,
    wallet: Principal,
    input: TransferInput,
    policy_version: u64,
) -> Result<Json> {
    let view = state(h, wallet).await?;
    ensure(
        view.policy.version == policy_version,
        Error::VersionConflict,
    )?;
    let fee = integer(
        input
            .max_fee
            .as_deref()
            .ok_or_else(|| dmsg_types::invalid("review the fee first"))?,
    )?;
    let intent = transfer_intent(&input, &view, fee)?;
    ops::propose(h, wallet, intent, None).await
}

/// Bind a local notification channel; publishing still checks its membership.
pub fn channel_binding(c: &mut Client, wallet: Principal, channel: Option<&str>) -> Result<Json> {
    let key = format!("wallet-channel:{wallet}");
    if let Some(channel) = channel {
        ensure_valid(
            channel.is_empty() || c.channel_list()?.iter().any(|v| v.id == channel),
            "channel",
        )?;
        c.workspace
            .put_private(c.keys()?, &key, &channel.to_string())?;
    }
    Ok(json!({"channel": c.workspace.private::<String>(c.keys()?, &key)?}))
}

/// Publish an encrypted hint; recipients read the wallet to establish its actual state.
pub async fn notify<H: Hold>(h: &H, wallet: Principal, proposal: Option<u64>) -> Result<bool> {
    let view = state(h, wallet).await?;
    let channel = h
        .with(|c| {
            c.workspace
                .private::<String>(c.keys()?, &format!("wallet-channel:{wallet}"))
        })
        .await?
        .or(view.channel_hint.clone());
    let Some(channel) = channel.filter(|c| !c.is_empty()) else {
        return Ok(false);
    };
    let id = hex::encode(*digest("dmsg/wallet-event/v1", &(wallet, view.audit_head)));
    let sent = format!("wallet-announced:{wallet}");
    if h.with(|c| c.workspace.private::<String>(c.keys()?, &sent))
        .await?
        .as_deref()
        == Some(&id)
    {
        return Ok(false);
    }
    let p = match proposal {
        Some(id) => Some(proposal_raw(h, wallet, id).await?),
        None => None,
    };
    let draft = crate::channel::Draft {
        text: String::new(),
        packet: Some(
            json!({"format":"dmsg-wallet-event/1","channel":channel,"wallet":wallet.to_text(),"name":view.name,"proposal":proposal,"status":p.as_ref().map(|p|format!("{:?}",p.status)),"audit_head":hex::encode(*view.audit_head)}),
        ),
        ..Default::default()
    };
    crate::channel::send(h, &channel, &draft, Some(&id)).await?;
    h.with(|c| c.workspace.put_private(c.keys()?, &sent, &id))
        .await?;
    Ok(true)
}

/// Read activation progress independently of the long-running creation request.
pub async fn activation<H: Hold>(h: &H, quote: &str) -> Result<Json> {
    let id = Hash::new(
        hex::decode(quote)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| dmsg_types::invalid("quote ID"))?,
    );
    let (hub, payment) = h
        .with(|c| {
            Ok((
                c.wallet_hub()?,
                c.payment_journal(&format!("{ACTIVATION}{quote}"))?,
            ))
        })
        .await?;
    let order: Option<ActivationOrder> =
        h.anonymous().await?.query(hub, "get_order", (id,)).await?;
    let (stage, reason) = match order.as_ref().map(|o| &o.status) {
        Some(ActivationStatus::Paid) => ("creating", None),
        Some(ActivationStatus::Created) => ("installing", None),
        Some(ActivationStatus::Ready) => ("ready", None),
        Some(ActivationStatus::Failed(reason)) => ("failed", Some(reason.clone())),
        Some(ActivationStatus::Refunded { .. }) => ("refunded", None),
        None => match payment.as_ref().map(|p| p.stage) {
            Some(Stage::Confirmed) => ("paid", None),
            Some(Stage::Unknown) => ("payment_unknown", None),
            Some(Stage::Rejected) => (
                "payment_failed",
                payment.as_ref().and_then(|p| p.error.clone()),
            ),
            _ => ("unpaid", None),
        },
    };
    Ok(
        json!({"quote":quote,"stage":stage,"reason":reason,"wallet":order.and_then(|o|o.wallet).map(|p|p.to_text()),"payment_block":payment.and_then(|p|p.block),"read_at":now_ms()}),
    )
}

/// Continue an authorized activation, or ask for a fresh approval of the same
/// unexpired quote. Its payer journal prevents a second payment.
pub async fn continue_activation<H: Hold>(h: &H, source: &str, request: &str) -> Result<Json> {
    let (quote, spec) = h
        .with(|c| {
            let request = c.approval_record(request)?;
            let crate::approval::Action::Wallet { job, digest } = &request.action else {
                return Err(Error::Forbidden);
            };
            ensure(c.wallet_job_digest(job)? == *digest, Error::IntegrityFailed)?;
            let JobKind::Create { quote, spec } = c.wallet_job(job)?.kind else {
                return Err(Error::Forbidden);
            };
            Ok((quote, spec))
        })
        .await?;
    let hub = h.with(|c| c.wallet_hub()).await?;
    let (_, ic) = identity(h).await?;
    let order: Option<ActivationOrder> = ic.query(hub, "get_order", (quote.quote_id,)).await?;
    match order.map(|o| o.status) {
        Some(ActivationStatus::Failed(_) | ActivationStatus::Refunded { .. }) => Err(
            dmsg_types::invalid("activation cannot continue; check its refund"),
        ),
        Some(_) => {
            let order: Result<ActivationOrder> = ic
                .update(hub, "reconcile_activation", (quote.quote_id,))
                .await?;
            let order = order?;
            Ok(
                json!({"stage":if order.status==ActivationStatus::Ready {"ready"}else{"installing"},"wallet":order.wallet.map(|w|w.to_text())}),
            )
        }
        None => {
            h.with(|c| c.wallet_create_request(source, &hex::encode(*quote.quote_id), *spec))
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_inputs_never_round_or_overflow() {
        assert_eq!(
            atomic("1.000000000000000001", 18).unwrap(),
            1_000_000_000_000_000_001
        );
        assert_eq!(atomic("0.00000001", 8).unwrap(), 1);
        assert_eq!(atomic(&u128::MAX.to_string(), 0).unwrap(), u128::MAX);
        for value in ["1e8", "-1", "1.", " 1", "1.000000001"] {
            assert!(atomic(value, 8).is_err(), "{value}");
        }
        assert!(atomic("340282366920938463463374607431768211456", 0).is_err());
    }

    #[test]
    fn member_and_asset_forms_reject_bad_identifiers() {
        for value in ["a", "panda", "aaaaa-aa", "2vxsx-fae"] {
            assert!(member_id(value).is_err(), "{value}");
        }
        assert!(member_id("rrkah-fqaaa-aaaaa-aaaaq-cai").is_ok());
        for key in [
            "icp",
            "btc",
            "evm:1",
            "sol",
            "icrc:ryjl3-tyaaa-aaaaa-aaaba-cai",
        ] {
            assert_eq!(asset_key(&parse_asset(key).unwrap()), key);
        }
    }

    #[test]
    fn erc20_symbols_are_bounded_and_decoded_without_trusting_offsets() {
        let fixed = format!("0x{}", hex::encode([b"USDC".as_slice(), &[0; 28]].concat()));
        assert_eq!(abi_text(&fixed).as_deref(), Some("USDC"));
        let dynamic = format!(
            "0x{}",
            hex::encode([&[0; 31][..], &[32], &[0; 31], &[4], b"USDC", &[0; 28]].concat())
        );
        assert_eq!(abi_text(&dynamic).as_deref(), Some("USDC"));
        assert!(abi_text("0x20").is_none());
        assert!(abi_text(&dynamic.replace("000020", "0000ff")).is_none());
    }
}
