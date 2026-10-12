//! CLI commands of account workspaces, account security, recovery, payout
//! revisions and commerce maintenance.

use crate::remote::{Failure, Remote};
use crate::{created, print, summary};
use clap::Subcommand;
use serde_json::{json, Value};

/// Account security: bound logins, the signing policy and recovery delay.
#[derive(Subcommand)]
pub enum SecurityCommand {
    /// Bound logins, the signing policy, the recovery delay and any
    /// pending recovery.
    Security,
    /// On a login without an account: ask an administrator device of
    /// `account` to bind this login; prints a request to hand over.
    BindRequest {
        /// The account to join.
        account: String,
    },
    /// Approve another login's request to be bound; goes through the
    /// approval center.
    BindApprove {
        /// The request printed by `dmsg account bind-request`.
        request: String,
        /// The login Principal shown on the other device.
        #[arg(long)]
        principal: String,
    },
    /// On the new login: accept the binding an administrator approved.
    BindAccept {
        /// The account.
        account: String,
    },
    /// Remove a bound login; goes through the approval center.
    Unbind {
        /// Login Principal.
        principal: String,
    },
    /// Change the signing policy; goes through the approval center.
    Policy {
        /// Freeze new formal signatures, or lift the freeze with --unfreeze.
        #[arg(long, conflicts_with = "unfreeze")]
        freeze: bool,
        /// Lift a freeze.
        #[arg(long)]
        unfreeze: bool,
        /// Formal executions a day, 0 to 64.
        #[arg(long)]
        daily: Option<u64>,
        /// Allowed purposes, comma separated: file_attestation, statement,
        /// app_action.
        #[arg(long, value_delimiter = ',')]
        purposes: Option<Vec<String>>,
    },
    /// Set the delay of a recovery, 1 to 7 days; goes through the approval center.
    RecoveryDelay {
        /// Days.
        days: u64,
    },
}

/// Recovery after every device is lost.
#[derive(Subcommand)]
pub enum RecoveryCommand {
    /// The pending recovery, if any.
    Status,
    /// On a new device logged in with a bound login: ask to take the
    /// account over after its recovery delay.
    Request,
    /// After the delay: take the account over, open the content root and
    /// rotate to a new one.
    Complete,
    /// Cancel a pending recovery from a device of the account; goes
    /// through the approval center.
    Dispute,
}

/// Escrow transfers of paid letters.
#[derive(Subcommand)]
pub enum PaymentCommand {
    /// An escrow's transfers.
    Transfers {
        /// Escrow ID.
        escrow: String,
    },
    /// Send a transfer the ledger refused again, at the fee it asked for.
    Revise {
        /// Escrow ID.
        escrow: String,
        /// Transfer leg.
        leg: u64,
    },
}

/// Commerce and membership records the wallet may read and repair.
#[derive(Subcommand)]
pub enum OpsCommand {
    /// Checkout orders with their per-ledger balances.
    Orders {
        /// Continue after this order.
        #[arg(long)]
        after: Option<String>,
    },
    /// Outgoing cash transfers.
    Transfers {
        /// Continue after this transfer.
        #[arg(long)]
        after: Option<String>,
    },
    /// PANDA claims and their eligibility leases.
    Claims {
        /// Continue after this claim.
        #[arg(long)]
        after: Option<String>,
    },
    /// Read or replay an order's original decision.
    ReconcileOrder {
        /// Order ID.
        id: String,
    },
    /// Send a transfer again with its original parameters.
    ProcessTransfer {
        /// Transfer ID.
        id: String,
    },
    /// As its recipient, replace a refused transfer at the fee the ledger asked for.
    ReviseTransfer {
        /// Transfer ID.
        id: String,
        /// The fee, in base units.
        #[arg(long)]
        fee: String,
    },
    /// Refresh a live claim's eligibility, or reconcile another.
    Claim {
        /// Claim ID.
        id: String,
        /// Refresh an active or terminated claim.
        #[arg(long)]
        refresh: bool,
    },
    /// Release or compact expired PANDA commitments.
    Sweep,
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// The account workspaces on this device.
pub async fn accounts(remote: &Remote, json: bool) -> Result<(), Failure> {
    let list = remote.call("session/accounts", Value::Null).await?;
    print(json, &list, |list| {
        list.as_array()
            .into_iter()
            .flatten()
            .map(|a| {
                format!(
                    "{}{} device {}{}{}",
                    if a["current"] == true { "* " } else { "  " },
                    a["account"].as_str().unwrap_or_default(),
                    &a["device_id"].as_str().unwrap_or_default()[..12],
                    if a["bound"] == true {
                        ""
                    } else {
                        " (waiting to pair or recover)"
                    },
                    if a["fast_unlock"] == true {
                        " fast unlock"
                    } else {
                        ""
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(())
}

pub async fn security(
    remote: &Remote,
    json: bool,
    command: SecurityCommand,
) -> Result<(), Failure> {
    let (method, params) = match command {
        SecurityCommand::Security => ("account/security", Value::Null),
        SecurityCommand::BindRequest { account } => {
            let request = remote
                .call("account/bind_request", json!({"account": account}))
                .await?;
            let principal = remote.status().await?["principal"].clone();
            print(json, &request, |v| {
                format!(
                    "login: {}\n\nOn an administrator device of the account run:\n  dmsg account bind-approve {} --principal {}\nthen here, within ten minutes of the approval:\n  dmsg account bind-accept {account}",
                    principal.as_str().unwrap_or_default(),
                    v["token"].as_str().unwrap_or_default(),
                    principal.as_str().unwrap_or_default(),
                )
            });
            return Ok(());
        }
        SecurityCommand::BindApprove { request, principal } => {
            let request = remote
                .call(
                    "account/bind_approve",
                    json!({"token": request, "principal": principal, "source": "cli"}),
                )
                .await?;
            return created(remote, json, request).await;
        }
        SecurityCommand::BindAccept { account } => {
            let status = remote
                .call("account/bind_accept", json!({"account": account}))
                .await?;
            print(json, &status, summary);
            return Ok(());
        }
        SecurityCommand::Unbind { principal } => {
            let request = remote
                .call(
                    "account/unbind",
                    json!({"principal": principal, "source": "cli"}),
                )
                .await?;
            return created(remote, json, request).await;
        }
        SecurityCommand::Policy {
            freeze,
            unfreeze,
            daily,
            purposes,
        } => {
            let mut params = json!({"source": "cli"});
            if freeze || unfreeze {
                params["frozen"] = freeze.into();
            }
            if let Some(daily) = daily {
                params["daily_executions"] = daily.into();
            }
            if let Some(purposes) = purposes {
                params["allowed_purposes"] = purposes.into();
            }
            let request = remote.call("account/policy", params).await?;
            return created(remote, json, request).await;
        }
        SecurityCommand::RecoveryDelay { days } => {
            let request = remote
                .call(
                    "account/recovery_delay",
                    json!({"days": days, "source": "cli"}),
                )
                .await?;
            return created(remote, json, request).await;
        }
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}

pub async fn recovery(
    remote: &Remote,
    json: bool,
    command: RecoveryCommand,
) -> Result<(), Failure> {
    let method = match command {
        RecoveryCommand::Status => "recovery/status",
        RecoveryCommand::Request => "recovery/request",
        RecoveryCommand::Complete => {
            let status = remote.call("recovery/complete", Value::Null).await?;
            print(json, &status, summary);
            return Ok(());
        }
        RecoveryCommand::Dispute => {
            let request = remote
                .call("recovery/dispute", json!({"source": "cli"}))
                .await?;
            return created(remote, json, request).await;
        }
    };
    let value = remote.call(method, Value::Null).await?;
    print(json, &value, pretty);
    Ok(())
}

pub async fn payment(remote: &Remote, json: bool, command: PaymentCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        PaymentCommand::Transfers { escrow } => ("payment/transfers", json!({"escrow": escrow})),
        PaymentCommand::Revise { escrow, leg } => {
            ("payment/revise", json!({"escrow": escrow, "leg": leg}))
        }
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}

pub async fn ops(remote: &Remote, json: bool, command: OpsCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        OpsCommand::Orders { after } => ("ops/orders", json!({"after": after})),
        OpsCommand::Transfers { after } => ("ops/transfers", json!({"after": after})),
        OpsCommand::Claims { after } => ("ops/claims", json!({"after": after})),
        OpsCommand::ReconcileOrder { id } => ("ops/reconcile_order", json!({"id": id})),
        OpsCommand::ProcessTransfer { id } => ("ops/process_transfer", json!({"id": id})),
        OpsCommand::ReviseTransfer { id, fee } => {
            ("ops/revise_transfer", json!({"id": id, "fee": fee}))
        }
        OpsCommand::Claim { id, refresh } => ("ops/claim", json!({"id": id, "refresh": refresh})),
        OpsCommand::Sweep => ("ops/sweep", Value::Null),
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}
