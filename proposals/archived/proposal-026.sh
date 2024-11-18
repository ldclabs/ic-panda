#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(record {batch_id=33:nat; evidence=blob "\8b\08\aa\9b\3f\ac\fa\9f\f7\a0\1b\80\06\0a\95\15\9b\7f\ad\8e\fd\fb\af\ba\21\0e\fa\07\2a\23\ee\a3"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v1.5.0\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v1.5.0.\n\n1. Feat: PANDA Prize.\n\n2. Feat: PANDA Naming.\n\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json