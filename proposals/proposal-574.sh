#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# Before submitting, add SNS root as a controller of all nine canisters:
# for canister in dmsg_handle dmsg_user dmsg_cose dmsg_directory dmsg_payment membership dmsg_commerce dmsg_wallet_hub dmsg_connect; do
#   icp canister settings update "$canister" -e ic --add-controller d7wvo-iiaaa-aaaaq-aacsq-cai
# done
# After execution, verify that SNS root lists all nine dapps and is their sole controller.

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal '(
    record {
        title = "Register new dMsg canisters with ICPanda SNS";
        url = "https://github.com/ldclabs/ic-panda/blob/main/docs/dmsg_canisters.md";
        summary = "Register the nine new dMsg canisters with ICPanda SNS and transfer their canister control to SNS root (d7wvo-iiaaa-aaaaq-aacsq-cai), so future canister upgrades are governed by PANDA holders through SNS proposals.

1. dmsg_handle: handle registration, ownership, transfers and legacy name claims.
2. dmsg_user: accounts, authentication bindings, devices and recovery controls.
3. dmsg_cose: content-root vetKD public keys and recovery derivation.
4. dmsg_directory: certified publication of Agent Delegation principal documents.
5. dmsg_payment: paid-message escrow, settlement and refunds.
6. membership: PANDA SNS membership eligibility and cross-product benefit allocation.
7. dmsg_commerce: subscription plans, orders, refunds and resource entitlements.
8. dmsg_wallet_hub: multisig wallet activation, creation and approved module management.
9. dmsg_connect: the connect.dmsg.net bridge asset canister.

Individual dmsg_wallet instances are managed through dmsg_wallet_hub and are not registered directly by this proposal.

Mainnet canister IDs: https://github.com/ldclabs/ic-panda/blob/main/.icp/data/mappings/ic.ids.json";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {
                    principal "wfxrt-2iaaa-aaaaj-qsjwa-cai"; // dmsg_handle
                    principal "wqqa6-3aaaa-aaaaj-qsjvq-cai"; // dmsg_user
                    principal "wcwxh-xqaaa-aaaaj-qsjwq-cai"; // dmsg_cose
                    principal "wlv43-byaaa-aaaaj-qsjxa-cai"; // dmsg_directory
                    principal "urjuh-7iaaa-aaaaj-qsjya-cai"; // dmsg_payment
                    principal "wmu2p-maaaa-aaaaj-qsjxq-cai"; // membership
                    principal "uwist-sqaaa-aaaaj-qsjyq-cai"; // dmsg_commerce
                    principal "ghcje-dyaaa-aaaas-qg7va-cai"; // dmsg_wallet_hub
                    principal "acjs3-cyaaa-aaaas-qg7da-cai"; // dmsg_connect
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
