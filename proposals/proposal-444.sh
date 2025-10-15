#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(record {canister=principal "sb6zj-3aaaa-aaaaj-qndla-cai"}, null)')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_deploy_bucket() to update ic-oss-ic_oss_bucket_02 WASM to v1.2.0\";
        url = \"https://github.com/ldclabs/ic-oss/releases/tag/v1.2.0\";
        summary = \"This proposal executes admin_deploy_bucket() on ic_oss_cluster 5szpn-tiaaa-aaaaj-qncoq-cai to upgrade ic_oss_bucket_02 sb6zj-3aaaa-aaaaj-qndla-cai.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_115 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
