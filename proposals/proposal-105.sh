#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"admin_add_canister\" to messaging management service";
        url = "https://internetcomputer.org/docs/current/developer-docs/daos/sns/managing/sns-asset-canister#sns-genericnervoussystemfunctions";
        summary = "The `admin_add_canister` function is used to add more COSE, Profile, and Channel service canisters to the messaging cluster.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_101 : nat64;
                name = "`admin_add_canister` function";
                description = opt "It is used to add more COSE, Profile, and Channel service canisters to the messaging cluster.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "nscli-qiaaa-aaaaj-qa4pa-cai";
                        target_canister_id = opt principal "nscli-qiaaa-aaaaj-qa4pa-cai";
                        validator_method_name = opt "validate_admin_add_canister";
                        target_method_name = opt "admin_add_canister";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json