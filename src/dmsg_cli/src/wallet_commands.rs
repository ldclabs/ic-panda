//! `dmsg wallet`: team wallets. Creating, joining, deciding and cosigning
//! wait in the approval center, decided here at an interactive terminal.

use crate::locale::tr;
use crate::remote::{Failure, Remote};
use crate::{created, pretty, print};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Team wallets: one canister per team, holding ICP and ICRC tokens and
/// signing Bitcoin, EVM and Solana transactions with chain keys.
#[derive(Subcommand)]
pub enum WalletCommand {
    /// The account's wallets.
    List,
    /// A wallet: members, policy, addresses (checked against its keys) and cosigners.
    Show {
        /// Wallet canister ID.
        wallet: String,
    },
    /// This device's wallet key Principal, which joining binds.
    Principal,
    /// Price and quota for creating a wallet.
    Quote,
    /// Pay a quote and create a wallet; goes through the approval center.
    Create {
        /// The quote ID from `dmsg wallet quote`.
        quote: String,
        /// Wallet name.
        #[arg(long)]
        name: Option<String>,
        /// A member as `<account or principal>:<role>[:label]`; repeat.
        #[arg(long = "member")]
        members: Vec<String>,
        /// Approver approvals for spending.
        #[arg(long, default_value_t = 1)]
        approvals: u8,
        /// Admin approvals for changes.
        #[arg(long, default_value_t = 1)]
        admin_approvals: u8,
        /// A complete wallet spec as JSON instead of the flags above.
        #[arg(long)]
        spec: Option<PathBuf>,
    },
    /// Join a wallet this account was added to; goes through the approval center.
    Join {
        /// Wallet canister ID.
        wallet: String,
    },
    /// Proposals, newest first.
    Proposals {
        /// Wallet canister ID.
        wallet: String,
        /// Only proposals before this ID.
        #[arg(long)]
        before: Option<u64>,
    },
    /// One proposal with its plans.
    Proposal {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
    },
    /// Propose a transfer.
    Send {
        /// Wallet canister ID.
        wallet: String,
        /// Source address ID.
        #[arg(long)]
        from: u32,
        /// `icp`, `icrc:<ledger>`, `btc`, `evm:<chain>[:<token>]`, `sol[:<mint>]`.
        #[arg(long)]
        asset: String,
        /// Recipient.
        #[arg(long)]
        to: String,
        /// Amount in base units.
        #[arg(long)]
        amount: String,
        /// Highest network fee in the chain's base units.
        #[arg(long)]
        max_fee: String,
        /// ICRC memo, hex.
        #[arg(long)]
        memo: Option<String>,
    },
    /// Propose any intent, given as JSON.
    Propose {
        /// Wallet canister ID.
        wallet: String,
        /// The intent as JSON.
        intent: PathBuf,
    },
    /// Approve a proposal; goes through the approval center.
    Approve {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
    },
    /// Reject a proposal; goes through the approval center.
    Reject {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
    },
    /// Veto a proposal during its delay; goes through the approval center.
    Veto {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
    },
    /// Withdraw your own proposal.
    Cancel {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
    },
    /// Stop every execution of the wallet at once.
    Freeze {
        /// Wallet canister ID.
        wallet: String,
    },
    /// Execute an approved proposal: sign, broadcast and report.
    Execute {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
    },
    /// Broadcast a signed transaction again.
    Broadcast {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
        /// Plan; the latest signed one by default.
        #[arg(long)]
        plan: Option<u32>,
    },
    /// Check the chain and report the proposal settled once final.
    Settle {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
    },
    /// Report a signed plan broadcast, settled or failed.
    Report {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
        /// Plan.
        plan: u32,
        /// `broadcast`, `settled` or `failed`.
        outcome: String,
        /// Note.
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Resend an interrupted ICP transfer or top-up.
    Reconcile {
        /// Wallet canister ID.
        wallet: String,
        /// Proposal ID.
        id: u64,
    },
    /// Balances of every address.
    Balances {
        /// Wallet canister ID.
        wallet: String,
    },
    /// Verify the audit chain.
    Audit {
        /// Wallet canister ID.
        wallet: String,
    },
    /// Top the wallet up with cycles under its refuel rule.
    Refuel {
        /// Wallet canister ID.
        wallet: String,
    },
    /// Cosigner keys of this device.
    Cosigners,
    /// Create a cosigner key on this device; prints the AddCosigner intent.
    CosignerNew {
        /// Local label.
        label: String,
        /// Refuse a software key.
        #[arg(long)]
        hardware: bool,
    },
}

fn member(text: &str) -> Result<Value, Failure> {
    let mut parts = text.splitn(3, ':');
    let id = parts.next().unwrap_or_default();
    let role = parts.next().unwrap_or("approver");
    let label = parts.next().unwrap_or_default();
    if id.is_empty() {
        return Err(Failure::new("INVALID_INPUT", "member"));
    }
    Ok(json!({"id": id, "role": role, "label": label}))
}

fn read_json(path: &PathBuf) -> Result<Value, Failure> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Failure::new("INVALID_INPUT", format!("{}: {e}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|e| Failure::new("INVALID_INPUT", format!("{}: {e}", path.display())))
}

fn proposal_line(p: &Value) -> String {
    let intent = &p["intent"];
    let what = match intent["kind"].as_str().unwrap_or_default() {
        "transfer" | "batch" => {
            let outputs: Vec<String> = intent["outputs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|o| {
                    format!(
                        "{} → {}",
                        o["amount"].as_str().unwrap_or_default(),
                        o["to"].as_str().unwrap_or_default()
                    )
                })
                .collect();
            format!(
                "{} {}",
                intent["asset"]["asset"].as_str().unwrap_or_default(),
                outputs.join(", ")
            )
        }
        kind => kind.to_string(),
    };
    let approvals = p["votes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| v["decision"] == "Approve")
        .count();
    format!(
        "#{} {:<9} {approvals}/{} {what}",
        p["id"],
        p["status"].as_str().unwrap_or_default(),
        p["requirement"]["approvals"],
    )
}

fn wallet_lines(w: &Value) -> String {
    let mut lines = vec![format!(
        "{} {}{}",
        w["name"].as_str().unwrap_or_default(),
        w["wallet"].as_str().unwrap_or_default(),
        if w["frozen"] == true {
            tr!(" (frozen)", "（已冻结）")
        } else {
            String::new()
        }
    )];
    for m in w["members"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  {} {:<9} {:<8} {}",
            m["id"].as_str().unwrap_or_default(),
            m["role"].as_str().unwrap_or_default(),
            m["status"].as_str().unwrap_or_default(),
            m["label"].as_str().unwrap_or_default()
        ));
    }
    for a in w["addresses"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  [{}] {:<8} {}{}",
            a["id"],
            a["chain"].as_str().unwrap_or_default(),
            a["text"].as_str().unwrap_or_default(),
            if a["verified"] == true {
                String::new()
            } else {
                tr!(" (NOT VERIFIED)", "（未通过核对）")
            }
        ));
    }
    lines.join("\n")
}

pub async fn wallet(remote: &Remote, json: bool, command: WalletCommand) -> Result<(), Failure> {
    let call =
        |method: &'static str, params: Value| async move { remote.call(method, params).await };
    match command {
        WalletCommand::List => {
            let list = call("wallet/list", Value::Null).await?;
            print(json, &list, |l| {
                l.as_array()
                    .into_iter()
                    .flatten()
                    .map(|w| {
                        format!(
                            "{} {}",
                            w["wallet"].as_str().unwrap_or_default(),
                            w["name"].as_str().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        WalletCommand::Show { wallet } => {
            let w = call("wallet/show", json!({"wallet": wallet})).await?;
            print(json, &w, wallet_lines);
        }
        WalletCommand::Principal => {
            let p = call("wallet/principal", Value::Null).await?;
            print(json, &p, |p| {
                p["principal"].as_str().unwrap_or_default().to_string()
            });
        }
        WalletCommand::Quote => {
            let q = call("wallet/quote", Value::Null).await?;
            print(json, &q, |q| {
                tr!(
                    "quote {} · {} e8s ICP plus fee {} · {} wallets left on {} · until {}",
                    "报价 {} · {} e8s ICP，另加手续费 {} · {} 套餐还可创建 {} 个 · 截止 {}",
                    q["quote_id"].as_str().unwrap_or_default(),
                    q["price_e8s"].as_str().unwrap_or_default(),
                    q["ledger_fee_e8s"].as_str().unwrap_or_default(),
                    q["remaining"],
                    q["plan"].as_str().unwrap_or_default(),
                    q["expires_at"]
                )
            });
        }
        WalletCommand::Create {
            quote,
            name,
            members,
            approvals,
            admin_approvals,
            spec,
        } => {
            let mut params = json!({"quote": quote, "source": "cli"});
            match spec {
                Some(path) => params["spec"] = read_json(&path)?,
                None => {
                    params["name"] =
                        json!(name.ok_or_else(|| Failure::new("INVALID_INPUT", "--name"))?);
                    params["members"] = Value::Array(
                        members
                            .iter()
                            .map(|m| member(m))
                            .collect::<Result<_, _>>()?,
                    );
                    params["approvals"] = json!(approvals);
                    params["admin_approvals"] = json!(admin_approvals);
                }
            }
            created(remote, json, call("wallet/create", params).await?).await?;
        }
        WalletCommand::Join { wallet } => {
            created(
                remote,
                json,
                call("wallet/join", json!({"wallet": wallet, "source": "cli"})).await?,
            )
            .await?;
        }
        WalletCommand::Proposals { wallet, before } => {
            let page = call(
                "wallet/proposals",
                json!({"wallet": wallet, "before": before}),
            )
            .await?;
            print(json, &page, |p| {
                p["proposals"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(proposal_line)
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        WalletCommand::Proposal { wallet, id } => {
            let p = call("wallet/proposal", json!({"wallet": wallet, "proposal": id})).await?;
            print(json, &p, pretty);
        }
        WalletCommand::Send {
            wallet,
            from,
            asset,
            to,
            amount,
            max_fee,
            memo,
        } => {
            let p = call(
                "wallet/send",
                json!({"wallet": wallet, "from": from, "asset": asset, "to": to, "amount": amount, "max_fee": max_fee, "memo": memo}),
            )
            .await?;
            print(json, &p, proposal_line);
        }
        WalletCommand::Propose { wallet, intent } => {
            let p = call(
                "wallet/propose",
                json!({"wallet": wallet, "intent": read_json(&intent)?}),
            )
            .await?;
            print(json, &p, proposal_line);
        }
        WalletCommand::Approve { wallet, id } => {
            created(
                remote,
                json,
                call(
                    "wallet/approve",
                    json!({"wallet": wallet, "proposal": id, "source": "cli"}),
                )
                .await?,
            )
            .await?;
        }
        WalletCommand::Reject { wallet, id } => {
            created(
                remote,
                json,
                call(
                    "wallet/reject",
                    json!({"wallet": wallet, "proposal": id, "source": "cli"}),
                )
                .await?,
            )
            .await?;
        }
        WalletCommand::Veto { wallet, id } => {
            created(
                remote,
                json,
                call(
                    "wallet/veto",
                    json!({"wallet": wallet, "proposal": id, "source": "cli"}),
                )
                .await?,
            )
            .await?;
        }
        WalletCommand::Cancel { wallet, id } => {
            let p = call("wallet/cancel", json!({"wallet": wallet, "proposal": id})).await?;
            print(json, &p, proposal_line);
        }
        WalletCommand::Freeze { wallet } => {
            let r = call("wallet/freeze", json!({"wallet": wallet})).await?;
            print(json, &r, |_| tr!("frozen", "已冻结"));
        }
        WalletCommand::Execute { wallet, id } => {
            let r = call(
                "wallet/execute",
                json!({"wallet": wallet, "proposal": id, "source": "cli"}),
            )
            .await?;
            // A cosigned plan comes back as an approval request.
            if r["kind"] == "wallet" && !r["digest"].is_null() {
                created(remote, json, r).await?;
            } else {
                print(json, &r, |p| {
                    let mut line = proposal_line(p);
                    if let Some(txid) = p["broadcast"].as_str() {
                        line.push_str(&tr!("\nbroadcast {txid}", "\n已广播 {txid}"));
                    }
                    if let Some(error) = p["broadcast_error"].as_str() {
                        line.push_str(&tr!(
                            "\nsigned, broadcast failed: {error}; retry with dmsg wallet broadcast",
                            "\n已签名，广播失败：{error}；可用 dmsg wallet broadcast 重试"
                        ));
                    }
                    line
                });
            }
        }
        WalletCommand::Broadcast { wallet, id, plan } => {
            let p = call(
                "wallet/broadcast",
                json!({"wallet": wallet, "proposal": id, "plan": plan}),
            )
            .await?;
            print(json, &p, proposal_line);
        }
        WalletCommand::Settle { wallet, id } => {
            let p = call("wallet/settle", json!({"wallet": wallet, "proposal": id})).await?;
            print(json, &p, |p| match p["pending"] == true {
                true => tr!("not final yet", "尚未最终确认"),
                false => proposal_line(p),
            });
        }
        WalletCommand::Report {
            wallet,
            id,
            plan,
            outcome,
            note,
        } => {
            let p = call(
                "wallet/report",
                json!({"wallet": wallet, "proposal": id, "plan": plan, "outcome": outcome, "note": note}),
            )
            .await?;
            print(json, &p, proposal_line);
        }
        WalletCommand::Reconcile { wallet, id } => {
            let p = call(
                "wallet/reconcile",
                json!({"wallet": wallet, "proposal": id}),
            )
            .await?;
            print(json, &p, proposal_line);
        }
        WalletCommand::Balances { wallet } => {
            let b = call("wallet/balances", json!({"wallet": wallet})).await?;
            print(json, &b, |b| {
                b["addresses"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|a| {
                        let head = format!(
                            "[{}] {}",
                            a["address"],
                            a["text"].as_str().unwrap_or_default()
                        );
                        let rows = a["balances"].as_array().cloned().unwrap_or_default();
                        std::iter::once(head).chain(rows.into_iter().map(|r| {
                            let value = r["balance"]["Ok"]
                                .as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| r["balance"].to_string());
                            format!("    {} {value}", r["asset"].as_str().unwrap_or_default())
                        }))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        WalletCommand::Audit { wallet } => {
            let a = call("wallet/audit", json!({"wallet": wallet})).await?;
            print(json, &a, |a| {
                tr!(
                    "audit chain verified: {} events, head {}",
                    "审计链已核对：{} 条，末端 {}",
                    a["events"],
                    a["head"].as_str().unwrap_or_default()
                )
            });
        }
        WalletCommand::Refuel { wallet } => {
            let r = call("wallet/refuel", json!({"wallet": wallet})).await?;
            print(json, &r, |r| match r["cycles"].as_str() {
                Some(c) => tr!("topped up {c} cycles", "已充值 {c} cycles"),
                None => tr!("no top-up needed", "无需充值"),
            });
        }
        WalletCommand::Cosigners => {
            let c = call("wallet/cosigners", Value::Null).await?;
            print(json, &c, pretty);
        }
        WalletCommand::CosignerNew { label, hardware } => {
            let c = call(
                "wallet/cosigner_new",
                json!({"label": label, "hardware": hardware}),
            )
            .await?;
            print(json, &c, pretty);
        }
    }
    Ok(())
}
