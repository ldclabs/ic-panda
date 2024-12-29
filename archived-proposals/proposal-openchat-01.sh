#!/usr/bin/env bash

export PROPOSAL_NEURON_ID="ca8df98c3b947fba74bebd51c984fd1352efc7e79b55b755f721b3813b266da9"
export PROPOSAL_PEM_FILE="$HOME/.config/dfx/identity/$(dfx identity whoami)/identity.pem"

export BLOB="$(didc encode --format blob '(record {governance_canister_id=principal "dwv6s-6aaaa-aaaaq-aacta-cai"; community_id=principal "dqcvf-haaaa-aaaar-a5uqq-cai"})')"

quill sns make-proposal --canister-ids-file ./sns_openchat_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Import the 'ICPanda DAO Proposals' group into the 'ICPanda Community'\";
        url = \"https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai\";
        summary = \"Import the [ICPanda DAO Proposals](https://oc.app/group/x3eaz-zaaaa-aaaar-bai5q-cai) group into the [ICPanda Community](https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai).\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 4003 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json