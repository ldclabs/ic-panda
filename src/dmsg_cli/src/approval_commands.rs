//! CLI commands of the approval center, formal statements and web
//! applications through dmsg-connect/1. A request is rendered for review
//! here, and only a person at an interactive terminal decides it.

use crate::home::Home;
use crate::locale::{row, tr};
use crate::remote::{Failure, Remote};
use crate::{absolute, ask, interactive, open_browser, pretty, print, short};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Subcommand)]
pub enum ApprovalsCommand {
    /// Requests waiting for a decision, or every request with --all.
    List {
        /// Include decided requests.
        #[arg(long)]
        all: bool,
    },
    /// Everything a decision needs.
    Show {
        /// Request ID or a unique prefix.
        id: String,
    },
    /// Approve after reviewing it; interactive terminals only.
    Approve {
        /// Request ID or a unique prefix.
        id: String,
    },
    /// Reject; interactive terminals only.
    Reject {
        /// Request ID or a unique prefix.
        id: String,
    },
    /// Withdraw a request this device's own clients made.
    Cancel {
        /// Request ID or a unique prefix.
        id: String,
    },
    /// Quote a web application's checkout before deciding it: cash on a
    /// ledger, or a PANDA waiver backed by a neuron.
    Quote {
        /// Request ID or a unique prefix.
        id: String,
        /// Ledger Principal (cash) or neuron ID in hex (PANDA).
        selection: String,
    },
}

#[derive(Args)]
pub struct StatementFields {
    /// What the statement is about (a URI or text).
    #[arg(long)]
    subject: Option<String>,
    /// Media type of the original.
    #[arg(long)]
    content_type: Option<String>,
    /// URI of the original.
    #[arg(long)]
    location: Option<String>,
}

#[derive(Subcommand)]
pub enum AttestCommand {
    /// Sign a text statement.
    Text {
        /// The text, 1 to 4,096 bytes.
        text: String,
        /// What the text is about.
        #[arg(long)]
        subject: Option<String>,
    },
    /// Sign the SHA-256 of a file, or of a digest given in hex.
    Digest {
        /// The file.
        #[arg(long, conflicts_with = "sha256")]
        file: Option<PathBuf>,
        /// The digest, when no file is given.
        #[arg(long)]
        sha256: Option<String>,
        #[command(flatten)]
        fields: StatementFields,
    },
    /// Sign a statement about one file.
    File {
        /// The file.
        path: PathBuf,
        /// The statement.
        #[arg(long)]
        text: String,
        #[command(flatten)]
        fields: StatementFields,
    },
    /// Sign an application action its product prepared: the action's
    /// canonical CBOR, as a page's `signAction` would send it.
    Action {
        /// The file holding the action, or `-` for standard input.
        path: PathBuf,
    },
    /// Finished and pending attestations.
    History,
    /// This month's formal-execution allowance and the archived months.
    Usage,
    /// Write a finished attestation as a self-contained bundle.
    Export {
        /// Execution ID.
        execution: String,
        /// Where to write it; must not exist.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Verify a bundle without the daemon: signature, certified receipt,
    /// their binding, the file, and the device's present status.
    Verify {
        /// The bundle.
        bundle: PathBuf,
        /// The original file of a digest or file statement.
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum ConnectCommand {
    /// Browsers paired with the account.
    #[command(subcommand)]
    Browsers(BrowsersCommand),
    /// Web requests in the approval center.
    Requests,
    /// Fetch and check web requests now.
    Sync,
}

#[derive(Subcommand)]
pub enum BrowsersCommand {
    /// Pair a browser: open the pairing page and confirm its name here.
    Add {
        /// Print the link instead of opening the default browser.
        #[arg(long)]
        copy_link: bool,
    },
    /// Paired browsers.
    List,
    /// Revoke a pairing.
    Revoke {
        /// Pairing ID.
        pairing: String,
    },
}

/// A request rendered for review.
fn render(request: &Value) -> String {
    let field = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => "-".into(),
        other => other.to_string(),
    };
    let this_device = |d: &Value| match d["this_device"] == true {
        true => tr!(" (this device)", "（本设备）"),
        false => String::new(),
    };
    let d = &request["detail"];
    let mut lines = vec![
        row("request", "请求", field(&request["id"])),
        row("from", "来源", field(&request["source"])),
        row("state", "状态", field(&request["state"])),
        row("expires", "期限", field(&request["expires_at"])),
    ];
    match request["type"].as_str() {
        Some("attest") => {
            let s = &d["statement"];
            let (profile, issuer) = (field(&s["profile"]), field(&s["issuer"]));
            lines.push(row(
                "sign",
                "签署",
                tr!(
                    "{profile} statement as {issuer}",
                    "{profile} 声明，签发者 {issuer}"
                ),
            ));
            lines.push(row("origin", "网站", field(&d["origin"])));
            if !s["subject"].is_null() {
                lines.push(row("subject", "主题", field(&s["subject"])));
            }
            for name in [
                "text",
                "sha256",
                "content_type",
                "location",
                "app_id",
                "command",
                "args",
                "operation_id",
            ] {
                if !s["content"][name].is_null() {
                    lines.push(format!("{name:<9} {}", field(&s["content"][name])));
                }
            }
            let file = match (d["file"].as_str(), d["file_checked"].as_bool()) {
                (Some(path), Some(true)) => tr!("{path} (digest matches)", "{path}（摘要一致）"),
                (Some(path), _) => tr!(
                    "{path} (CHANGED or unreadable)",
                    "{path}（已变化或无法读取）"
                ),
                (None, _) if !s["content"]["sha256"].is_null() => tr!(
                    "only the digest is confirmed; the file was not checked",
                    "仅确认摘要，未核对原文件"
                ),
                _ => String::new(),
            };
            if !file.is_empty() {
                lines.push(row("file", "文件", file));
            }
        }
        Some("device_revoke") => {
            let (device, mark) = (field(&d["device_id"]), this_device(d));
            lines.push(row(
                "revoke",
                "撤销",
                tr!(
                    "device {device}{mark}; the content root is rotated without it",
                    "设备 {device}{mark}；随后换根，它不再能读取",
                ),
            ));
        }
        Some("device_capabilities") => {
            let (device, mark) = (field(&d["device_id"]), this_device(d));
            lines.push(row(
                "change",
                "变更",
                tr!(
                    "capabilities of device {device}{mark}",
                    "设备 {device}{mark} 的能力"
                ),
            ));
            lines.push(row("to", "改为", field(&d["capabilities"])));
        }
        Some("principal") => {
            let change = field(&d["change"]);
            lines.push(row(
                "principal",
                "主体",
                tr!(
                    "{change} — an account change, published by the user home",
                    "{change}：账户变更，由 user home 发布",
                ),
            ));
            for name in [
                "principal_type",
                "generation",
                "name",
                "delegation",
                "invalid_from",
            ] {
                if !d[name].is_null() {
                    lines.push(format!(
                        "{:<9} {}",
                        name.replace("principal_", ""),
                        field(&d[name])
                    ));
                }
            }
        }
        Some("wallet") => {
            let name = format!("{} {}", field(&d["name"]), field(&d["wallet"]));
            match d["action"].as_str() {
                Some("create") => {
                    lines.push(row(
                        "create",
                        "创建",
                        tr!(
                            "team wallet {} for {} e8s ICP",
                            "团队钱包 {}，费用 {} e8s ICP",
                            field(&d["name"]),
                            field(&d["price_e8s"])
                        ),
                    ));
                    lines.push(row("quote", "报价", field(&d["quote_id"])));
                    for m in d["members"].as_array().into_iter().flatten() {
                        lines.push(row(
                            "member",
                            "成员",
                            format!("{} {}", field(&m["id"]), field(&m["role"])),
                        ));
                    }
                }
                Some("join") => lines.push(row("join", "加入", name)),
                Some(action) => {
                    let p = &d["proposal"];
                    let intent = &p["intent"];
                    lines.push(row("wallet", "钱包", name));
                    lines.push(row(
                        if action == "cosign" {
                            "cosign"
                        } else {
                            "decide"
                        },
                        if action == "cosign" {
                            "协签"
                        } else {
                            "决定"
                        },
                        format!(
                            "{} #{} {}",
                            field(&d["decision"]),
                            field(&p["id"]),
                            field(&intent["kind"])
                        ),
                    ));
                    if !intent["asset"].is_null() {
                        lines.push(row("asset", "资产", field(&intent["asset"]["asset"])));
                    }
                    for o in intent["outputs"].as_array().into_iter().flatten() {
                        lines.push(row(
                            "pay",
                            "支付",
                            format!("{} → {}", field(&o["amount"]), field(&o["to"])),
                        ));
                    }
                    if !d["plan"].is_null() {
                        let plan = &d["plan"];
                        lines.push(row("chain", "链", field(&plan["chain"])));
                        lines.push(row("fee", "手续费", field(&plan["fee"])));
                    }
                    lines.push(row("digest", "摘要", field(&p["digest"])));
                }
                None => {}
            }
        }
        Some("delegation") => {
            let k = &d["kind"];
            let (kind, generation) = (field(&k["type"]), field(&d["generation"]));
            lines.push(row(
                "sign",
                "签署",
                tr!(
                    "delegation.{kind} with controller #{generation}",
                    "用 controller #{generation} 签署 delegation.{kind}",
                ),
            ));
            for name in [
                "id",
                "subject",
                "scopes",
                "audiences",
                "expires_at",
                "relationship",
                "reason",
            ] {
                if !k[name].is_null() {
                    lines.push(format!("{name:<9} {}", field(&k[name])));
                }
            }
            lines.push(row(
                "",
                "",
                tr!(
                    "relying parties may act on a grant before its revocation",
                    "撤销生效前，依赖方仍可能依据授权行事",
                ),
            ));
        }
        Some("connect") => {
            let (app, version) = (field(&d["app_id"]), field(&d["app_version"]));
            lines.push(row(
                "web app",
                "网页应用",
                tr!("{app} (version {version})", "{app}（版本 {version}）"),
            ));
            lines.push(row("origin", "网站", field(&d["origin"])));
            lines.push(row(
                "",
                "",
                tr!(
                    "the origin is relayed by your browser through connect.dmsg.net",
                    "来源由浏览器经 connect.dmsg.net 转述",
                ),
            ));
            lines.push(row("browser", "浏览器", field(&d["pairing_label"])));
            lines.push(row("account", "账户", field(&d["account"])));
            lines.push(row(
                "method",
                "方法",
                format!("{} ({})", field(&d["method"]), field(&d["kind"])),
            ));
            lines.push(row("requested", "发起时间", field(&d["requested_at"])));
            lines.push(row("content", "内容", pretty(&d["content"])));
            if d["method"] == "checkout" {
                lines.push(row(
                    "terms",
                    "条款",
                    match d["checkout"].is_null() {
                        true => tr!(
                            "not quoted: run dmsg approvals quote <id> <ledger or neuron>",
                            "尚未报价：运行 dmsg approvals quote <id> <账本或神经元>",
                        ),
                        false => pretty(&d["checkout"]),
                    },
                ));
            }
        }
        Some("account") => {
            let principal = field(&d["principal"]);
            let change = match d["change"].as_str() {
                Some("bind_login") => tr!(
                    "bind login {principal} to the account; it signs in to every device's data",
                    "把登录 {principal} 绑定到账户；它可以登录并读取所有设备的数据",
                ),
                Some("remove_login") => tr!("remove login {principal}", "移除登录 {principal}"),
                Some("policy") => {
                    let (frozen, daily, purposes) = (
                        field(&d["frozen"]),
                        field(&d["daily_executions"]),
                        field(&d["allowed_purposes"]),
                    );
                    tr!(
                        "signing policy: frozen {frozen}, {daily} executions a day, purposes {purposes}",
                        "签名策略：冻结 {frozen}，每天 {daily} 次执行，用途 {purposes}",
                    )
                }
                Some("recovery_delay") => {
                    let days = d["delay_ms"].as_u64().unwrap_or(0) / 86_400_000;
                    tr!("recovery delay {days} days", "恢复等待期 {days} 天")
                }
                Some("dispute_recovery") => {
                    let op = field(&d["op_id"]);
                    tr!("cancel the pending recovery {op}", "取消待生效的恢复 {op}")
                }
                _ => field(&d["change"]),
            };
            lines.push(row("change", "变更", change));
        }
        _ => lines.push(row("detail", "详情", pretty(d))),
    }
    if !request["error"].is_null() {
        lines.push(row("error", "错误", field(&request["error"])));
    }
    lines.join("\n")
}

/// Show a request and let the person at the terminal decide it; a
/// rejection asks nothing more.
async fn decide(remote: &Remote, request: &Value, reject: bool) -> Result<Value, Failure> {
    println!("{}", render(request));
    let approve = match reject {
        true => false,
        false => match ask(&tr!(
            "\nApprove this request? Type yes to approve, no to reject: ",
            "\n批准这个请求？输入 yes 批准，no 拒绝："
        ))
        .as_str()
        {
            "yes" => true,
            "no" => false,
            _ => {
                return Err(Failure::new(
                    "CANCELLED",
                    "no decision; the request still waits",
                ))
            }
        },
    };
    remote
        .call(
            "approval/respond",
            json!({"id": request["id"], "digest": request["digest"], "approve": approve, "human": true}),
        )
        .await
}

/// A request just created: decided here at an interactive terminal, else
/// left waiting in the approval center.
pub(crate) async fn created(remote: &Remote, json: bool, request: Value) -> Result<(), Failure> {
    if !interactive(json) {
        print(json, &request, |r| {
            let id = r["id"].as_str().unwrap_or_default();
            tr!(
                "request {id} waits in the approval center: dmsg approvals approve {id}",
                "请求 {id} 在审批中心等待确认：dmsg approvals approve {id}"
            )
        });
        return Ok(());
    }
    let decided = decide(remote, &request, false).await?;
    println!(
        "{}{}",
        decided["state"].as_str().unwrap_or_default(),
        decided["execution"]
            .as_str()
            .map(|e| tr!("; execution {e}", "；执行 {e}"))
            .unwrap_or_default()
    );
    Ok(())
}

/// Call a method that creates an approval request from this terminal, then
/// handle the request as [`created`] does.
pub(crate) async fn request(
    remote: &Remote,
    json: bool,
    method: &str,
    mut params: Value,
) -> Result<(), Failure> {
    params["source"] = "cli".into();
    let request = remote.call(method, params).await?;
    created(remote, json, request).await
}

fn approval_lines(list: &Value) -> String {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            format!(
                "{} {:<13} {:<20} {}",
                short(r["id"].as_str().unwrap_or_default(), 12),
                r["state"].as_str().unwrap_or_default(),
                r["type"].as_str().unwrap_or_default(),
                r["source"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `dmsg approvals`. Requests are named by a unique prefix of their ID,
/// which the daemon resolves.
pub async fn approvals(
    remote: &Remote,
    json: bool,
    command: ApprovalsCommand,
) -> Result<(), Failure> {
    match command {
        ApprovalsCommand::List { all } => {
            let list = remote.call("approval/list", json!({"all": all})).await?;
            print(json, &list, approval_lines);
        }
        ApprovalsCommand::Show { id } => {
            let request = remote.call("approval/show", json!({"id": id})).await?;
            print(json, &request, render);
        }
        ApprovalsCommand::Approve { .. } | ApprovalsCommand::Reject { .. }
            if !interactive(json) =>
        {
            return Err(Failure::new(
                "FORBIDDEN",
                "approvals are decided by a person: run this in an interactive terminal without --json, or use the desktop approval window",
            ));
        }
        ApprovalsCommand::Approve { id } => {
            let request = remote.call("approval/show", json!({"id": id})).await?;
            let decided = decide(remote, &request, false).await?;
            println!("{}", decided["state"].as_str().unwrap_or_default());
        }
        ApprovalsCommand::Reject { id } => {
            let request = remote.call("approval/show", json!({"id": id})).await?;
            let decided = decide(remote, &request, true).await?;
            println!("{}", decided["state"].as_str().unwrap_or_default());
        }
        ApprovalsCommand::Cancel { id } => {
            let request = remote.call("approval/cancel", json!({"id": id})).await?;
            print(json, &request, |r| {
                r["state"].as_str().unwrap_or_default().to_string()
            });
        }
        ApprovalsCommand::Quote { id, selection } => {
            let request = remote
                .call("approval/quote", json!({"id": id, "selection": selection}))
                .await?;
            print(json, &request, render);
        }
    }
    Ok(())
}

/// `dmsg attest`, except `verify`, which needs no daemon ([`verify`]).
pub async fn attest(remote: &Remote, json: bool, command: AttestCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        AttestCommand::Text { text, subject } => (
            "attest/request",
            json!({"statement": {"profile": "text", "text": text, "subject": subject}}),
        ),
        AttestCommand::Digest {
            file,
            sha256,
            fields,
        } => (
            "attest/request",
            json!({
                "statement": {
                    "profile": "digest", "sha256": sha256,
                    "content_type": fields.content_type, "location": fields.location,
                    "subject": fields.subject,
                },
                "file": file.map(absolute).transpose()?,
            }),
        ),
        AttestCommand::File { path, text, fields } => (
            "attest/request",
            json!({
                "statement": {
                    "profile": "file_statement", "text": text,
                    "content_type": fields.content_type, "location": fields.location,
                    "subject": fields.subject,
                },
                "file": absolute(path)?,
            }),
        ),
        AttestCommand::Action { path } => {
            let bytes = match path.as_os_str() == "-" {
                true => {
                    let mut bytes = Vec::new();
                    std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes).map(|_| bytes)
                }
                false => std::fs::read(&path),
            }
            .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
            (
                "attest/action",
                json!({"action": dmsg_client::codec::b64(&bytes)}),
            )
        }
        AttestCommand::History => {
            let list = remote.call("attest/history", Value::Null).await?;
            print(json, &list, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| {
                        format!(
                            "{} {:<9} {} {}",
                            a["execution"].as_str().unwrap_or_default(),
                            a["stage"].as_str().unwrap_or_default(),
                            a["statement"]["profile"].as_str().unwrap_or_default(),
                            a["origin"].as_str().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            return Ok(());
        }
        AttestCommand::Usage => {
            let usage = remote.call("attest/usage", Value::Null).await?;
            print(json, &usage, pretty);
            return Ok(());
        }
        AttestCommand::Export { execution, out } => {
            let bundle = remote
                .call("attest/export", json!({"execution": execution}))
                .await?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&out)
                .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
            std::io::Write::write_all(&mut file, pretty(&bundle).as_bytes())
                .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?;
            print(json, &json!({"path": out.display().to_string()}), |v| {
                let path = v["path"].as_str().unwrap_or_default();
                tr!("wrote {path}", "已写入 {path}")
            });
            return Ok(());
        }
        AttestCommand::Verify { .. } => unreachable!("verified without the daemon"),
    };
    request(remote, json, method, params).await
}

/// `dmsg attest verify`: a bundle against the deployment's user homes,
/// without the daemon.
pub async fn verify(
    home: &Home,
    json: bool,
    bundle: &Path,
    file: Option<&Path>,
) -> Result<(), Failure> {
    let settings = home
        .settings()
        .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
    let text = std::fs::read(bundle).map_err(|e| Failure::new("NOT_FOUND", e.to_string()))?;
    let bundle: dmsg_client::attest::Bundle =
        serde_json::from_slice(&text).map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
    let failure = |e: dmsg_types::Error| Failure::new(&dmsg_client::error_code(&e), e.to_string());
    let config = &settings.config;
    let ic = dmsg_client::ic::Ic::anonymous(config)
        .await
        .map_err(failure)?;
    // Only the deployment's user homes certify receipts for its accounts.
    let homes = dmsg_client::account::user_homes(&ic, config.handle, &config.user_homes)
        .await
        .map_err(failure)?
        .user_homes;
    let report = dmsg_client::attest::verify_online(&ic, &homes, &bundle, file)
        .await
        .map_err(failure)?;
    let report = serde_json::to_value(&report).unwrap_or_default();
    print(json, &report, |r| {
        let text = |name: &str| r[name].as_str().unwrap_or_default().to_string();
        let (key, account, device) = (text("key_fingerprint"), text("account"), text("device_id"));
        let (at, epoch) = (&r["approved_at"], &r["security_epoch"]);
        [
            row("content", "内容", text("content")),
            row(
                "signature",
                "签名",
                tr!("valid (key {key})", "有效（密钥 {key}）"),
            ),
            row(
                "approved",
                "批准",
                tr!(
                    "by account {account} with device {device} at {at} (epoch {epoch})",
                    "账户 {account} 的设备 {device}，时间 {at}（安全纪元 {epoch}）"
                ),
            ),
            row("origin", "网站", text("origin")),
            row("certified", "认证时间", &r["certified_at"]),
            row("device", "设备现状", text("current")),
            row("statement", "声明", pretty(&r["statement"])),
        ]
        .join("\n")
    });
    Ok(())
}

/// `dmsg connect`.
pub async fn connect(remote: &Remote, json: bool, command: ConnectCommand) -> Result<(), Failure> {
    match command {
        ConnectCommand::Browsers(BrowsersCommand::Add { copy_link }) => {
            pair(remote, json, copy_link).await?
        }
        ConnectCommand::Browsers(BrowsersCommand::List) => {
            let list = remote.call("connect/pairings", Value::Null).await?;
            print(json, &list, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| {
                        format!(
                            "{} {:<8} {} (by {})",
                            p["pairing_id"].as_str().unwrap_or_default(),
                            p["state"].as_str().unwrap_or_default(),
                            p["label"].as_str().unwrap_or("-"),
                            short(p["issued_by"].as_str().unwrap_or("-"), 12)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        ConnectCommand::Browsers(BrowsersCommand::Revoke { pairing }) => {
            remote
                .call("connect/revoke", json!({"pairing": pairing}))
                .await?;
            print(json, &json!({"revoked": pairing}), |_| {
                tr!("revoked", "已撤销")
            });
        }
        ConnectCommand::Requests => {
            let list = remote.call("approval/list", json!({"all": true})).await?;
            let web: Vec<Value> = list
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["type"] == "connect")
                .cloned()
                .collect();
            print(json, &Value::Array(web), approval_lines);
        }
        ConnectCommand::Sync => {
            let report = remote.call("connect/sync", Value::Null).await?;
            print(json, &report, |r| {
                let (fetched, queued, refused) = (&r["fetched"], &r["queued"], &r["refused"]);
                let (replied, sealed) = (&r["replied"], &r["sealed"]);
                tr!(
                    "fetched {fetched}, queued {queued}, refused {refused}, replied {replied}, sealed {sealed}",
                    "取回 {fetched}，排队 {queued}，拒绝 {refused}，已答复 {replied}，封存 {sealed}"
                )
            });
        }
    }
    Ok(())
}

/// Pair a browser: open the pairing page, then confirm here the name each
/// browser that answered shows.
async fn pair(remote: &Remote, json: bool, copy_link: bool) -> Result<(), Failure> {
    if !interactive(json) {
        return Err(Failure::new(
            "FORBIDDEN",
            "pairing a browser needs a person to confirm its name: run this in an interactive terminal",
        ));
    }
    let invite = remote.call("connect/invite", Value::Null).await?;
    let url = invite["url"].as_str().unwrap_or_default();
    if copy_link || !open_browser(url) {
        println!(
            "{}",
            tr!(
                "Open this link in the browser to connect (valid ten minutes):\n{url}",
                "在要连接的浏览器中打开这个链接（十分钟内有效）：\n{url}"
            )
        );
    } else {
        println!(
            "{}",
            tr!(
                "Opened the pairing page in your browser.",
                "已在浏览器中打开配对页。"
            )
        );
    }
    let invite_id = invite["invite_id"].as_str().unwrap_or_default();
    let deadline = invite["expires_at"].as_u64().unwrap_or_default();
    let mut seen = std::collections::BTreeSet::new();
    while dmsg_client::now_ms() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let found = remote
            .call("connect/candidates", json!({"invite": invite_id}))
            .await?;
        for candidate in found["candidates"].as_array().into_iter().flatten() {
            let pairing = candidate["pairing_id"].as_str().unwrap_or_default();
            if !seen.insert(pairing.to_string()) {
                continue;
            }
            let label = candidate["label"].as_str().unwrap_or_default();
            let short = short(pairing, 12);
            let answer = ask(&tr!(
                "Connect the browser \"{label}\" ({short})? Type yes to connect: ",
                "连接浏览器“{label}”（{short}）？输入 yes 连接："
            ));
            if answer == "yes" {
                let granted = remote
                    .call(
                        "connect/grant",
                        json!({"invite": invite_id, "pairing": pairing, "human": true}),
                    )
                    .await?;
                let (label, until) = (
                    granted["label"].as_str().unwrap_or_default(),
                    &granted["expires_at"],
                );
                println!(
                    "{}",
                    tr!(
                        "connected {label} until {until}",
                        "已连接 {label}，有效至 {until}"
                    )
                );
                return Ok(());
            }
        }
    }
    Err(Failure::new("EXPIRED", "the pairing link expired"))
}
