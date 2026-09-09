#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Transfer 200 ICP to Dev for maintaining DAO canisters";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai";
        summary = "Proposes to transfer 200 ICP to Dev account for maintaining canisters.\n\nThe developer principal is ntihc-z566a-oifro-4tvua-vkbdv-ndi7q-tx2h6-yblw7-t6ofd-g7fwa-gqe. ICPanda DAO canisters are maintained by a sub-account of this principal.\n\n200 ICP is expected to last for over six months.\n";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 1 : int32;
                to_principal = opt principal "ntihc-z566a-oifro-4tvua-vkbdv-ndi7q-tx2h6-yblw7-t6ofd-g7fwa-gqe";
                to_subaccount = null;
                memo = null;
                amount_e8s = 20000000000 : nat64;
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json