#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"admin_remove_managers\" to ic_message service";
        url = "https://internetcomputer.org/docs/current/developer-docs/daos/sns/managing/sns-asset-canister#sns-genericnervoussystemfunctions";
        summary = "The `admin_remove_managers` function is used to remove managers from the ic_message canister.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_109 : nat64;
                name = "`admin_remove_managers` function";
                description = opt "It is used to remove managers from the ic_message canister.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "nscli-qiaaa-aaaaj-qa4pa-cai";
                        target_canister_id = opt principal "nscli-qiaaa-aaaaj-qa4pa-cai";
                        validator_method_name = opt "validate2_admin_remove_managers";
                        target_method_name = opt "admin_remove_managers";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json