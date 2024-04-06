#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Transfer 140 ICP to CryptoEights for Marketing Campaign Partnership between CryptoEights and ICPanda DAO\";
        url = \"https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/13\";
        summary = \"According to Proposal #13, we are entering into a 3-month marketing campaign partnershipn with CryptoEights, for which we need to pay 140 ICP.\n\";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 1 : int32;
                to_principal = opt principal \"mdpkg-77n55-ialpr-4awyj-tzmf6-fhgco-6fbu5-k7itz-p4r7h-fqqdt-mqe\";
                to_subaccount = null;
                memo = null;
                amount_e8s = 14000000000 : nat64;
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json