#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(record {canister_id=principal "532er-faaaa-aaaaj-qncpa-cai"; settings=record { controllers = opt vec {principal "5szpn-tiaaa-aaaaj-qncoq-cai"; principal "5vdms-kaaaa-aaaap-aa3uq-cai";}}})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_update_bucket_canister_settings() to update ic-oss-bucket controllers\";
        url = \"https://github.com/ldclabs/ic-oss/blob/main/src/ic_oss_cluster/src/api_admin.rs#L490\";
        summary = \"This proposal executes admin_update_bucket_canister_settings() on ic_oss_cluster 5szpn-tiaaa-aaaaj-qncoq-cai to add CycleOps blackhole 5vdms-kaaaa-aaaap-aa3uq-cai as ic-oss-bucket 532er-faaaa-aaaaj-qncpa-cai controllers.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_114 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
