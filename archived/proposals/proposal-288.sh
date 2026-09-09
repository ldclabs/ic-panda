#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"admin_deploy_bucket\" to IC-OSS service";
        url = "https://internetcomputer.org/docs/current/developer-docs/daos/sns/managing/sns-asset-canister#sns-genericnervoussystemfunctions";
        summary = "The `admin_deploy_bucket` function is used to upgrade bucket canister WASM from the ic_oss_cluster canister.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_115 : nat64;
                name = "`admin_deploy_bucket` function";
                description = opt "It is used to upgrade bucket canister WASM from the ic_oss_cluster canister.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "5szpn-tiaaa-aaaaj-qncoq-cai";
                        target_canister_id = opt principal "5szpn-tiaaa-aaaaj-qncoq-cai";
                        validator_method_name = opt "validate2_admin_deploy_bucket";
                        target_method_name = opt "admin_deploy_bucket";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json