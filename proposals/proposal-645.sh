#!/usr/bin/env bash
set -eo pipefail
source "$(pwd)/proposals/env.sh"

# build and get batch_id, evidence:
# dfx deploy dmsg_connect --ic --by-proposal

# Generate the signed proposal message from the repository root.
BLOB="$(didc encode --format blob '(record { batch_id = 5 : nat; evidence = blob "\57\be\b5\08\76\35\ea\57\b4\3c\10\c5\2d\9c\a1\27\0c\e4\5b\f3\c4\90\9a\97\aa\5b\0f\97\06\9e\47\8d" })')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
        title = \"Publish dMsg Connect asset batch 5\";
        url = \"https://connect.dmsg.net/\";
        summary = \"This proposal publishes the staged assets for connect.dmsg.net by calling commit_proposed_batch on dmsg_connect (acjs3-cyaaa-aaaas-qg7da-cai). The proposed batch is 5 and its evidence is 57beb5087635ea57b43c10c52d9ca1270ce45bf3c4909a97aa5b0f97069e478d.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 3000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
