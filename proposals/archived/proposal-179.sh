#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(variant { OssBucket }, principal "sb6zj-3aaaa-aaaaj-qndla-cai")')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_add_canister() to add ic_oss_bucket_02 canister to ic_message_channel_02 service\";
        url = \"https://github.com/ldclabs/ic-panda/tree/main/src/ic_message_channel\";
        summary = \"This proposal executes admin_add_canister() on zof5a-5yaaa-aaaai-acr2q-cai to add ic_oss_bucket canister to ic_message_channel_02 service.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_107 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json