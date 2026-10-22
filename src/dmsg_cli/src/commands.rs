//! CLI commands of channels, the inbox, the payer, plans and names.
//! Payments, checkouts and name operations create approval requests, which
//! an interactive terminal decides at once.

use crate::approval_commands::request;
use crate::locale::tr;
use crate::remote::{Failure, Remote};
use crate::{absolute, pretty, print};
use clap::Subcommand;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ChannelCommand {
    /// Channels of this account.
    List,
    /// Delete an unactivated legacy inheritance draft.
    DeleteLegacyDraft {
        /// Channel ID.
        id: String,
    },
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
    /// Finish the rotation a channel waits for after a member or device
    /// change; an unlocked daemon that may rotate does it on its own.
    Rotate {
        /// Channel ID.
        id: String,
    },
    /// Start a new epoch now: who joins later reads nothing before it.
    Rekey {
        /// Channel ID.
        id: String,
    },
    /// Send a message, or share a library file with --file.
    Send {
        /// Channel ID.
        id: String,
        /// The text.
        #[arg(default_value = "")]
        text: String,
        /// A library file, `<id>` or `<id>@<version>`; shared as a new
        /// edition under its own key.
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
    /// Move the channel's billing to a member, in the same three steps;
    /// the channel's writes pause until it completes or is cancelled.
    Sponsor {
        #[command(subcommand)]
        step: SponsorStep,
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
    /// Messages on their way, or give one up with --discard.
    Outbox {
        /// Channel ID.
        id: String,
        /// The message ID to give up.
        #[arg(long)]
        discard: Option<String>,
    },
    /// Rename a channel for this account, mute it or hide it.
    Settings {
        /// Channel ID.
        id: String,
        /// The name this account calls it.
        #[arg(long)]
        name: Option<String>,
        /// Mute (true) or unmute (false) its notifications.
        #[arg(long)]
        mute: Option<bool>,
        /// Hide (true) it from the list or show (false) it again.
        #[arg(long)]
        hide: Option<bool>,
    },
    /// Messages this device read whose text or file name holds a query.
    Search {
        /// Channel ID.
        id: String,
        /// What to look for.
        query: String,
    },
    /// Withdraw an open invitation.
    Uninvite {
        /// Channel ID.
        id: String,
        /// The invitation ID.
        invitation: String,
    },
    /// How far each member's devices took and read the messages.
    Receipts {
        /// Channel ID.
        id: String,
    },
}

#[derive(Subcommand)]
pub enum SponsorStep {
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
    /// Call off the transfer under way, which resumes the channel's writes.
    Cancel {
        /// Channel ID.
        id: String,
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
        /// For paid: who receives it; the payer when absent.
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
        /// Pay from the payer when the recipient charges.
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
    /// Recover what a paid letter's escrow holds for the payer: the payment
    /// after its acceptance deadline, the unused fee reserve after
    /// settlement, and deposits it did not take.
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
pub enum PayerCommand {
    /// Pay with the identity in a PEM file, or with the login (no file).
    Use {
        /// The PEM file.
        pem: Option<PathBuf>,
    },
    /// The payer's balance on a ledger.
    Balance {
        /// The ledger canister.
        ledger: String,
    },
    /// Who pays, the login or a PEM identity, and its balance on every
    /// settlement asset and PANDA.
    Status,
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
    /// Apply a qualified PANDA claim immediately.
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
    /// What cancelling the first purchase within its refund window returns.
    RefundQuote {
        /// Checkout ID.
        id: String,
    },
    /// Forget a quote nobody approved, or an approved one past its deadline.
    Discard {
        /// Checkout ID.
        id: String,
    },
    /// The PANDA path: the stake each plan needs and the payer's neurons.
    Panda,
    /// Acknowledge the relay's over-limit notice; the export window starts.
    NoticeAck {
        /// Notice ID.
        id: String,
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
    /// Register a new name, paid from the payer; goes through the approval center.
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
    /// approval center. The owner then has ten minutes to commit.
    Accept {
        /// The token printed by `dmsg handle transfer`.
        token: String,
    },
    /// Move the name once the target accepted; goes through the approval center.
    Commit {
        /// Operation ID printed by `dmsg handle transfer`.
        id: String,
    },
    /// This device's name operations.
    Operations,
    /// This account's names, as the registry lists them.
    Mine,
    /// Print again the token of a transfer offered and not yet committed.
    Token {
        /// Operation ID.
        id: String,
    },
    /// Forget an operation that moved nothing.
    Forget {
        /// Operation ID.
        id: String,
    },
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

fn message_lines(list: &Value) -> String {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            let mut notes = vec![];
            if let Some(name) = m["file"]["name"].as_str() {
                notes.push(format!("[file {name}]"));
            }
            if let Some(seq) = m["reply_to"]["seq"].as_u64() {
                notes.push(format!("[reply to {seq}]"));
            }
            if let Some(agent) = m["agent"]["name"].as_str() {
                notes.push(format!("[agent {agent}]"));
            }
            if m["edited_at"].is_u64() {
                notes.push("[edited]".into());
            }
            if m["retracted"] == true {
                notes.push("[retracted]".into());
            }
            if m["unreadable"] == true {
                notes.push("[unreadable]".into());
            }
            format!(
                "{:>5} {} {}{}{}",
                m["seq"],
                m["account"].as_str().unwrap_or_default(),
                m["text"].as_str().unwrap_or_default(),
                if notes.is_empty() { "" } else { " " },
                notes.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn channel(remote: &Remote, json: bool, command: ChannelCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        ChannelCommand::List => ("channel/list", Value::Null),
        ChannelCommand::DeleteLegacyDraft { id } => {
            ("channel/delete_legacy_draft", json!({"id": id}))
        }
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
        ChannelCommand::Leave { id } => ("channel/leave", json!({"id": id})),
        ChannelCommand::Archive { id } => (
            "channel/control",
            json!({"id": id, "action": {"type": "archive"}}),
        ),
        ChannelCommand::Rotate { id } => ("channel/rotate", json!({"id": id})),
        ChannelCommand::Rekey { id } => ("channel/rekey", json!({"id": id})),
        ChannelCommand::Send {
            id,
            text,
            file: Some(file),
            message_id,
        } => {
            let (file, version) = match file.split_once('@') {
                Some((file, version)) => (file.to_string(), Some(version.to_string())),
                None => (file, None),
            };
            (
                "channel/send_file",
                json!({"id": id, "file": file, "version": version, "text": text, "message_id": message_id}),
            )
        }
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
            SponsorStep::Propose { id, account } => (
                "channel/sponsor_propose",
                json!({"id": id, "account": account}),
            ),
            SponsorStep::Accept { packet } => (
                "channel/sponsor_accept",
                json!({"packet": json_arg(&packet)?}),
            ),
            SponsorStep::Commit { packet } => (
                "channel/sponsor_commit",
                json!({"packet": json_arg(&packet)?}),
            ),
            SponsorStep::Cancel { id } => ("channel/sponsor_cancel", json!({"id": id})),
        },
        ChannelCommand::Reactivate { id } => ("channel/reactivate", json!({"id": id})),
        ChannelCommand::Pending { id, key: Some(key) } => {
            ("channel/resume", json!({"id": id, "key": key}))
        }
        ChannelCommand::Pending { id, key: None } => ("channel/pending", json!({"id": id})),
        ChannelCommand::Outbox {
            id,
            discard: Some(message_id),
        } => (
            "channel/discard",
            json!({"id": id, "message_id": message_id}),
        ),
        ChannelCommand::Outbox { id, discard: None } => ("channel/outbox", json!({"id": id})),
        ChannelCommand::Settings {
            id,
            name,
            mute,
            hide,
        } => (
            "channel/settings",
            json!({"id": id, "name": name, "muted": mute, "hidden": hide}),
        ),
        ChannelCommand::Search { id, query } => {
            let list = remote
                .call("channel/search", json!({"id": id, "query": query}))
                .await?;
            print(json, &list, message_lines);
            return Ok(());
        }
        ChannelCommand::Uninvite { id, invitation } => (
            "channel/control",
            json!({"id": id, "action": {"type": "revoke_invite", "invitation_id": invitation}}),
        ),
        ChannelCommand::Receipts { id } => ("channel/receipts", json!({"id": id})),
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
            let params = json!({"recipient": recipient, "order": order});
            return request(remote, json, "inbox/pay", params).await;
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

pub async fn payer(remote: &Remote, json: bool, command: PayerCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        PayerCommand::Use { pem: Some(pem) } => ("payer/use", json!({"pem": absolute(pem)?})),
        PayerCommand::Use { pem: None } => ("payer/use", json!({})),
        PayerCommand::Balance { ledger } => ("payer/balance", json!({"ledger": ledger})),
        PayerCommand::Status => ("payer/status", Value::Null),
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
            return request(remote, json, "plan/checkout", json!({"id": id})).await;
        }
        PlanCommand::Advance { id } => {
            return request(remote, json, "plan/advance", json!({"id": id})).await;
        }
        PlanCommand::Order { id } => ("plan/order", json!({"id": id})),
        PlanCommand::List => ("plan/checkouts", Value::Null),
        PlanCommand::Cancel { id } => ("plan/cancel", json!({"id": id})),
        PlanCommand::Reconcile { id } => ("plan/reconcile", json!({"id": id})),
        PlanCommand::Refund { id, blocks } => ("plan/refund", json!({"id": id, "blocks": blocks})),
        PlanCommand::RefundQuote { id } => ("plan/refund_quote", json!({"id": id})),
        PlanCommand::Discard { id } => ("plan/discard", json!({"id": id})),
        PlanCommand::Panda => ("plan/panda", Value::Null),
        PlanCommand::NoticeAck { id } => ("plan/notice_ack", json!({"id": id})),
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
            return request(remote, json, "handle/claim", json!({"name": name})).await;
        }
        HandleCommand::Register { name } => {
            return request(remote, json, "handle/register", json!({"name": name})).await;
        }
        HandleCommand::Transfer { name, target } => {
            let offered = remote
                .call("handle/transfer", json!({"name": name, "target": target}))
                .await?;
            print(json, &offered, |v| {
                let (token, operation) = (
                    v["token"].as_str().unwrap_or_default(),
                    v["operation"]["id"].as_str().unwrap_or_default(),
                );
                tr!(
                    "On a device of {target} run:\n  dmsg handle accept {token}\nthen here, within ten minutes of its approval:\n  dmsg handle commit {operation}",
                    "在 {target} 的设备上运行：\n  dmsg handle accept {token}\n批准后十分钟内在这里运行：\n  dmsg handle commit {operation}"
                )
            });
            return Ok(());
        }
        HandleCommand::Accept { token } => {
            return request(remote, json, "handle/accept", json!({"token": token})).await;
        }
        HandleCommand::Commit { id } => {
            return request(remote, json, "handle/transfer_commit", json!({"id": id})).await;
        }
        HandleCommand::Operations => ("handle/operations", Value::Null),
        HandleCommand::Mine => ("handle/mine", Value::Null),
        HandleCommand::Token { id } => ("handle/token", json!({"id": id})),
        HandleCommand::Forget { id } => ("handle/forget", json!({"id": id})),
        HandleCommand::Reconcile { id, block } => {
            ("handle/reconcile", json!({"id": id, "block": block}))
        }
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}
