#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(record {batch_id=32:nat; evidence=blob "\dd\db\c4\8c\81\72\f4\33\e1\28\fe\4d\a4\bd\c5\75\f2\58\2a\48\65\4b\dc\a8\03\42\7d\34\af\c0\3e\04"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v1.4.2\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v1.4.2.\n\n1. Chore: add Google Analytics to frontend.\n\n2. Chore: introduce Lucky Balance.\n\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json