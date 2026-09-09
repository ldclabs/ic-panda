#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(principal "druyg-tyaaa-aaaaq-aactq-cai", record {owner=principal "dwv6s-6aaaa-aaaaq-aacta-cai"; subaccount=blob "\f6\cc\24\dd\36\82\35\db\df\2b\3c\79\2e\39\9a\c1\0f\00\a0\00\33\73\de\6d\09\60\ae\55\ca\87\3e\bb"}, 100000000)')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Check token transferred back from ICDex Trader to the treasury\";
        url = \"https://github.com/ldclabs/ICDex-Trader/blob/main/docs/Guide_for_SNS_treasury.md\";
        summary = \"This proposal executes withdraw() on 6sul7-xqaaa-aaaap-ahdsq-cai to check if token can be successfully transferred back from ICDex Trader to the treasury.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1004 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
