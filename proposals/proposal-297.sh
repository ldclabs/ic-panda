#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"admin_remove_managers\" to ic_cose_canister service";
        url = "https://github.com/ldclabs/ic-cose/blob/main/src/ic_cose_canister/src/api_admin.rs";
        summary = "The `admin_remove_managers` function is used to remove managers from the ic_cose_canister canister.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_116 : nat64;
                name = "`admin_remove_managers` function";
                description = opt "It is used to remove managers from the ic_cose_canister canister.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "n3bau-gaaaa-aaaaj-qa4oq-cai";
                        target_canister_id = opt principal "n3bau-gaaaa-aaaaj-qa4oq-cai";
                        validator_method_name = opt "validate2_admin_remove_managers";
                        target_method_name = opt "admin_remove_managers";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json