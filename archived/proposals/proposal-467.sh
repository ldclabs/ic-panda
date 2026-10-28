#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register ic_signin_with Canister";
        url = "https://github.com/ldclabs/ic-panda/tree/main/src/ic_signin_with";
        summary = "ic_signin_with is used to sign-in users via EVM & SVM wallets.";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "ny3i7-miaaa-aaaap-an5mq-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
