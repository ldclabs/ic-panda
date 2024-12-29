#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --network ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=34:nat; evidence=blob "\1c\1b\e4\03\83\00\e2\b0\20\08\a9\43\e4\72\e4\bf\db\6d\fc\d7\fd\fb\c5\fc\9e\99\39\5d\f7\75\69\7e"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v1.5.1\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v1.5.1.\n\n1. Feat: PANDA Prize.\n\n2. Feat: PANDA Naming.\n\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json