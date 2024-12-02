#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
      title = "Transfer 3,575,000 PANDA to the dev account for ICPanda Thanksgiving event";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/217";
        summary = "According to Proposal #217, the ICPanda Thanksgiving event expenses are as follows:\n  1. **5,000 $PANDA rewards**: 115 recipients (excluding developer accounts).\n  2. **1,000,000 $PANDA rewards**: 3 recipients.\n\nTotal expenditure: **3,575,000 $PANDA**, advanced by the developer account. This proposal requests transferring **3,575,000 $PANDA** back to the developer account.";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 2 : int32;
                to_principal = opt principal "ina3q-qkxfe-betg6-qghu7-guvpz-e5kmg-wsxr5-t6yw2-o7ehe-zpq26-xqe";
                to_subaccount = null;
                memo = null;
                amount_e8s = 357_500_000_000_000 : nat64;
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
