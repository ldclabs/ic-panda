#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Deregister asset canister to grant it grant_permission() and commit_proposed_batch() permissions";
        url = "https://forum.dfinity.org/t/issue-asset-canister-cant-be-upgraded-via-sns-proposal/23421";
        summary = "In order to comply with SNS upgrade rules for asset canister, it is necessary to grant Governance canister grant_permission() and commit_proposed_batch() permissions. Since the controller of the asset canister is currently SNS ROOT, we need to manually grant the Governance canister grant_permission() and commit_proposed_batch() permissions after handing over its controller to the team. After that, re-register the asset canister to SNS.\n\nRefer to ICLighthouse: https://nns.ic0.app/proposal/?u=hjcnr-bqaaa-aaaaq-aacka-cai&proposal=104";
        action = opt variant {
            DeregisterDappCanisters = record {
                canister_ids = vec {principal "c63a7-6yaaa-aaaap-ab3gq-cai"};
                new_controllers = vec {principal "d7wvo-iiaaa-aaaaq-aacsq-cai"; principal "i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe"};
            }
        };
    }
)' > proposal-message.json

quill send proposal-message.json