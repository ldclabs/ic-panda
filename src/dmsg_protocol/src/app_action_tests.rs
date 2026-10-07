use super::*;
use crate::{integration::validate_app, *};
use ed25519_dalek::{Signer, SigningKey};
#[path = "../../dmsg_types/tests/support/app_action.rs"]
mod fixture;

fn finish(tbs: &[u8], public: &[u8], signature: Vec<u8>) -> Result<SignedArtifact> {
    parse_signing_input(tbs)?
        .into_signature(public)?
        .finish(signature)
}

fn statement(action: AppAction) -> Statement {
    Statement {
        issuer: "https://example.test/u/00000000000000000000".into(),
        subject: None,
        issued_at: None,
        content: StatementContent::AppAction(Box::new(action)),
    }
}

fn arg(name: &str, value: ActionValue) -> ActionArg {
    ActionArg {
        name: name.into(),
        value,
    }
}

fn label(text: &str) -> Vec<ActionLabel> {
    vec![ActionLabel {
        locale: "en".into(),
        text: text.into(),
    }]
}

fn field(name: &str, ty: FieldType) -> FieldSchema {
    FieldSchema {
        name: name.into(),
        label: label(name),
        ty,
    }
}

fn one_command(fields: Vec<FieldSchema>) -> ActionSchema {
    ActionSchema {
        version: 1,
        commands: vec![CommandSchema {
            name: "Assign".into(),
            title: label("Assign"),
            fields: SchemaFields(fields),
        }],
    }
}

/// The fixture action carrying `command`, signed under `schema`.
fn with(schema: &ActionSchema, name: &str, args: Vec<ActionArg>) -> AppAction {
    let mut action = fixture::action();
    action.schema_hash = action_schema_hash(schema);
    action.command = ActionCommand {
        name: name.into(),
        args: ActionArgs(args),
    };
    action
}

#[test]
fn registered_actions_roundtrip_and_every_signed_field_is_bound() {
    let schema = fixture::schema();
    validate_action_schema(&schema).unwrap();
    let signer = SigningKey::from_bytes(&[71; 32]);
    let public = signer.verifying_key().to_bytes();
    let kid = key_thumbprint(&public_cose_key(&[], &public).unwrap()).unwrap();
    for command in fixture::commands() {
        let mut action = fixture::action();
        action.command = command;
        validate_app_action(&action).unwrap();
        validate_action_command(&action, &schema).unwrap();
        let original = statement(action.clone());
        let (_, tbs) = prepare_cose(&original, &kid[..]).unwrap();
        let sig = signer.sign(&tbs).to_bytes().to_vec();
        let artifact = finish(&tbs, &public, sig.clone()).unwrap();
        assert_eq!(verify_artifact(&artifact).unwrap(), original);
        assert_eq!(statement_purpose(&original), KeyPurpose::AppAction);
        for mutate in [
            |a: &mut AppAction| a.origin = "https://other.test".into(),
            |a: &mut AppAction| a.receiver = candid::Principal::from_slice(&[99, 1]),
            |a: &mut AppAction| a.files[0].display_name = Some("changed.cbor".into()),
            |a: &mut AppAction| a.files[0].sha256 = Hash::new([42; 32]),
            |a: &mut AppAction| a.files[0].revision += 1,
            |a: &mut AppAction| a.files[0].representation = ActionFileRepresentation::Encrypted,
            |a: &mut AppAction| a.expires_at_ms -= 1,
            |a: &mut AppAction| a.actor = vec![42; 12].into(),
            |a: &mut AppAction| a.schema_hash = Hash::new([42; 32]),
            |a: &mut AppAction| a.command = fixture::commands().pop().unwrap(),
            |a: &mut AppAction| a.command.args.0[0].value = ActionValue::Nat(43),
        ] {
            let mut changed = action.clone();
            mutate(&mut changed);
            if changed == action {
                continue;
            }
            let (_, changed_tbs) = prepare_cose(&statement(changed), &kid[..]).unwrap();
            let changed_artifact = finish(&changed_tbs, &public, sig.clone()).unwrap();
            assert!(verify_artifact(&changed_artifact).is_err());
        }
    }
}

#[test]
fn unknown_fields_values_and_ambiguous_claims_are_rejected() {
    let a = fixture::action();
    let mut value: cbor2::Value = cbor2::from_slice(&canonical(&a)).unwrap();
    if let cbor2::Value::Map(ref mut entries) = value {
        entries.push(("display_summary".into(), "approve everything".into()));
    }
    assert!(decode_canonical::<AppAction>(&canonical(&value)).is_err());
    // The value model has no opaque bytes.
    let mut value: cbor2::Value = cbor2::from_slice(&canonical(&a)).unwrap();
    if let cbor2::Value::Map(ref mut entries) = value {
        let command = entries
            .iter_mut()
            .find(|(k, _)| *k == cbor2::Value::from("command"))
            .unwrap();
        command.1 = cbor2::Value::Map(vec![
            ("name".into(), "Transfer".into()),
            (
                "args".into(),
                cbor2::Value::Array(vec![cbor2::Value::Map(vec![
                    ("name".into(), "payload".into()),
                    (
                        "value".into(),
                        cbor2::Value::Map(vec![("Bytes".into(), vec![1u8].into())]),
                    ),
                ])]),
            ),
        ]);
    }
    assert!(decode_canonical::<AppAction>(&canonical(&value)).is_err());
    let mut s = statement(a.clone());
    s.subject = Some("a contradictory subject".into());
    assert!(prepare_cose(&s, &[1]).is_err());
    s.subject = None;
    s.issued_at = Some(1);
    assert!(prepare_cose(&s, &[1]).is_err());
    let mut wrong = a.clone();
    wrong.version = 2;
    assert!(validate_app_action(&wrong).is_err());
    wrong = a.clone();
    wrong.files.push(wrong.files[0].clone());
    assert!(validate_app_action(&wrong).is_err());
    wrong = a.clone();
    wrong.expires_at_ms += 1;
    assert!(validate_app_action(&wrong).is_err());
    wrong = a.clone();
    wrong.actor = vec![0; 12].into();
    assert!(validate_app_action(&wrong).is_err());
    wrong = a.clone();
    wrong.command.name = "not a name".into();
    assert!(validate_app_action(&wrong).is_err());
    // Values deeper than any schema may declare fail without a schema.
    let deep = ActionValue::List(vec![ActionValue::List(vec![ActionValue::List(vec![
        ActionValue::Nat(1),
    ])])]);
    wrong = a;
    wrong.command.args.0[0].value = deep;
    assert!(validate_app_action(&wrong).is_err());
}

#[test]
fn commands_must_match_the_signed_schema_exactly() {
    let schema = fixture::schema();
    let review = fixture::commands().remove(1);
    let check = |command: ActionCommand| {
        let mut action = fixture::action();
        action.command = command;
        validate_action_command(&action, &schema)
    };
    check(review.clone()).unwrap();
    let mut action = fixture::action();
    action.schema_hash = Hash::new([42; 32]);
    assert_eq!(
        validate_action_command(&action, &schema),
        Err(Error::IntegrityFailed)
    );
    let mut c = review.clone();
    c.name = "TransferAssets".into();
    assert_eq!(check(c), Err(Error::UnsupportedProtocol));
    let invalid = |change: fn(&mut ActionCommand)| {
        let mut c = review.clone();
        change(&mut c);
        assert!(matches!(check(c), Err(Error::InvalidInput(_))));
    };
    invalid(|c| {
        c.args.0.pop();
    });
    invalid(|c| c.args.0.push(c.args.0[0].clone()));
    // Same-typed arguments cannot be swapped silently.
    invalid(|c| c.args.0.swap(1, 2));
    invalid(|c| c.args.0[1].name = "project_id".into());
    invalid(|c| c.args.0[0].value = ActionValue::Nat(0));
    invalid(|c| c.args.0[2].value = ActionValue::Nat(u64::from(u32::MAX) + 1));
    invalid(|c| c.args.0[0].value = ActionValue::Text("42".into()));
    invalid(|c| c.args.0[3].value = ActionValue::Choice("Escalated".into()));
    invalid(|c| c.args.0[3].value = ActionValue::Null);
    invalid(|c| c.args.0[5].value = ActionValue::Text(" \n".into()));
    invalid(|c| c.args.0[5].value = ActionValue::Text("x".repeat(4097)));
    invalid(|c| {
        let ActionValue::List(items) = &mut c.args.0[4].value else {
            unreachable!()
        };
        let ActionValue::Record(fields) = &mut items[0] else {
            unreachable!()
        };
        fields.0[0].value = ActionValue::Text("07/\ntotal_supply".into());
    });
    invalid(|c| {
        let ActionValue::List(items) = &mut c.args.0[4].value else {
            unreachable!()
        };
        *items = vec![items[0].clone(); 65];
    });
    let mut transition = fixture::commands().remove(2);
    transition.args.0[4].value = ActionValue::Null;
    check(transition.clone()).unwrap();
    transition.args.0[2].value = ActionValue::Hash(Hash::new([0; 32]));
    assert!(check(transition).is_err());

    let schema = one_command(vec![field("reviewer", FieldType::Principal)]);
    validate_action_schema(&schema).unwrap();
    let assign = |p: candid::Principal| {
        let action = with(
            &schema,
            "Assign",
            vec![arg("reviewer", ActionValue::Principal(p))],
        );
        validate_app_action(&action)?;
        validate_action_command(&action, &schema)
    };
    assign(candid::Principal::from_slice(&[5, 1])).unwrap();
    assert!(assign(candid::Principal::anonymous()).is_err());
}

#[test]
fn schemas_are_bounded_and_unambiguous() {
    let nat = || FieldType::Nat { min: 0, max: 9 };
    let valid = one_command(vec![field("a", nat())]);
    validate_action_schema(&valid).unwrap();
    let rejected = |schema: ActionSchema| assert!(validate_action_schema(&schema).is_err());
    rejected(ActionSchema {
        version: 2,
        ..valid.clone()
    });
    rejected(ActionSchema {
        version: 1,
        commands: vec![],
    });
    let mut twice = valid.clone();
    twice.commands.push(twice.commands[0].clone());
    rejected(twice);
    rejected(one_command(vec![field("a", nat()), field("a", nat())]));
    rejected(one_command(vec![field("1a", nat())]));
    rejected(one_command(vec![field(
        "a",
        FieldType::Nat { min: 2, max: 1 },
    )]));
    for max_bytes in [0, 4097] {
        rejected(one_command(vec![field(
            "a",
            FieldType::Text {
                max_bytes,
                multiline: false,
            },
        )]));
    }
    rejected(one_command(vec![field(
        "a",
        FieldType::List {
            item: Box::new(nat()),
            max_items: 0,
        },
    )]));
    rejected(one_command(vec![field(
        "a",
        FieldType::Choice { options: vec![] },
    )]));
    rejected(one_command(vec![field(
        "a",
        FieldType::Record {
            fields: SchemaFields(vec![]),
        },
    )]));
    rejected(one_command(vec![field(
        "a",
        FieldType::Optional {
            item: Box::new(FieldType::Optional {
                item: Box::new(nat()),
            }),
        },
    )]));
    let list = |item: FieldType| FieldType::List {
        item: Box::new(item),
        max_items: 1,
    };
    validate_action_schema(&one_command(vec![field("a", list(list(nat())))])).unwrap();
    rejected(one_command(vec![field("a", list(list(list(nat()))))]));
    let mut labels = valid.clone();
    labels.commands[0].title = vec![];
    rejected(labels.clone());
    labels.commands[0].title = vec![
        ActionLabel {
            locale: "en".into(),
            text: "A".into(),
        };
        2
    ];
    rejected(labels.clone());
    labels.commands[0].title = label("Line\nbreak");
    rejected(labels);
    // Commands, fields and values told apart by their labels never share a
    // text, whichever locale the confirmation page shows.
    let ambiguous = |schema: ActionSchema| {
        assert_eq!(
            validate_action_schema(&schema),
            Err(Error::InvalidInput("ambiguous labels".into()))
        )
    };
    let texts = |pairs: &[(&str, &str)]| -> Vec<ActionLabel> {
        pairs
            .iter()
            .map(|(locale, text)| ActionLabel {
                locale: (*locale).into(),
                text: (*text).into(),
            })
            .collect()
    };
    let decision = |yes, no| one_command(vec![field("a", FieldType::Bool { yes, no })]);
    validate_action_schema(&decision(texts(&[("en", "OK"), ("fr", "OK")]), label("No"))).unwrap();
    ambiguous(decision(label("Approve"), label("Approve")));
    ambiguous(decision(
        label("Approve"),
        texts(&[("en", "Refuse"), ("zh", "Approve")]),
    ));
    let option = |value: &str| ChoiceOption {
        value: value.into(),
        label: label("Same"),
    };
    ambiguous(one_command(vec![field(
        "a",
        FieldType::Choice {
            options: vec![option("x"), option("y")],
        },
    )]));
    let mut fields = one_command(vec![field("a", nat()), field("b", nat())]);
    fields.commands[0].fields.0[1].label = label("a");
    ambiguous(fields);
    let mut titles = valid.clone();
    titles.commands.push(CommandSchema {
        name: "Other".into(),
        ..valid.commands[0].clone()
    });
    ambiguous(titles);
    let wide = (0..32)
        .map(|i| {
            field(
                &format!("f{i}"),
                FieldType::Choice {
                    options: (0..32)
                        .map(|j| ChoiceOption {
                            value: format!("o{j}"),
                            label: label(&format!("o{j}")),
                        })
                        .collect(),
                },
            )
        })
        .collect();
    assert_eq!(
        validate_action_schema(&one_command(wide)),
        Err(Error::QuotaExceeded)
    );
}

#[test]
fn admission_and_registration_fail_closed() {
    let action = fixture::action();
    let mut app = fixture::app();
    validate_app(&app).unwrap();
    validate_action_admission(&action, &app, action.issued_at_ms).unwrap();
    assert_eq!(
        validate_action_admission(&action, &app, action.expires_at_ms),
        Err(Error::Expired)
    );
    let mut changed = app.clone();
    changed.action_schema.as_mut().unwrap().commands.pop();
    validate_app(&changed).unwrap();
    assert_eq!(
        validate_action_admission(&action, &changed, action.issued_at_ms),
        Err(Error::IntegrityFailed)
    );
    changed.action_schema = None;
    assert!(validate_app(&changed).is_err());
    assert_eq!(
        validate_action_admission(&action, &changed, action.issued_at_ms),
        Err(Error::Forbidden)
    );
    changed = app.clone();
    changed
        .capabilities
        .retain(|c| *c != AppCapability::SignAction);
    changed.profiles.clear();
    assert!(validate_app(&changed).is_err());
    app.paused = true;
    assert_eq!(
        validate_action_admission(&action, &app, action.issued_at_ms),
        Err(Error::Locked)
    );
}

#[test]
fn app_action_attestations_require_the_signing_account_and_a_bounded_approval() {
    let action = fixture::action();
    let request = AppActionAttestRequest {
        account_id: action.signing_account,
        issuer: "https://example.test/u/00000000000000000000".into(),
        action: action.clone(),
        signature: [6; 64].into(),
        approval: Approval {
            device_id: Hash::new([1; 32]),
            security_epoch: 1,
            sequence: 1,
            request_id: Hash::new([2; 32]),
            expires_at: action.expires_at_ms,
            signature: Default::default(),
        },
    };
    let statement = app_action_statement(&request).unwrap();
    assert_eq!(statement.issuer, request.issuer);
    assert_eq!(statement_purpose(&statement), KeyPurpose::AppAction);
    let mut foreign = request.clone();
    foreign.account_id = AccountId([4; 12]);
    assert_eq!(app_action_statement(&foreign), Err(Error::Forbidden));
    let mut late = request;
    late.approval.expires_at = action.expires_at_ms + 1;
    assert_eq!(app_action_statement(&late), Err(Error::Expired));
}
