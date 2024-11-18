#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register ICDex Trader canister";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/40";
        summary = "In order to add liquidity to ICDex from SNS treasury, we should run a ICDex Trader canister and register it as a dApp canister.\n\nRefer: https://github.com/iclighthouse/ICDex-Trader/blob/main/docs/Guide_for_SNS_treasury.md";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "6sul7-xqaaa-aaaap-ahdsq-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json