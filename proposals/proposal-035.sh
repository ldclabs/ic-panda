#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --network ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=37:nat; evidence=blob "\01\38\d2\f3\c4\5f\97\3f\5e\d2\81\56\a1\7f\37\01\71\bb\95\8e\e7\75\a4\88\8e\67\76\d6\3b\ea\39\c3"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v1.5.4\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v1.5.4.\n\n1. Feat: support QR code for PANDA prize\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json