#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"admin_collect_token\" to messaging management service";
        url = "https://github.com/ldclabs/ic-panda/blob/main/src/ic_message/src/api_admin.rs#L73";
        summary = "The `admin_collect_token` function is used to collect tokens to a account.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_103 : nat64;
                name = "`admin_collect_token` function";
                description = opt "It is used to collect tokens to a account.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "nscli-qiaaa-aaaaj-qa4pa-cai";
                        target_canister_id = opt principal "nscli-qiaaa-aaaaj-qa4pa-cai";
                        validator_method_name = opt "validate_admin_collect_token";
                        target_method_name = opt "admin_collect_token";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json