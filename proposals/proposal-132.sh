#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function to \"Commit proposed assets for upgrading the ic_message_frontend canister\"";
        url = "https://internetcomputer.org/docs/current/developer-docs/daos/sns/managing/sns-asset-canister#sns-genericnervoussystemfunctions";
        summary = "Adding a new generic function that commit proposed assets for upgrading the ic_message_frontend canister.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_100 : nat64;
                name = "Commit proposed assets for upgrading the ic_message_frontend canister";
                description = opt "Commit proposed assets for upgrading the ic_message_frontend canister.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "2fvu6-tqaaa-aaaap-akksa-cai";
                        target_canister_id = opt principal "2fvu6-tqaaa-aaaap-akksa-cai";
                        validator_method_name = opt "validate_commit_proposed_batch";
                        target_method_name = opt "commit_proposed_batch";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json