#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(vec {principal "i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_remove_managers() to remove the Dev manager\";
        url = \"https://github.com/ldclabs/ic-panda/tree/main/src/ic_message_channel\";
        summary = \"This proposal executes admin_remove_managers() on zof5a-5yaaa-aaaai-acr2q-cai to remove the Dev manager from the ic_message_channel_02 canister.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_112 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json