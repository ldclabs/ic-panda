#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_message_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=9:nat; evidence=blob "\09\f3\67\ef\b8\d5\47\cb\00\89\8d\10\8f\cd\d6\53\9f\ee\76\84\7d\96\74\ee\43\eb\3c\7e\c1\43\f9\87"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_message_frontend v2.3.0\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release ic_message_frontend v2.3.0.\n\n1. chore: 1. improve UI.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json