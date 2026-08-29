#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=84:nat; evidence=blob "\e4\ec\a9\19\e7\7f\ae\c2\62\b2\f3\22\7b\be\72\3d\05\73\a7\80\3f\82\99\4a\b9\61\c9\e5\91\fc\4f\46"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v3.1.0\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v3.1.0.\n\n1. feat: migrate to the ICP JS SDK and replace the Skeleton UI framework with in-repo components;\n2. style: refresh the page ground to cool paper with a blueprint grid, and tune the header, footer and hero.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json