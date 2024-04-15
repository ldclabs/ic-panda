#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(record {batch_id=30:nat; evidence=blob "\f5\95\52\4f\a1\79\f7\89\3a\12\71\b4\d7\a9\da\a8\ff\6d\86\ed\1e\e6\7f\0a\69\62\49\0a\04\07\90\71"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v1.4.0\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v1.4.0.\n\n-Feat: Support airdrop verifying by Twitter OAuth.\n-Feat: Upgrade Lucky pool UI.\n\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json