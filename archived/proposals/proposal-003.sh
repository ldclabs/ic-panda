#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function to \"Commit proposed assets for upgrading the frontend canister\"";
        url = "https://internetcomputer.org/docs/current/developer-docs/daos/sns/managing/sns-asset-canister#sns-genericnervoussystemfunctions";
        summary = "Adding a new generic function that commit proposed assets for upgrading the frontend canister.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_000 : nat64;
                name = "Commit proposed assets for upgrading the frontend canister";
                description = opt "Commit proposed assets for upgrading the frontend canister.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "c63a7-6yaaa-aaaap-ab3gq-cai";
                        target_canister_id = opt principal "c63a7-6yaaa-aaaap-ab3gq-cai";
                        validator_method_name = opt "validate_commit_proposed_batch";
                        target_method_name = opt "commit_proposed_batch";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json