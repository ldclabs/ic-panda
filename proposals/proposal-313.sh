#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Allocating 10 million PANDA tokens to Luis";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/286";
        summary = "This proposal fulfills part of the commitments outlined in Proposal 286, allocating 10 million PANDA tokens to Luis in recognition of his contributions and in anticipation of further achievements.\n\nAs per Proposal 286, the PANDA token price has remained above 0.005 USD for three consecutive days, enabling the fulfillment of clauses 2 and 3, totaling 10 million PANDA.\n\nFor details, refer to Proposal 286 https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/286.\n\n## Token Recipient Address:\nfiecn-b6ky4-h2amr-nmcrl-ra36a-ogffr-oaf4x-aqjsq-kp7bo-nfbka-yae";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 2 : int32;
                to_principal = opt principal "fiecn-b6ky4-h2amr-nmcrl-ra36a-ogffr-oaf4x-aqjsq-kp7bo-nfbka-yae";
                to_subaccount = null;
                memo = null;
                amount_e8s = 1000_000_000_000_000 : nat64;
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
