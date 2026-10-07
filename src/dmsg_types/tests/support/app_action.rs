#![allow(dead_code)]
use candid::Principal;
use dmsg_types::{app_action::*, integration::*, AccountId, Environment, Hash};

/// Registration of the sample application that signs actions.
pub fn app() -> AppRegistration {
    AppRegistration {
        version: INTEGRATION_VERSION,
        environment: Environment::Local,
        app_id: "tokenlist".into(),
        config_version: 1,
        origins: vec!["http://localhost:5188".into()],
        product_ids: vec![],
        capabilities: vec![AppCapability::Authenticate, AppCapability::SignAction],
        profiles: vec![SigningProfile::AppActionV1],
        authentication_receiver: Principal::from_slice(&[3, 1]),
        action_authority: Principal::from_slice(&[3, 1]),
        action_schema: Some(schema()),
        paused: false,
    }
}

fn label(en: &str, zh: &str) -> Vec<ActionLabel> {
    vec![
        ActionLabel {
            locale: "en".into(),
            text: en.into(),
        },
        ActionLabel {
            locale: "zh".into(),
            text: zh.into(),
        },
    ]
}

fn field(name: &str, en: &str, zh: &str, ty: FieldType) -> FieldSchema {
    FieldSchema {
        name: name.into(),
        label: label(en, zh),
        ty,
    }
}

fn id(name: &str, en: &str, zh: &str) -> FieldSchema {
    field(
        name,
        en,
        zh,
        FieldType::Nat {
            min: 1,
            max: u64::MAX,
        },
    )
}

fn text_field(name: &str, en: &str, zh: &str, max_bytes: u64, multiline: bool) -> FieldSchema {
    field(
        name,
        en,
        zh,
        FieldType::Text {
            max_bytes,
            multiline,
        },
    )
}

fn command(name: &str, en: &str, zh: &str, fields: Vec<FieldSchema>) -> CommandSchema {
    CommandSchema {
        name: name.into(),
        title: label(en, zh),
        fields: SchemaFields(fields),
    }
}

/// Review workflow sample, shaped like the first registered integration.
pub fn schema() -> ActionSchema {
    let project = || id("project_id", "Project", "项目");
    let transition = || id("transition_id", "Transition", "过渡");
    let statement = || {
        field(
            "statement_hash",
            "Statement digest",
            "声明摘要",
            FieldType::Hash,
        )
    };
    let rationale = || text_field("rationale", "Rationale", "理由", 4096, true);
    let choice = |value: &str, en: &str, zh: &str| ChoiceOption {
        value: value.into(),
        label: label(en, zh),
    };
    ActionSchema {
        version: 1,
        commands: vec![
            command(
                "CertifyDisclosure",
                "Certify disclosure draft",
                "认证披露稿",
                vec![
                    project(),
                    id("contract_id", "Contract", "合同"),
                    id("revision", "Draft revision", "稿件版本"),
                ],
            ),
            command(
                "DecideReview",
                "Record review decision",
                "记录审阅决定",
                vec![
                    project(),
                    id("case_id", "Review case", "审阅"),
                    field(
                        "round",
                        "Round",
                        "轮次",
                        FieldType::Nat {
                            min: 1,
                            max: u32::MAX.into(),
                        },
                    ),
                    field(
                        "outcome",
                        "Decision",
                        "决定",
                        FieldType::Choice {
                            options: vec![
                                choice("Approved", "Approve", "批准"),
                                choice("Rejected", "Reject", "拒绝"),
                                choice("ChangesRequested", "Request changes", "要求修改"),
                            ],
                        },
                    ),
                    field(
                        "changes",
                        "Requested changes",
                        "修改要求",
                        FieldType::List {
                            item: Box::new(FieldType::Record {
                                fields: SchemaFields(vec![
                                    text_field("locator", "Field", "字段", 128, false),
                                    text_field("detail", "Change", "修改内容", 4096, true),
                                    field(
                                        "blocking",
                                        "Blocking",
                                        "是否阻断",
                                        FieldType::Bool {
                                            yes: label("Must be resolved", "必须解决"),
                                            no: label("Optional", "可选"),
                                        },
                                    ),
                                ]),
                            }),
                            max_items: 64,
                        },
                    ),
                    rationale(),
                ],
            ),
            command(
                "CertifyTransition",
                "Certify transition analysis",
                "认证过渡分析",
                vec![
                    project(),
                    transition(),
                    statement(),
                    rationale(),
                    field(
                        "analysis",
                        "Analysis file",
                        "分析文件",
                        FieldType::Optional {
                            item: Box::new(FieldType::Artifact),
                        },
                    ),
                ],
            ),
            command(
                "ApproveTransition",
                "Approve or refuse transition",
                "批准或拒绝过渡",
                vec![
                    project(),
                    transition(),
                    field(
                        "approve",
                        "Decision",
                        "决定",
                        FieldType::Bool {
                            yes: label("Approve", "批准"),
                            no: label("Refuse", "拒绝"),
                        },
                    ),
                    statement(),
                    rationale(),
                ],
            ),
        ],
    }
}

fn arg(name: &str, value: ActionValue) -> ActionArg {
    ActionArg {
        name: name.into(),
        value,
    }
}

fn text(value: &str) -> ActionValue {
    ActionValue::Text(value.into())
}

pub fn action() -> AppAction {
    AppAction {
        version: 1,
        environment: Environment::Local,
        app_id: "tokenlist".into(),
        app_config_version: 1,
        origin: "http://localhost:5188".into(),
        receiver: Principal::from_slice(&[7, 1]),
        actor: vec![8; 12].into(),
        signing_account: AccountId([3; 12]),
        operation_id: Hash::new([1; 32]),
        intent_hash: Hash::new([2; 32]),
        schema_hash: dmsg_protocol::app_action::action_schema_hash(&schema()),
        issued_at_ms: 1_800_000_000_000,
        expires_at_ms: 1_800_000_300_000,
        command: commands().remove(0),
        files: vec![ActionFile {
            file_id: "disclosure/draft/9".into(),
            revision: 3,
            sha256: Hash::new([9; 32]),
            byte_length: 412,
            media_type: "application/cbor".into(),
            display_name: Some("披露文件.cbor".into()),
            representation: ActionFileRepresentation::Original,
        }],
    }
}

/// One command of each sample schema entry, covering every value kind but Principal.
pub fn commands() -> Vec<ActionCommand> {
    use ActionValue as V;
    vec![
        ActionCommand {
            name: "CertifyDisclosure".into(),
            args: ActionArgs(vec![
                arg("project_id", V::Nat(42)),
                arg("contract_id", V::Nat(9)),
                arg("revision", V::Nat(3)),
            ]),
        },
        ActionCommand {
            name: "DecideReview".into(),
            args: ActionArgs(vec![
                arg("project_id", V::Nat(42)),
                arg("case_id", V::Nat(5)),
                arg("round", V::Nat(2)),
                arg("outcome", V::Choice("ChangesRequested".into())),
                arg(
                    "changes",
                    V::List(vec![V::Record(ActionArgs(vec![
                        arg("locator", text("07/total_supply")),
                        arg("detail", text("Explain the complete supply allocation")),
                        arg("blocking", V::Bool(true)),
                    ]))]),
                ),
                arg("rationale", text("The allocation differs from the ledger.")),
            ]),
        },
        ActionCommand {
            name: "CertifyTransition".into(),
            args: ActionArgs(vec![
                arg("project_id", V::Nat(42)),
                arg("transition_id", V::Nat(8)),
                arg("statement_hash", V::Hash(Hash::new([10; 32]))),
                arg(
                    "rationale",
                    text("The published transition analysis is complete."),
                ),
                arg(
                    "analysis",
                    V::Artifact(ActionArtifact {
                        uri: "https://example.test/analysis.pdf".into(),
                        sha256: Hash::new([11; 32]),
                        content_type: "application/pdf".into(),
                        size: 112,
                    }),
                ),
            ]),
        },
        ActionCommand {
            name: "ApproveTransition".into(),
            args: ActionArgs(vec![
                arg("project_id", V::Nat(42)),
                arg("transition_id", V::Nat(8)),
                arg("approve", V::Bool(false)),
                arg("statement_hash", V::Hash(Hash::new([12; 32]))),
                arg(
                    "rationale",
                    text("Refused because obligations remain outstanding."),
                ),
            ]),
        },
    ]
}
