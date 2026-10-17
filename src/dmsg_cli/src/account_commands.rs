//! CLI commands of the login and account workspaces, the account and its
//! devices, the public profile, account security, recovery, payout
//! revisions and commerce maintenance.

use crate::approval_commands::{created, request};
use crate::locale::tr;
use crate::remote::{Failure, Remote};
use crate::{absolute, open_browser, pretty, print, short, summary};
use clap::Subcommand;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Subcommand)]
pub enum ProfileCommand {
    /// The published profile of this account.
    Show,
    /// Publish exactly the given fields; anything left out is unpublished.
    Publish {
        /// Display name.
        #[arg(long, default_value = "")]
        name: String,
        /// Bio.
        #[arg(long, default_value = "")]
        bio: String,
        /// HTTPS links.
        #[arg(long = "link")]
        links: Vec<String>,
    },
    /// Another account's published profile, checked against its devices.
    Resolve {
        /// Account ID.
        account: String,
    },
    /// Publish an image (PNG, JPEG or WebP, at most 512 KiB) as the public
    /// avatar, which the relay serves to anyone; the other fields stay.
    Avatar {
        /// The image.
        #[arg(required_unless_present = "remove")]
        file: Option<PathBuf>,
        /// Remove the avatar instead.
        #[arg(long, conflicts_with = "file")]
        remove: bool,
    },
}

#[derive(Subcommand)]
pub enum AccountCommand {
    /// Create an account for the current login with this device as administrator.
    Create,
    /// Show the account and its devices.
    Show,
    #[command(flatten)]
    Security(SecurityCommand),
}

#[derive(Subcommand)]
pub enum DeviceCommand {
    /// List the account's devices.
    List,
    /// Ask to join the login's account; prints a request for an administrator device.
    PairRequest {
        /// Join as a member instead of an administrator.
        #[arg(long)]
        member: bool,
    },
    /// Approve a pairing request and rotate the content root to include the device.
    Approve {
        /// The request printed by `dmsg device pair-request`.
        request: String,
        /// The device ID shown on the new device.
        #[arg(long)]
        device: String,
    },
    /// Replace a device's capabilities; goes through the approval center.
    Capabilities {
        /// Device ID.
        device: String,
        /// The complete set, comma separated: content_sign, vault_unlock,
        /// root_manage, formal_approve, payment_offer, wallet_approve.
        #[arg(long, value_delimiter = ',')]
        set: Vec<String>,
    },
    /// Let this device approve formal signatures and web requests, or stop.
    FormalApprove {
        /// on or off.
        state: String,
    },
    /// Revoke another device and rotate the content root without it.
    Revoke {
        /// Device ID.
        device: String,
    },
}

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

/// Commerce and membership records the payer may read and repair.
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

/// The account workspaces on this device.
pub async fn accounts(remote: &Remote, json: bool) -> Result<(), Failure> {
    let list = remote.call("session/accounts", Value::Null).await?;
    print(json, &list, |list| {
        list.as_array()
            .into_iter()
            .flatten()
            .map(|a| {
                format!(
                    "{}{} {} {}{}{}",
                    if a["current"] == true { "* " } else { "  " },
                    a["account"].as_str().unwrap_or_default(),
                    tr!("device", "设备"),
                    short(a["device_id"].as_str().unwrap_or_default(), 12),
                    if a["bound"] == true {
                        String::new()
                    } else {
                        tr!(" (waiting to pair or recover)", "（等待配对或恢复）")
                    },
                    if a["fast_unlock"] == true {
                        tr!(" fast unlock", " 快速解锁")
                    } else {
                        String::new()
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
                let (login, token) = (
                    principal.as_str().unwrap_or_default(),
                    v["token"].as_str().unwrap_or_default(),
                );
                tr!(
                    "login: {login}\n\nOn an administrator device of the account run:\n  dmsg account bind-approve {token} --principal {login}\nthen here, within ten minutes of the approval:\n  dmsg account bind-accept {account}",
                    "登录：{login}\n\n在账户的一台管理员设备上运行：\n  dmsg account bind-approve {token} --principal {login}\n批准后十分钟内在这里运行：\n  dmsg account bind-accept {account}"
                )
            });
            return Ok(());
        }
        SecurityCommand::BindApprove {
            request: token,
            principal,
        } => {
            return request(
                remote,
                json,
                "account/bind_approve",
                json!({"token": token, "principal": principal}),
            )
            .await;
        }
        SecurityCommand::BindAccept { account } => {
            let status = remote
                .call("account/bind_accept", json!({"account": account}))
                .await?;
            print(json, &status, summary);
            return Ok(());
        }
        SecurityCommand::Unbind { principal } => {
            return request(
                remote,
                json,
                "account/unbind",
                json!({"principal": principal}),
            )
            .await;
        }
        SecurityCommand::Policy {
            freeze,
            unfreeze,
            daily,
            purposes,
        } => {
            let mut params = json!({});
            if freeze || unfreeze {
                params["frozen"] = freeze.into();
            }
            if let Some(daily) = daily {
                params["daily_executions"] = daily.into();
            }
            if let Some(purposes) = purposes {
                params["allowed_purposes"] = purposes.into();
            }
            return request(remote, json, "account/policy", params).await;
        }
        SecurityCommand::RecoveryDelay { days } => {
            return request(
                remote,
                json,
                "account/recovery_delay",
                json!({"days": days}),
            )
            .await;
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
            return request(remote, json, "recovery/dispute", json!({})).await;
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

/// `dmsg login`: through the dMsg login page; with `bind`, the login is
/// bound to the unlocked account through the approval center.
pub async fn login(
    remote: &Remote,
    json: bool,
    origin: &str,
    no_browser: bool,
    bind: bool,
) -> Result<(), Failure> {
    let origin = match origin {
        "dmsg.net" | "https://dmsg.net" => "https://dmsg.net",
        "panda.fans" | "https://panda.fans" => "https://panda.fans",
        _ => {
            return Err(Failure::new(
                "INVALID_INPUT",
                "origin must be dmsg.net or panda.fans",
            ));
        }
    };
    let started = remote
        .call(
            "session/login",
            json!({"origin": origin, "bind": bind, "source": "cli"}),
        )
        .await?;
    let url = started["url"].as_str().unwrap_or_default();
    if json {
        println!("{}", json!({"url": url}));
    } else {
        println!("{url}");
        eprintln!(
            "{}",
            tr!(
                "Waiting for the login in your browser…",
                "等待在浏览器中完成登录…"
            )
        );
    }
    if !no_browser && !open_browser(url) {
        eprintln!(
            "{}",
            tr!(
                "Open the URL above in your browser.",
                "请在浏览器中打开上面的链接。"
            )
        );
    }
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let result = remote.call("session/login_result", Value::Null).await?;
        if result["pending"] == true {
            continue;
        }
        if let Some(error) = result["error"]["message"].as_str() {
            return Err(Failure::new("AUTH_REQUIRED", error));
        }
        if bind {
            return created(remote, json, result["approval"].clone()).await;
        }
        if result["status"]["principal"].is_null() {
            return Err(Failure::new("EXPIRED", "the login was not completed"));
        }
        print(json, &result["status"], summary);
        return Ok(());
    }
}

/// `dmsg account`.
pub async fn account(remote: &Remote, json: bool, command: AccountCommand) -> Result<(), Failure> {
    match command {
        AccountCommand::Create => {
            let created = remote.call("account/create", Value::Null).await?;
            print(json, &created, |v| {
                let (account, generation) = (
                    v["account"].as_str().unwrap_or_default(),
                    &v["root_generation"],
                );
                tr!(
                    "account: {account}\nroot generation: {generation}",
                    "账户：{account}\n内容根代数：{generation}"
                )
            });
        }
        AccountCommand::Show => {
            let view = remote.call("account/show", Value::Null).await?;
            print(json, &view, pretty);
        }
        AccountCommand::Security(command) => security(remote, json, command).await?,
    }
    Ok(())
}

/// `dmsg device`.
pub async fn device(remote: &Remote, json: bool, command: DeviceCommand) -> Result<(), Failure> {
    match command {
        DeviceCommand::List => {
            let view = remote.call("account/show", Value::Null).await?;
            print(json, &view["devices"], |devices| {
                devices
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|d| {
                        format!(
                            "{}{} {} {}{}",
                            if d["current"] == true { "* " } else { "  " },
                            d["device_id"].as_str().unwrap_or_default(),
                            d["role"].as_str().unwrap_or_default(),
                            d["capabilities"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(","),
                            if d["revoked_at"].is_null() {
                                String::new()
                            } else {
                                tr!(" revoked", " 已撤销")
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        DeviceCommand::PairRequest { member } => {
            let role = if member { "member" } else { "administrator" };
            let request = remote
                .call("device/pair_request", json!({"role": role}))
                .await?;
            print(json, &request, |v| {
                let (device, token) = (
                    v["device_id"].as_str().unwrap_or_default(),
                    v["token"].as_str().unwrap_or_default(),
                );
                tr!(
                    "device: {device}\n\nOn an administrator device run:\n  dmsg device approve {token} --device {device}",
                    "设备：{device}\n\n在一台管理员设备上运行：\n  dmsg device approve {token} --device {device}"
                )
            });
        }
        DeviceCommand::Approve {
            request: token,
            device,
        } => {
            let approved = remote
                .call("device/approve", json!({"token": token, "device": device}))
                .await?;
            print(json, &approved, |v| {
                let (device, generation) = (
                    v["device_id"].as_str().unwrap_or_default(),
                    &v["root_generation"],
                );
                tr!(
                    "approved {device}\nroot generation: {generation}",
                    "已批准 {device}\n内容根代数：{generation}"
                )
            });
        }
        DeviceCommand::Capabilities { device, set } => {
            let params = json!({"device": device, "capabilities": set});
            request(remote, json, "device/capabilities", params).await?
        }
        DeviceCommand::FormalApprove { state } => {
            let enable = match state.as_str() {
                "on" => true,
                "off" => false,
                _ => return Err(Failure::new("INVALID_INPUT", "on or off")),
            };
            let params = json!({"enable": enable});
            request(remote, json, "device/formal_approve", params).await?
        }
        DeviceCommand::Revoke { device } => {
            request(remote, json, "device/revoke", json!({"device": device})).await?
        }
    }
    Ok(())
}

/// `dmsg profile`.
pub async fn profile(remote: &Remote, json: bool, command: ProfileCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        ProfileCommand::Show => ("profile/show", Value::Null),
        ProfileCommand::Publish { name, bio, links } => (
            "profile/publish",
            json!({"display_name": name, "bio": bio, "links": links}),
        ),
        ProfileCommand::Resolve { account } => ("profile/resolve", json!({"account": account})),
        ProfileCommand::Avatar { file, remove } => {
            let params = match file {
                Some(file) => json!({"path": absolute(file)?}),
                None => json!({"remove": remove}),
            };
            let profile = remote.call("profile/avatar", params).await?;
            print(json, &profile, |p| {
                let version = &p["profile"]["version"];
                match p["profile"]["avatar_upload"].as_str() {
                    Some(upload) => tr!(
                        "published version {version} with avatar {upload}",
                        "已发布版本 {version}，头像 {upload}"
                    ),
                    None => tr!(
                        "published version {version} without an avatar",
                        "已发布版本 {version}，不带头像"
                    ),
                }
            });
            return Ok(());
        }
    };
    let profile = remote.call(method, params).await?;
    print(json, &profile, pretty);
    Ok(())
}
