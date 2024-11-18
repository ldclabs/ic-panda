#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"admin_update_bucket_canister_settings\" to IC-OSS service";
        url = "https://internetcomputer.org/docs/current/developer-docs/daos/sns/managing/sns-asset-canister#sns-genericnervoussystemfunctions";
        summary = "The `admin_update_bucket_canister_settings` function is used to update bucket canister settings from the ic_oss_cluster canister.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_114 : nat64;
                name = "`admin_update_bucket_canister_settings` function";
                description = opt "It is used to update bucket canister settings from the ic_oss_cluster canister.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "5szpn-tiaaa-aaaaj-qncoq-cai";
                        target_canister_id = opt principal "5szpn-tiaaa-aaaaj-qncoq-cai";
                        validator_method_name = opt "validate_admin_update_bucket_canister_settings";
                        target_method_name = opt "admin_update_bucket_canister_settings";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json