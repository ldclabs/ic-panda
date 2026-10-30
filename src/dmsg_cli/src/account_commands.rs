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
    /// The official PrincipalDocument of this account, or an empty draft
    /// before the first publication.
    Show,
    /// Publish an edited PrincipalDocument JSON file (from `profile show --json`).
    Publish {
        /// UTF-8 PrincipalDocument JSON.
        document: PathBuf,
    },
    /// Reconcile an uncertain document update and retry directory publication.
    Retry,
    /// Another account's independently certified PrincipalDocument.
    Resolve {
        /// Account ID.
        account: String,
    },
    /// Upload a public raster image and print its URL; publish the document separately.
    Avatar {
        /// PNG, JPEG or WebP, at most 512 KiB.
        file: PathBuf,
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
    /// Ask to approve a pairing request and rotate the content root to
    /// include the device; goes through the approval center.
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
    /// This device's approval key and which approvals need one.
    #[command(subcommand)]
    Key(KeyCommand),
    /// Replace an administrator device with a new one, beyond the plan's
    /// device limit: the old device hands over its keys and wallets.
    #[command(subcommand)]
    Succession(SuccessionCommand),
    /// Move this device's local content, today the file library, to
    /// another device of the account over the local network, end to end
    /// encrypted.
    #[command(subcommand)]
    Transfer(TransferCommand),
}

/// A direct transfer between two devices of the account: the receiving
/// device opens a session, the sending device connects to it.
#[derive(Subcommand)]
pub enum TransferCommand {
    /// On the receiving device: open a session and wait for the sender;
    /// prints what to run there.
    Receive {
        /// Resume this session after a broken connection.
        #[arg(long)]
        session: Option<String>,
        /// The succession whose content step it settles.
        #[arg(long)]
        succession: Option<String>,
        /// Wait until the transfer ends.
        #[arg(long)]
        wait: bool,
    },
    /// On the sending device: send to the session the receiver shows.
    Send {
        /// The session the receiving device shows.
        session: String,
        /// The receiving device's ID; a succession names it.
        #[arg(long)]
        to: Option<String>,
        /// The receiver's address and port, where Bonjour does not find it.
        #[arg(long)]
        address: Option<String>,
        /// The succession whose content step it settles.
        #[arg(long)]
        succession: Option<String>,
        /// Wait until the transfer ends.
        #[arg(long)]
        wait: bool,
    },
    /// The running transfer, or the last one.
    Status,
    /// Stop the running transfer.
    Cancel,
}

/// A planned device replacement. The new device asks, the device it
/// replaces approves and hands over, and the new device revokes it.
#[derive(Subcommand)]
pub enum SuccessionCommand {
    /// On the new device, signed in to the account's login: ask to replace
    /// `predecessor`; prints a request for that device. Makes the new
    /// device's approval and cosigner keys, asking for Touch ID here.
    Request {
        /// The ID of the device being replaced.
        predecessor: String,
    },
    /// On the device being replaced: what a request asks, and the wallets
    /// the handover covers.
    Review {
        /// The request printed by `dmsg device succession request`.
        request: String,
    },
    /// On the device being replaced: ask to approve the new device; goes
    /// through the approval center.
    Approve {
        /// The request printed by `dmsg device succession request`.
        request: String,
        /// The device ID shown on the new device.
        #[arg(long)]
        device: String,
    },
    /// This device's successions.
    List,
    /// A succession's steps, after reading what the account shows done.
    Status {
        /// Succession ID.
        id: String,
    },
    /// Run a step on this device: authorize, approval_key, cosigners,
    /// wallets or revoke.
    Run {
        /// Succession ID.
        id: String,
        /// The step.
        step: String,
    },
    /// Leave local content or native addresses behind.
    Skip {
        /// Succession ID.
        id: String,
        /// content or native_addresses.
        step: String,
        /// What stays behind, comma separated.
        #[arg(long, value_delimiter = ',')]
        keep: Vec<String>,
    },
    /// Stop a succession; what was committed stays.
    Cancel {
        /// Succession ID.
        id: String,
    },
}

/// Approval keys: hardware keys whose signature the user home requires for
/// the approvals the account protects.
#[derive(Subcommand)]
pub enum KeyCommand {
    /// This device's key, every device's key and the protected purposes.
    Status,
    /// Make a new key on this device and ask to register it; goes through
    /// the approval center, where the key proves itself.
    Register {
        /// Take only the fingerprints enrolled now, never the login password.
        #[arg(long)]
        biometry_only: bool,
    },
    /// Make a new key and print a request for an administrator device that
    /// has a key: for a device without a working key once keys are
    /// required. Asks for Touch ID here.
    Request {
        /// Take only the fingerprints enrolled now, never the login password.
        #[arg(long)]
        biometry_only: bool,
    },
    /// Ask to register another device's key from its request; goes
    /// through the approval center.
    Approve {
        /// The request printed by `dmsg device key request`.
        request: String,
        /// The device ID shown on that device.
        #[arg(long)]
        device: String,
    },
    /// Ask to retire a device's key, this device's by default; goes
    /// through the approval center.
    Retire {
        /// Device ID.
        #[arg(long)]
        device: Option<String>,
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
    /// Require approval keys for account, device and Agent principal
    /// changes, and optionally payments and attestations, or no longer
    /// (`off`); goes through the approval center. Devices without a key
    /// then cannot make those approvals; a login recovery still can take
    /// the account over after its delay.
    Hardware {
        /// on or off.
        state: String,
        /// Also payments: checkouts, PANDA subscriptions and names.
        #[arg(long)]
        payment: bool,
        /// Also formal attestations and authentications.
        #[arg(long)]
        attestation: bool,
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
        SecurityCommand::Hardware {
            state,
            payment,
            attestation,
        } => {
            let purposes = match state.as_str() {
                "on" => {
                    let mut purposes =
                        vec!["account_control", "device_control", "principal_control"];
                    purposes.extend(payment.then_some("payment"));
                    purposes.extend(attestation.then_some("attestation"));
                    purposes
                }
                "off" => vec![],
                _ => return Err(Failure::new("INVALID_INPUT", "on or off")),
            };
            let params = json!({"purposes": purposes});
            return request(remote, json, "account/hardware_policy", params).await;
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
        // The page ran out before the browser answered.
        if result["done"] != true {
            return Err(Failure::new("EXPIRED", "the login was not completed"));
        }
        if bind {
            return created(remote, json, result["approval"].clone()).await;
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
            let params = json!({"token": token, "device": device});
            request(remote, json, "device/approve", params).await?
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
        DeviceCommand::Key(command) => key(remote, json, command).await?,
        DeviceCommand::Succession(command) => succession(remote, json, command).await?,
        DeviceCommand::Transfer(command) => transfer(remote, json, command).await?,
    }
    Ok(())
}

/// `dmsg device succession`.
async fn succession(
    remote: &Remote,
    json: bool,
    command: SuccessionCommand,
) -> Result<(), Failure> {
    let (method, params) = match command {
        SuccessionCommand::Request { predecessor } => {
            let params = json!({"predecessor": predecessor});
            let made = remote.call("device/succession_request", params).await?;
            print(json, &made, |v| {
                let (device, token) = (
                    v["device_id"].as_str().unwrap_or_default(),
                    v["token"].as_str().unwrap_or_default(),
                );
                tr!(
                    "device: {device}\n\nOn the device being replaced run:\n  dmsg device succession approve {token} --device {device}",
                    "设备：{device}\n\n在被替换的设备上运行：\n  dmsg device succession approve {token} --device {device}"
                )
            });
            return Ok(());
        }
        SuccessionCommand::Approve {
            request: token,
            device,
        } => {
            let params = json!({"token": token, "device": device});
            return request(remote, json, "device/succession_approve", params).await;
        }
        SuccessionCommand::Run { id, step } => {
            let params = json!({"id": id, "step": step, "source": "cli"});
            let made = remote.call("device/succession_run", params).await?;
            // A step asks in the approval center, once per wallet when
            // taking over bindings, or reports what it did.
            if made["kind"].is_string() {
                return created(remote, json, made).await;
            }
            if let Some(requests) = made["requests"].as_array() {
                for made in requests.clone() {
                    created(remote, json, made).await?;
                }
                return Ok(());
            }
            print(json, &made, pretty);
            return Ok(());
        }
        SuccessionCommand::Review { request } => {
            ("device/succession_review", json!({"token": request}))
        }
        SuccessionCommand::List => ("device/successions", Value::Null),
        SuccessionCommand::Status { id } => ("device/succession_status", json!({"id": id})),
        SuccessionCommand::Skip { id, step, keep } => (
            "device/succession_skip",
            json!({"id": id, "step": step, "skipped": keep}),
        ),
        SuccessionCommand::Cancel { id } => ("device/succession_cancel", json!({"id": id})),
    };
    let view = remote.call(method, params).await?;
    print(json, &view, pretty);
    Ok(())
}

/// `dmsg device key`.
async fn key(remote: &Remote, json: bool, command: KeyCommand) -> Result<(), Failure> {
    match command {
        KeyCommand::Status => {
            let view = remote.call("device/key", Value::Null).await?;
            print(json, &view, |v| {
                let mut lines = vec![match v["key"]["key_id"].as_str() {
                    Some(id) if v["key"]["here"] == true => {
                        tr!("this device: key {}", "本设备：钥匙 {}", short(id, 16))
                    }
                    Some(id) => tr!(
                        "this device: key {} is not on this device",
                        "本设备：钥匙 {} 不在本机",
                        short(id, 16)
                    ),
                    None => tr!("this device: no key", "本设备：无钥匙"),
                }];
                if v["key"]["fingerprints_changed"] == true {
                    lines.push(tr!(
                        "the fingerprints changed; the key no longer works",
                        "指纹已变化，此钥匙已失效"
                    ));
                }
                let policy = v["policy"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>();
                lines.push(match policy.is_empty() {
                    true => tr!("required for: nothing", "需要钥匙的批准：无"),
                    false => tr!("required for: {}", "需要钥匙的批准：{}", policy.join(", ")),
                });
                for d in v["devices"].as_array().into_iter().flatten() {
                    lines.push(format!(
                        "{}{} {}",
                        if d["this_device"] == true { "* " } else { "  " },
                        d["device_id"].as_str().unwrap_or_default(),
                        d["key_id"]
                            .as_str()
                            .map_or_else(|| tr!("no key", "无钥匙"), |k| short(k, 16).to_string()),
                    ));
                }
                lines.join("\n")
            });
        }
        KeyCommand::Register { biometry_only } => {
            let params = json!({"biometry_only": biometry_only});
            request(remote, json, "device/key_register", params).await?
        }
        KeyCommand::Request { biometry_only } => {
            let params = json!({"biometry_only": biometry_only});
            let made = remote.call("device/key_request", params).await?;
            print(json, &made, |v| {
                let (device, token) = (
                    v["device_id"].as_str().unwrap_or_default(),
                    v["token"].as_str().unwrap_or_default(),
                );
                tr!(
                    "device: {device}\n\nOn an administrator device with a key run:\n  dmsg device key approve {token} --device {device}",
                    "设备：{device}\n\n在一台有钥匙的管理员设备上运行：\n  dmsg device key approve {token} --device {device}"
                )
            });
        }
        KeyCommand::Approve {
            request: token,
            device,
        } => {
            let params = json!({"token": token, "device": device});
            request(remote, json, "device/key_approve", params).await?
        }
        KeyCommand::Retire { device } => {
            let params = json!({"device": device});
            request(remote, json, "device/key_retire", params).await?
        }
    }
    Ok(())
}

/// `dmsg profile`.
pub async fn profile(remote: &Remote, json: bool, command: ProfileCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        ProfileCommand::Show => ("profile/show", Value::Null),
        ProfileCommand::Retry => ("profile/retry", Value::Null),
        ProfileCommand::Publish { document } => {
            let bytes = std::fs::read(document)
                .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
            ("profile/publish", value)
        }
        ProfileCommand::Resolve { account } => ("profile/resolve", json!({"account": account})),
        ProfileCommand::Avatar { file } => {
            let params = json!({"path": absolute(file)?});
            let url = remote.call("profile/avatar", params).await?;
            print(json, &url, |url| {
                url.as_str().unwrap_or_default().to_string()
            });
            return Ok(());
        }
    };
    let profile = remote.call(method, params).await?;
    print(json, &profile, pretty);
    Ok(())
}

async fn transfer(remote: &Remote, json: bool, command: TransferCommand) -> Result<(), Failure> {
    let (method, params, wait) = match command {
        TransferCommand::Receive {
            session,
            succession,
            wait,
        } => (
            "transfer/receive_start",
            json!({"session": session, "succession": succession}),
            wait,
        ),
        TransferCommand::Send {
            session,
            to,
            address,
            succession,
            wait,
        } => (
            "transfer/send_start",
            json!({"session": session, "to": to, "address": address, "succession": succession}),
            wait,
        ),
        TransferCommand::Status => ("transfer/status", Value::Null, false),
        TransferCommand::Cancel => ("transfer/cancel", Value::Null, false),
    };
    let mut view = remote.call(method, params).await?;
    if method == "transfer/receive_start" && !json {
        let (session, to) = (
            view["session"].as_str().unwrap_or_default(),
            view["to"].as_str().unwrap_or_default(),
        );
        let address = view["addresses"][0].as_str().unwrap_or("<address:port>");
        println!(
            "{}",
            tr!(
                "On the sending device run:\n  dmsg device transfer send {session} --to {to}\nwhere Bonjour does not find this device, add --address {address}\n",
                "在发送的设备上运行：\n  dmsg device transfer send {session} --to {to}\n如果 Bonjour 找不到本设备，加上 --address {address}\n"
            )
        );
    }
    while wait && matches!(view["state"].as_str(), Some("waiting" | "running")) {
        tokio::time::sleep(Duration::from_millis(500)).await;
        view = remote.call("transfer/status", Value::Null).await?;
    }
    print(json, &view, pretty);
    match view["state"].as_str() {
        Some("failed") => Err(Failure::new(
            view["error"]["code"].as_str().unwrap_or("UNAVAILABLE"),
            view["error"]["message"].as_str().unwrap_or_default(),
        )),
        _ => Ok(()),
    }
}
