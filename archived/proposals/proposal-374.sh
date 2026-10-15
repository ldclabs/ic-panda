#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_message_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(vec { "vetkd_public_key"; "vetkd_encrypted_key" })')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_add_allowed_apis() to enable VetKeys APIs\";
        url = \"https://github.com/ldclabs/ic-cose/blob/main/src/ic_cose_canister/src/api_admin.rs#L48\";
        summary = \"This proposal executes admin_add_allowed_apis() on n3bau-gaaaa-aaaaj-qa4oq-cai to enable VetKeys APIs.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_117 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
