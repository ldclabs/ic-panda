#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(variant { OssCluster }, principal "5szpn-tiaaa-aaaaj-qncoq-cai")')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Try again: execute admin_add_canister() to add ic_oss_cluster canister to ic_message_profile service\";
        url = \"https://panda.fans/_/messages\";
        summary = \"This proposal executes admin_add_canister() on ijyxz-wyaaa-aaaaj-qa4qa-cai to add ic_oss_cluster canister to ic_message_profile service.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_105 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json