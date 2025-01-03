#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"admin_collect_tokens\" to transfer PANDA tokens to DAO treasury";
        url = "https://github.com/ldclabs/ic-panda/blob/main/src/ic_panda_luckypool/src/api_admin.rs#L39";
        summary = "The `admin_collect_tokens` function is used to transfer PANDA tokens from ic_panda_luckypool to DAO treasury.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_005 : nat64;
                name = "`admin_collect_tokens` function";
                description = opt "It is used to transfer PANDA tokens to DAO treasury.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "a7cug-2qaaa-aaaap-ab3la-cai";
                        target_canister_id = opt principal "a7cug-2qaaa-aaaap-ab3la-cai";
                        validator_method_name = opt "validate_admin_collect_tokens";
                        target_method_name = opt "admin_collect_tokens";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json