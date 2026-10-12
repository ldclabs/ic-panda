//! CLI commands of channels, the inbox, the wallet, plans and names.
//! Payments, checkouts and name operations create approval requests, which
//! an interactive terminal decides at once.

use crate::remote::{Failure, Remote};
use crate::{absolute, created, print};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ChannelCommand {
    /// Channels of this account.
    List,
    /// One channel: members, epoch and keys held.
    Show {
        /// Channel ID.
        id: String,
    },
    /// Create a channel this account owns; its first epoch follows.
    Create {
        /// Name, as this account calls it.
        name: String,
        /// direct, collaboration or distribution.
        #[arg(long = "type", default_value = "collaboration")]
        kind: String,
    },
    /// Invite an account; prints the invitation to hand to it.
    Invite {
        /// Channel ID.
        id: String,
        /// The invited account.
        account: String,
        /// member or publisher.
        #[arg(long, default_value = "member")]
        role: String,
    },
    /// Accept an invitation (the JSON `invite` printed).
    Accept {
        /// The invitation, as JSON.
        invitation: String,
    },
    /// Members and their roles.
    Members {
        /// Channel ID.
        id: String,
    },
    /// Change a member's role; owner only.
    Role {
        /// Channel ID.
        id: String,
        /// The member.
        account: String,
        /// admin, member or publisher.
        role: String,
        /// Let the member coordinate key rotations.
        #[arg(long)]
        can_rotate: bool,
    },
    /// Remove a member; a new epoch follows.
    Remove {
        /// Channel ID.
        id: String,
        /// The member.
        account: String,
    },
    /// Leave a channel.
    Leave {
        /// Channel ID.
        id: String,
    },
    /// Close a channel; owner only.
    Archive {
        /// Channel ID.
        id: String,
    },
    /// Start the next epoch now, when membership changed.
    Rotate {
        /// Channel ID.
        id: String,
    },
    /// Send a message, or share a vault file with --file.
    Send {
        /// Channel ID.
        id: String,
        /// The text.
        #[arg(default_value = "")]
        text: String,
        /// A file item of the vault.
        #[arg(long)]
        file: Option<String>,
        /// Message ID, to retry a send.
        #[arg(long)]
        message_id: Option<String>,
    },
    /// Read new messages and print what this device read.
    Read {
        /// Channel ID.
        id: String,
        /// Only messages after this sequence.
        #[arg(long, default_value_t = 0)]
        after: u64,
        /// Read every readable message again, as after a history grant.
        #[arg(long)]
        backfill: bool,
    },
    /// Download a shared file.
    Download {
        /// Channel ID.
        id: String,
        /// The message's sequence.
        seq: u64,
        /// Where to write it; must not exist.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Give a member keys of older epochs.
    History {
        /// Channel ID.
        id: String,
        /// The member.
        account: String,
        /// First epoch.
        from: u64,
        /// Last epoch.
        to: u64,
    },
    /// Hand ownership to a member: propose, the member accepts, you commit.
    Transfer {
        #[command(subcommand)]
        step: TransferStep,
    },
    /// Move the channel's billing to a member, in the same three steps.
    Sponsor {
        #[command(subcommand)]
        step: TransferStep,
    },
    /// Reopen an archived channel.
    Reactivate {
        /// Channel ID.
        id: String,
    },
    /// Operations waiting for their outcome, or settle one with --key.
    Pending {
        /// Channel ID.
        id: String,
        /// The operation to settle.
        #[arg(long)]
        key: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum TransferStep {
    /// Propose it to a member; prints the packet to hand over.
    Propose {
        /// Channel ID.
        id: String,
        /// The member.
        account: String,
    },
    /// Accept a proposal to this device; prints the packet to hand back.
    Accept {
        /// The packet, as JSON.
        packet: String,
    },
    /// Commit an accepted proposal.
    Commit {
        /// The packet, as JSON.
        packet: String,
    },
}

#[derive(Subcommand)]
pub enum InboxCommand {
    /// The contact policy in force.
    Policy,
    /// Publish a contact policy with a fresh inbox key.
    Configure {
        /// closed, contacts, free or paid.
        mode: String,
        /// For paid: the net amount a letter pays, in ledger units.
        #[arg(long)]
        price: Option<String>,
        /// For paid: who receives it; the wallet when absent.
        #[arg(long)]
        payee: Option<String>,
        /// Accounts that write for free.
        #[arg(long = "allow")]
        allow: Vec<String>,
    },
    /// Write to an account; with --pay, a paid letter returns its quote.
    Send {
        /// The recipient.
        recipient: String,
        /// The text.
        text: String,
        /// Pay from the wallet when the recipient charges.
        #[arg(long)]
        pay: bool,
        /// A contact invitation.
        #[arg(long)]
        invitation: Option<String>,
        /// Order ID, to retry.
        #[arg(long)]
        order: Option<String>,
    },
    /// Pay for a quoted letter; goes through the approval center.
    Pay {
        /// The recipient.
        recipient: String,
        /// Order ID.
        order: String,
    },
    /// Where a letter this account sent stands.
    Order {
        /// The recipient.
        recipient: String,
        /// Order ID.
        order: String,
    },
    /// Letters this account sent.
    Outgoing,
    /// A paid letter's escrow.
    Escrow {
        /// Order ID.
        order: String,
    },
    /// Recover a paid letter's funds after its acceptance deadline.
    Refund {
        /// Order ID.
        order: String,
    },
    /// Letters to this account.
    List,
    /// Mark a letter read, or archive it with --archive.
    Mark {
        /// Order ID.
        order: String,
        /// Archive it; its sender may write again.
        #[arg(long)]
        archive: bool,
    },
}

#[derive(Subcommand)]
pub enum WalletCommand {
    /// Pay with the identity in a PEM file, or with the login (no file).
    Use {
        /// The PEM file.
        pem: Option<PathBuf>,
    },
    /// The wallet's balance on a ledger.
    Balance {
        /// The ledger canister.
        ledger: String,
    },
}

#[derive(Subcommand)]
pub enum PlanCommand {
    /// The certified catalog.
    Catalog,
    /// This account's plan and limits.
    Status,
    /// Quote a subscription: cash on a ledger, or PANDA with a neuron.
    Quote {
        /// The SKU, such as plus.
        sku: String,
        /// cash or panda.
        #[arg(long, default_value = "cash")]
        method: String,
        /// The ledger (cash) or neuron ID in hex (panda).
        selection: String,
    },
    /// Approve a quote and pay it; goes through the approval center.
    Checkout {
        /// Checkout ID.
        id: String,
    },
    /// Approve a PANDA claim again after its cooling period.
    Advance {
        /// Checkout ID.
        id: String,
    },
    /// Where an order stands.
    Order {
        /// Checkout ID.
        id: String,
    },
    /// Every checkout of this device.
    List,
    /// Cancel an order before it applies.
    Cancel {
        /// Checkout ID.
        id: String,
    },
    /// Advance an order being applied.
    Reconcile {
        /// Checkout ID.
        id: String,
    },
    /// Refund deposits of a refunded order.
    Refund {
        /// Checkout ID.
        id: String,
        /// Ledger blocks of the deposits.
        blocks: Vec<u64>,
    },
}

#[derive(Subcommand)]
pub enum HandleCommand {
    /// Who owns a name, from a certified proof.
    Resolve {
        /// The name.
        name: String,
    },
    /// A name's price.
    Price {
        /// The name.
        name: String,
    },
    /// Claim a name the login owned before; goes through the approval center.
    Claim {
        /// The name.
        name: String,
    },
    /// Register a new name, paid from the wallet; goes through the approval center.
    Register {
        /// The name.
        name: String,
    },
    /// Offer a name this account owns to another account; prints a token
    /// for the target to accept.
    Transfer {
        /// The name.
        name: String,
        /// The target account.
        target: String,
    },
    /// Accept a transfer token naming this account; goes through the
    /// approval center. The owner then has about a minute to commit.
    Accept {
        /// The token printed by `dmsg handle transfer`.
        token: String,
    },
    /// Move the name once the target accepted; goes through the approval center.
    Commit {
        /// Operation ID printed by `dmsg handle transfer`.
        id: String,
    },
    /// This account's name operations.
    Operations,
    /// Settle a registration whose charge is unknown with its ledger block.
    Reconcile {
        /// Operation ID.
        id: String,
        /// The ledger block of the charge.
        #[arg(long)]
        block: u64,
    },
}

fn json_arg(text: &str) -> Result<Value, Failure> {
    serde_json::from_str(text).map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn message_lines(list: &Value) -> String {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            format!(
                "{:>5} {} {}{}",
                m["seq"],
                m["account"].as_str().unwrap_or_default(),
                m["text"].as_str().unwrap_or_default(),
                m["file"]["name"]
                    .as_str()
                    .map(|n| format!(" [file {n}]"))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn channel(remote: &Remote, json: bool, command: ChannelCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        ChannelCommand::List => ("channel/list", Value::Null),
        ChannelCommand::Show { id } | ChannelCommand::Members { id } => {
            ("channel/show", json!({"id": id}))
        }
        ChannelCommand::Create { name, kind } => {
            ("channel/create", json!({"name": name, "type": kind}))
        }
        ChannelCommand::Invite { id, account, role } => (
            "channel/invite",
            json!({"id": id, "account": account, "role": role}),
        ),
        ChannelCommand::Accept { invitation } => (
            "channel/join",
            json!({"invitation": json_arg(&invitation)?}),
        ),
        ChannelCommand::Role {
            id,
            account,
            role,
            can_rotate,
        } => (
            "channel/control",
            json!({"id": id, "action": {"type": "role", "account": account, "role": role, "can_rotate": can_rotate}}),
        ),
        ChannelCommand::Remove { id, account } => (
            "channel/control",
            json!({"id": id, "action": {"type": "remove", "account": account}}),
        ),
        ChannelCommand::Leave { id } => (
            "channel/control",
            json!({"id": id, "action": {"type": "leave"}}),
        ),
        ChannelCommand::Archive { id } => (
            "channel/control",
            json!({"id": id, "action": {"type": "archive"}}),
        ),
        ChannelCommand::Rotate { id } => ("channel/rotate", json!({"id": id})),
        ChannelCommand::Send {
            id,
            text,
            file: Some(item),
            message_id,
        } => (
            "channel/send_file",
            json!({"id": id, "item": item, "text": text, "message_id": message_id}),
        ),
        ChannelCommand::Send {
            id,
            text,
            file: None,
            message_id,
        } => (
            "channel/send",
            json!({"id": id, "text": text, "message_id": message_id}),
        ),
        ChannelCommand::Read {
            id,
            after,
            backfill,
        } => {
            remote
                .call("channel/sync", json!({"id": id, "backfill": backfill}))
                .await?;
            let list = remote
                .call("channel/messages", json!({"id": id, "after": after}))
                .await?;
            print(json, &list, message_lines);
            return Ok(());
        }
        ChannelCommand::Download { id, seq, out } => (
            "channel/download",
            json!({"id": id, "seq": seq, "path": absolute(out)?}),
        ),
        ChannelCommand::History {
            id,
            account,
            from,
            to,
        } => (
            "channel/history",
            json!({"id": id, "account": account, "from": from, "to": to}),
        ),
        ChannelCommand::Transfer { step } => match step {
            TransferStep::Propose { id, account } => (
                "channel/owner_propose",
                json!({"id": id, "account": account}),
            ),
            TransferStep::Accept { packet } => (
                "channel/owner_accept",
                json!({"packet": json_arg(&packet)?}),
            ),
            TransferStep::Commit { packet } => (
                "channel/owner_commit",
                json!({"packet": json_arg(&packet)?}),
            ),
        },
        ChannelCommand::Sponsor { step } => match step {
            TransferStep::Propose { id, account } => (
                "channel/sponsor_propose",
                json!({"id": id, "account": account}),
            ),
            TransferStep::Accept { packet } => (
                "channel/sponsor_accept",
                json!({"packet": json_arg(&packet)?}),
            ),
            TransferStep::Commit { packet } => (
                "channel/sponsor_commit",
                json!({"packet": json_arg(&packet)?}),
            ),
        },
        ChannelCommand::Reactivate { id } => ("channel/reactivate", json!({"id": id})),
        ChannelCommand::Pending { id, key: Some(key) } => {
            ("channel/resume", json!({"id": id, "key": key}))
        }
        ChannelCommand::Pending { id, key: None } => ("channel/pending", json!({"id": id})),
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}

pub async fn inbox(remote: &Remote, json: bool, command: InboxCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        InboxCommand::Policy => ("inbox/policy", Value::Null),
        InboxCommand::Configure {
            mode,
            price,
            payee,
            allow,
        } => (
            "inbox/configure",
            json!({"mode": mode, "price": price, "payee": payee, "allow": allow}),
        ),
        InboxCommand::Send {
            recipient,
            text,
            pay,
            invitation,
            order,
        } => (
            "inbox/send",
            json!({"recipient": recipient, "text": text, "pay": pay, "invitation": invitation, "order": order}),
        ),
        InboxCommand::Pay { recipient, order } => {
            let request = remote
                .call(
                    "inbox/pay",
                    json!({"recipient": recipient, "order": order, "source": "cli"}),
                )
                .await?;
            return created(remote, json, request).await;
        }
        InboxCommand::Order { recipient, order } => (
            "inbox/order",
            json!({"recipient": recipient, "order": order}),
        ),
        InboxCommand::Outgoing => ("inbox/outgoing", Value::Null),
        InboxCommand::Escrow { order } => ("inbox/escrow", json!({"order": order})),
        InboxCommand::Refund { order } => ("inbox/refund", json!({"order": order})),
        InboxCommand::List => ("inbox/list", Value::Null),
        InboxCommand::Mark { order, archive } => (
            "inbox/mark",
            json!({"order": order, "read": true, "archived": archive}),
        ),
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}

pub async fn wallet(remote: &Remote, json: bool, command: WalletCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        WalletCommand::Use { pem: Some(pem) } => ("wallet/use", json!({"pem": absolute(pem)?})),
        WalletCommand::Use { pem: None } => ("wallet/use", json!({})),
        WalletCommand::Balance { ledger } => ("wallet/balance", json!({"ledger": ledger})),
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}

pub async fn plan(remote: &Remote, json: bool, command: PlanCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        PlanCommand::Catalog => ("plan/catalog", Value::Null),
        PlanCommand::Status => ("plan/status", json!({"refresh": true})),
        PlanCommand::Quote {
            sku,
            method,
            selection,
        } => (
            "plan/quote",
            json!({"sku": sku, "method": method, "selection": selection}),
        ),
        PlanCommand::Checkout { id } => {
            let request = remote
                .call("plan/checkout", json!({"id": id, "source": "cli"}))
                .await?;
            return created(remote, json, request).await;
        }
        PlanCommand::Advance { id } => {
            let request = remote
                .call("plan/advance", json!({"id": id, "source": "cli"}))
                .await?;
            return created(remote, json, request).await;
        }
        PlanCommand::Order { id } => ("plan/order", json!({"id": id})),
        PlanCommand::List => ("plan/checkouts", Value::Null),
        PlanCommand::Cancel { id } => ("plan/cancel", json!({"id": id})),
        PlanCommand::Reconcile { id } => ("plan/reconcile", json!({"id": id})),
        PlanCommand::Refund { id, blocks } => ("plan/refund", json!({"id": id, "blocks": blocks})),
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}

pub async fn handle(remote: &Remote, json: bool, command: HandleCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        HandleCommand::Resolve { name } => ("handle/resolve", json!({"name": name})),
        HandleCommand::Price { name } => ("handle/price", json!({"name": name})),
        HandleCommand::Claim { name } => {
            let request = remote
                .call("handle/claim", json!({"name": name, "source": "cli"}))
                .await?;
            return created(remote, json, request).await;
        }
        HandleCommand::Register { name } => {
            let request = remote
                .call("handle/register", json!({"name": name, "source": "cli"}))
                .await?;
            return created(remote, json, request).await;
        }
        HandleCommand::Transfer { name, target } => {
            let offered = remote
                .call("handle/transfer", json!({"name": name, "target": target}))
                .await?;
            print(json, &offered, |v| {
                format!(
                    "On a device of {target} run:\n  dmsg handle accept {}\nthen here, within a minute of its approval:\n  dmsg handle commit {}",
                    v["token"].as_str().unwrap_or_default(),
                    v["operation"]["id"].as_str().unwrap_or_default()
                )
            });
            return Ok(());
        }
        HandleCommand::Accept { token } => {
            let request = remote
                .call("handle/accept", json!({"token": token, "source": "cli"}))
                .await?;
            return created(remote, json, request).await;
        }
        HandleCommand::Commit { id } => {
            let request = remote
                .call("handle/transfer_commit", json!({"id": id, "source": "cli"}))
                .await?;
            return created(remote, json, request).await;
        }
        HandleCommand::Operations => ("handle/operations", Value::Null),
        HandleCommand::Reconcile { id, block } => {
            ("handle/reconcile", json!({"id": id, "block": block}))
        }
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}
