#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function to \"Grant SNS asset canister permissions for upgrading the frontend\"";
        url = "https://internetcomputer.org/docs/current/developer-docs/daos/sns/managing/sns-asset-canister#sns-genericnervoussystemfunctions";
        summary = "Adding a new generic function that grant SNS asset canister permissions for upgrading the frontend.\n\nFix issue: Proposal 9 execution failed due to the DAO container lacking Commit permissions.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_003 : nat64;
                name = "Grant SNS asset canister permissions for upgrading the frontend";
                description = opt "Grant SNS asset canister permissions for upgrading the frontend.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "c63a7-6yaaa-aaaap-ab3gq-cai";
                        target_canister_id = opt principal "c63a7-6yaaa-aaaap-ab3gq-cai";
                        validator_method_name = opt "validate_grant_permission";
                        target_method_name = opt "grant_permission";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json