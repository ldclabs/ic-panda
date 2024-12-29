#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Transfer 500000000 PANDA to the Lucky Pool";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposals";
        summary = "According to the whitepaper, 50% of the total supply of PANDA tokens will be transferred from the treasury to the lucky pool for free airdrops and lucky draws. The ICP obtained from the lucky draw will be transferred back to the treasury in full.";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 2 : int32;
                to_principal = opt principal "a7cug-2qaaa-aaaap-ab3la-cai";
                to_subaccount = null;
                memo = null;
                amount_e8s = 50_000_000_000_000_000 : nat64;
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json