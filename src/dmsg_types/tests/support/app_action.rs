#![allow(dead_code)]
use candid::Principal;
use dmsg_types::{app_action::*, AccountId, Environment, Hash};

pub fn action() -> AppAction {
    let mut value = AppAction {
        version: 1,
        environment: Environment::Local,
        app_id: "tokenlist".into(),
        app_config_version: 1,
        origin: "http://localhost:5188".into(),
        receiver: Principal::from_slice(&[7, 1]),
        actor_id: AccountId([8; 12]),
        signing_account: AccountId([3; 12]),
        operation_id: Hash::new([1; 32]),
        intent_hash: Hash::new([2; 32]),
        input_hash: Hash::new([3; 32]),
        subject_hash: Hash::new([4; 32]),
        precondition_hash: Hash::new([5; 32]),
        role_snapshot_hash: Hash::new([6; 32]),
        signing_policy_hash: Hash::new([7; 32]),
        rule_set_hash: Hash::new([8; 32]),
        issued_at_ms: 1_800_000_000_000,
        expires_at_ms: 1_800_000_300_000,
        command: AppActionCommand::TokenListCertifyDisclosure {
            project_id: 42,
            contract_id: 9,
            revision: 3,
        },
        files: vec![ActionFile {
            file_id: "disclosure/draft/9".into(),
            revision: 3,
            sha256: Hash::new([9; 32]),
            byte_length: 412,
            media_type: "application/cbor".into(),
            display_name: Some("披露文件.cbor".into()),
            representation: ActionFileRepresentation::Original,
        }],
    };
    value.input_hash = dmsg_protocol::app_action::action_input_hash(&value.command);
    value
}

pub fn commands() -> Vec<AppActionCommand> {
    vec![
        action().command,
        AppActionCommand::TokenListDecideReview {
            project_id: 42,
            case_id: 5,
            round: 2,
            outcome: ActionReviewOutcome::ChangesRequested,
            changes: vec![ActionRequestedChange {
                locator: "07/total_supply".into(),
                detail: "Explain the complete supply allocation".into(),
                blocking: true,
            }],
            rationale: "The allocation differs from the ledger.".into(),
        },
        AppActionCommand::TokenListCertifyTransition {
            project_id: 42,
            transition_id: 8,
            statement_hash: Hash::new([10; 32]),
            rationale: "The published transition analysis is complete.".into(),
            analysis: Some(ActionArtifact {
                uri: "https://example.test/analysis.pdf".into(),
                sha256: Hash::new([11; 32]),
                content_type: "application/pdf".into(),
                size: 112,
            }),
        },
        AppActionCommand::TokenListApproveTransition {
            project_id: 42,
            transition_id: 8,
            approve: false,
            statement_hash: Hash::new([12; 32]),
            rationale: "Refused because obligations remain outstanding.".into(),
        },
    ]
}
