#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(record {batch_id=27:nat; evidence=blob "\96\53\c9\de\a7\f0\a8\38\dc\f4\b9\90\38\a8\ef\b2\49\fe\b1\6f\9f\58\18\0d\88\0c\14\c1\ee\9c\81\12"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v1.1.0\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v1.1.0.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json