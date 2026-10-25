//! CLI commands of the approval center, formal statements and web
//! applications through dmsg-connect/1. A request is rendered for review
//! here, and only a person at an interactive terminal decides it.

use crate::home::Home;
use crate::locale::{row, tr};
use crate::remote::{Failure, Remote};
use crate::{absolute, ask, interactive, open_browser, pretty, print, short};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
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
    /// Compare a file with the digest a request asks to sign, such as a
    /// page's document; what is approved does not change.
    CheckFile {
        /// Request ID or a unique prefix.
        id: String,
        /// The file.
        file: PathBuf,
    },
    /// Settle approved requests whose outcome is not known yet.
    Reconcile,
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
    /// Finished and pending attestations, the account's other devices' included.
    History,
    /// This month's formal-execution allowance and the archived months.
    Usage,
    /// Whether this device can sign now, and what stops it.
    Readiness,
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

/// A label in the CLI's language, else English, else any.
fn pick(labels: &Value) -> String {
    let want = if crate::locale::zh() { "zh" } else { "en" };
    let map = labels.as_object();
    map.and_then(|m| {
        m.iter()
            .find(|(locale, _)| locale.starts_with(want))
            .or_else(|| m.iter().find(|(locale, _)| locale.starts_with("en")))
            .or_else(|| m.iter().next())
    })
    .and_then(|(_, text)| text.as_str())
    .unwrap_or_default()
    .to_string()
}

/// A value of an action rendered by its schema.
fn action_value(value: &Value) -> String {
    match value["type"].as_str() {
        Some("bool") => pick(&value["label"]),
        Some("choice") if value["label"].is_object() => pick(&value["label"]),
        Some("null") => "-".into(),
        Some("artifact") => format!(
            "{} (sha256 {}, {} bytes)",
            value["uri"].as_str().unwrap_or_default(),
            value["sha256"].as_str().unwrap_or_default(),
            value["size"]
        ),
        Some("list") => value["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(action_value)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        Some("record") => value["fields"]
            .as_array()
            .map(|fields| {
                fields
                    .iter()
                    .map(|f| format!("{}: {}", pick(&f["label"]), action_value(&f["value"])))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default(),
        _ => value["text"].as_str().unwrap_or_default().to_string(),
    }
}

/// An application action as its registered schema renders it: the title,
/// then each labelled field, then the files.
fn action_lines(rendered: &Value) -> Vec<String> {
    let mut lines = vec![row("command", "命令", pick(&rendered["title"]))];
    for field in rendered["fields"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  {}: {}",
            pick(&field["label"]),
            action_value(&field["value"])
        ));
    }
    for file in rendered["files"].as_array().into_iter().flatten() {
        lines.push(row(
            "file",
            "文件",
            format!(
                "{} r{} sha256 {}",
                file["name"].as_str().unwrap_or_default(),
                file["revision"],
                file["sha256"].as_str().unwrap_or_default()
            ),
        ));
    }
    lines.push(tr!(
        "The application defined this wording; governance registered it.",
        "以上措辞由应用定义，经治理登记。"
    ));
    lines
}

/// What is known of the file behind a digest: only the digest, or the file
/// the person compared.
fn file_check(request: &Value) -> String {
    let check = &request["file_check"];
    match (check["name"].as_str(), check["matches"].as_bool()) {
        (Some(name), Some(true)) => tr!("{name} matches the digest", "{name} 与摘要一致"),
        (Some(name), _) => tr!("{name} does NOT match the digest", "{name} 与摘要不一致"),
        _ => tr!(
            "only the digest is confirmed; the file was not checked",
            "仅确认摘要，未核对原文件"
        ),
    }
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
    if request["risk"] == "high" {
        lines.push(row(
            "risk",
            "风险",
            tr!(
                "high: confirmed with Touch ID or the login password where this Mac can ask",
                "高：本机支持时须以 Touch ID 或登录密码确认"
            ),
        ));
    }
    if !request["error"].is_null() {
        lines.push(row("error", "错误", field(&request["error"])));
    }
    if !request["decided_at"].is_null() {
        let (at, via) = (
            field(&request["decided_at"]),
            field(&request["decided_via"]),
        );
        lines.push(row(
            "decided",
            "决定",
            tr!("{at} via {via}", "{at}，经 {via}"),
        ));
    }
    match request["type"].as_str() {
        Some("secret_ssh" | "secret_ssh_binding") => {
            lines.push(row("SSH", "SSH", "Original connection only; Git verifies namespace and digest, not commit contents. 主机指纹不证明 DNS 主机名。"));
            lines.push(row("request", "请求", pretty(d)));
        }
        Some("secret_process") => {
            lines.push(row("delivery", "交付", "Program and descendants; executable digest does not verify scripts/dependencies. 已交付的值无法收回。"));
            lines.push(row("program", "程序与引用", pretty(d)));
        }
        Some("secret_use") => {
            // Escape terminal controls while preserving the full business text.
            let field = |value: &Value| {
                value
                    .as_str()
                    .map(|s| {
                        s.chars()
                            .flat_map(|c| {
                                if c.is_control() && c != '\n' && c != '\t' {
                                    c.escape_default().collect::<Vec<_>>()
                                } else {
                                    vec![c]
                                }
                            })
                            .collect::<String>()
                    })
                    .unwrap_or_else(|| value.to_string())
            };
            lines.push(row(
                "target",
                "目标",
                format!(
                    "{}/repos/{}/{}/issues",
                    field(&d["target"]["api_origin"]),
                    field(&d["target"]["owner"]),
                    field(&d["target"]["repo"])
                ),
            ));
            lines.push(row("Agent ID", "Agent ID", field(&d["actor"])));
            lines.push(row("policy", "策略", field(&d["policy_name"])));
            lines.push(row("credential", "凭据引用", pretty(&d["resolved"])));
            lines.push(row("title", "完整标题", field(&d["arguments"]["title"])));
            lines.push(row("body", "完整正文", field(&d["arguments"]["body"])));
            lines.push(row(
                "untrusted reason",
                "不可信自述理由",
                field(&d["reason"]),
            ));
            lines.push(tr!(
                "Publishes one issue and may notify others. Unknown outcomes are never retried.",
                "发布一次 issue，可能通知他人；结果不明时不会重发。"
            ));
        }
        Some("secret_policy") => {
            lines.push(pretty(d));
        }
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
            lines.push(row("purpose", "用途", field(&d["purpose"])));
            if !s["subject"].is_null() {
                lines.push(row("subject", "主题", field(&s["subject"])));
            }
            let rendered = &d["action"];
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
                let shown_by_schema = rendered.is_object() && matches!(name, "command" | "args");
                if !s["content"][name].is_null() && !shown_by_schema {
                    lines.push(format!("{name:<9} {}", field(&s["content"][name])));
                }
            }
            if rendered.is_object() {
                lines.extend(action_lines(rendered));
            }
            let file = &d["file"];
            let named = match (file["path"].as_str(), file["library"].is_object()) {
                (Some(path), _) => path.to_string(),
                (None, true) => tr!(
                    "{} in the file library",
                    "文件库中的 {}",
                    field(&file["name"])
                ),
                _ => String::new(),
            };
            let line = match (named.is_empty(), d["file_checked"].as_bool()) {
                (false, Some(true)) => tr!("{named} (digest matches)", "{named}（摘要一致）"),
                (false, _) => tr!(
                    "{named} (CHANGED or unreadable)",
                    "{named}（已变化或无法读取）"
                ),
                (true, _) if !s["content"]["sha256"].is_null() => file_check(request),
                _ => String::new(),
            };
            if !line.is_empty() {
                lines.push(row("file", "文件", line));
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
        Some("device_pair") => {
            let (device, role) = (field(&d["device_id"]), field(&d["role"]));
            lines.push(row(
                "add",
                "添加",
                tr!(
                    "device {device} as {role}; the content root is rotated to include it",
                    "设备 {device}，角色 {role}；随后换根，它可以读取",
                ),
            ));
            lines.push(row("rights", "能力", field(&d["capabilities"])));
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
            let rendered = &d["content"]["action"];
            match rendered.is_object() {
                true => lines.extend(action_lines(rendered)),
                false => lines.push(row("content", "内容", pretty(&d["content"]))),
            }
            if !d["content"]["statement"]["content"]["sha256"].is_null() {
                lines.push(row("file", "文件", file_check(request)));
            }
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
                ));
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
        ApprovalsCommand::CheckFile { id, file } => {
            let request = remote.call("approval/show", json!({"id": id})).await?;
            let checked = remote
                .call(
                    "approval/check_file",
                    json!({"id": request["id"], "path": absolute(&file)?}),
                )
                .await?;
            print(json, &checked, file_check);
        }
        ApprovalsCommand::Reconcile => {
            let list = remote.call("approval/reconcile", Value::Null).await?;
            print(json, &list, approval_lines);
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
                        let device = match a["this_device"] == true {
                            true => String::new(),
                            false => tr!(" (another device)", "（其他设备）"),
                        };
                        format!(
                            "{} {:<9} {} {}{device}",
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
        AttestCommand::Readiness => {
            let ready = remote.call("attest/readiness", Value::Null).await?;
            print(json, &ready, |r| {
                let reason = |code: &str| match code {
                    "AUTH_REQUIRED" => tr!("log in first (dmsg login)", "需要先登录（dmsg login）"),
                    "FORMAL_APPROVE" => tr!(
                        "this device cannot approve formal signatures: dmsg device formal-approve on",
                        "本设备不能批准正式签名：dmsg device formal-approve on"
                    ),
                    "FROZEN" => tr!(
                        "the account's signing policy is frozen",
                        "账户的签名策略已冻结"
                    ),
                    "QUOTA" => tr!(
                        "this month's allowance was used up when last read",
                        "上次读取时本月额度已用完"
                    ),
                    other => other.to_string(),
                };
                let mut lines = vec![match r["ready"] == true {
                    true => tr!("ready to sign", "可以签名"),
                    false => tr!("not ready to sign", "暂不能签名"),
                }];
                for code in r["blockers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .chain(r["warnings"].as_array().into_iter().flatten())
                {
                    lines.push(format!("- {}", reason(code.as_str().unwrap_or_default())));
                }
                lines.join("\n")
            });
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
    let verified = dmsg_client::attest::verify_online(&ic, &homes, &bundle, file)
        .await
        .map_err(failure)?;
    let report = serde_json::to_value(&verified).unwrap_or_default();
    print(json, &report, |r| {
        let text = |name: &str| r[name].as_str().unwrap_or("-").to_string();
        let check = |ok: bool| match ok {
            true => tr!("passed", "通过"),
            false => tr!("FAILED", "未通过"),
        };
        let content = match r["content"].as_str() {
            Some("matches_file") => tr!("matches the file", "与文件一致"),
            Some("mismatch") => tr!("does NOT match the file", "与文件不一致"),
            Some("digest_only") => tr!(
                "digest only; the file was not checked",
                "仅摘要，未核对原文件"
            ),
            Some("text") => tr!("a text statement", "文本声明"),
            Some("action") => tr!("an application action", "应用动作"),
            _ => tr!("unknown: the signature does not verify", "未知：签名未通过"),
        };
        let current = match r["current"].as_str() {
            Some("active") => tr!("active", "仍有效"),
            Some("revoked") => tr!(
                "revoked since; the signature stays valid",
                "之后已撤销；签名仍然有效"
            ),
            _ => tr!("unknown", "未知"),
        };
        let (key, account, device) = (text("key_fingerprint"), text("account"), text("device_id"));
        let (at, epoch) = (&r["approved_at"], &r["security_epoch"]);
        [
            row(
                "result",
                "结论",
                match r["valid"] == true {
                    true => tr!("valid", "有效"),
                    false => tr!("NOT valid", "无效"),
                },
            ),
            row("content", "内容", content),
            row(
                "signature",
                "签名",
                format!("{} ({key})", check(r["signature"] == true)),
            ),
            row(
                "binding",
                "绑定",
                format!(
                    "{}: {}",
                    check(r["binding"] == true),
                    tr!(
                        "account {account}, device {device}, epoch {epoch}, at {at}",
                        "账户 {account}，设备 {device}，安全纪元 {epoch}，时间 {at}"
                    )
                ),
            ),
            row(
                "evidence",
                "授权证据",
                format!(
                    "{} ({} {})",
                    check(r["evidence"] == true),
                    text("home"),
                    r["certified_at"]
                ),
            ),
            row("origin", "网站", text("origin")),
            row("device", "设备现状", current),
            row("statement", "声明", pretty(&r["statement"])),
        ]
        .join("\n")
    });
    // A bundle that does not verify fails the command, after its report.
    match verified.valid {
        true => Ok(()),
        false => Err(Failure::new(
            "INTEGRITY_FAILED",
            format!("failed: {}", verified.failed().join(", ")),
        )),
    }
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
