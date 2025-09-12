#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=75:nat; evidence=blob "\9a\19\7a\e1\f9\a6\5e\05\31\65\8d\0a\58\ce\02\bc\ff\70\b9\85\9e\42\9d\3e\f2\1a\df\86\5d\69\dc\31"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v2.13.1\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v2.13.1.\n\n1. fix: fix token price.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json