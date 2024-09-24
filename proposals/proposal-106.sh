#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"admin_update_price\" to messaging management service";
        url = "https://github.com/ldclabs/ic-panda/blob/main/src/ic_message/src/api_admin.rs#L47";
        summary = "The `admin_update_price` function is used to update the price for registering a username and creating a channel.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_102 : nat64;
                name = "`admin_update_price` function";
                description = opt "It is used to update the price for registering a username and creating a channel.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "nscli-qiaaa-aaaaj-qa4pa-cai";
                        target_canister_id = opt principal "nscli-qiaaa-aaaaj-qa4pa-cai";
                        validator_method_name = opt "validate_admin_update_price";
                        target_method_name = opt "admin_update_price";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json