#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(100000000: nat)')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_collect_tokens() to transfer 1 PANDA token from ic_panda_luckypool to DAO treasury\";
        url = \"https://github.com/ldclabs/ic-panda/blob/main/src/ic_panda_luckypool/src/api_admin.rs#L16\";
        summary = \"This proposal executes admin_collect_tokens() on ic_panda_luckypool a7cug-2qaaa-aaaap-ab3la-cai to transfer PANDA token from ic_panda_luckypool to DAO treasury.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_005 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
