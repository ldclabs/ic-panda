#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# ic-oss-cli -i debug/oss.pem cluster-add-wasm -c 5szpn-tiaaa-aaaaj-qncoq-cai --path .dfx/ic/github/ic_oss_bucket.wasm.gz --description "ic_oss_bucket v1.2.0" --ic
# dfx canister call ic_oss_cluster get_cluster_info '()' --ic

export BLOB="$(didc encode --format blob '(record {canister=principal "532er-faaaa-aaaaj-qncpa-cai"}, null)')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_deploy_bucket() to update ic-oss-bucket WASM to v1.2.3\";
        url = \"https://github.com/ldclabs/ic-oss/releases/tag/v1.2.3\";
        summary = \"This proposal executes admin_deploy_bucket() on ic_oss_cluster 5szpn-tiaaa-aaaaj-qncoq-cai to upgrade ic_oss_bucket 532er-faaaa-aaaaj-qncpa-cai.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_115 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
