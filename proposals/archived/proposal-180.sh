#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(variant { OssCluster }, principal "5szpn-tiaaa-aaaaj-qncoq-cai")')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_add_canister() to add ic_oss_cluster canister to ic_message_channel_03 service\";
        url = \"https://github.com/ldclabs/ic-panda/tree/main/src/ic_message_channel\";
        summary = \"This proposal executes admin_add_canister() on 4jxyd-pqaaa-aaaah-qdqtq-cai to add ic_oss_cluster canister to ic_message_channel_03 service.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_108 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json