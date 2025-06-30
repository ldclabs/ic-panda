#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Long-Term Treasury Lock for ICPanda Sustainability 1/2";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai";
        summary = "To ensure the long-term development of ICPanda, we propose locking 10% of the total PANDA tokens (100M) in long-term staking by the team account (fkbua-maeyh-k6ldp-4j45o-64d4t-ct4ww-4nzje-p6rbp-eqlee-vcq6x-cqe) via the NNS App. The staking rewards will be used for ICPanda operations and growth.\n";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 2 : int32;
                to_principal = opt principal "fkbua-maeyh-k6ldp-4j45o-64d4t-ct4ww-4nzje-p6rbp-eqlee-vcq6x-cqe";
                to_subaccount = null;
                memo = null;
                amount_e8s = 7000000000000000 : nat64;
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
