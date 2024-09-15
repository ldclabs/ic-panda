#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register E2EE Message canisters";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/90";
        summary = "E2EE Message canisters: ic_cose_canister, ic_message, ic_message_channel, ic_message_profile.\n\nRefer: https://github.com/ldclabs/ic-panda/blob/main/canister_ids.json";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "n3bau-gaaaa-aaaaj-qa4oq-cai"; principal "nscli-qiaaa-aaaaj-qa4pa-cai"; principal "nvdn4-5qaaa-aaaaj-qa4pq-cai"; principal "ijyxz-wyaaa-aaaaj-qa4qa-cai"};
            }
        };
    }
)' > proposal-message.json

quill send proposal-message.json