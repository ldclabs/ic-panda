#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register IC-OSS canisters";
        url = "https://github.com/ldclabs/ic-oss";
        summary = "IC-OSS canisters: ic_oss_cluster. ic_oss_bucket canisters will be managed by ic_oss_cluster.\n\nRefer: https://github.com/ldclabs/ic-panda/blob/main/canister_ids.json";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "5szpn-tiaaa-aaaaj-qncoq-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json