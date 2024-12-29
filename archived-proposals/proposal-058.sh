#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Transfer 4000 ICP to add liquidity to the PANDA/ICP pool on ICPSwap";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/58";
        summary = "Proposes to add liquidity to the [ICP/PANDA pool on ICPSwap](https://info.icpswap.com/swap/pool/details/5fq4w-lyaaa-aaaag-qjqta-cai).\n\nFollowing this proposal, we proposals to apply for the transfer of 4000 ICP from the ICPanda DAO treasury to the ICP/PANDA SwapPool canister owned by ICPSwap (5fq4w-lyaaa-aaaag-qjqta-cai).\n\nThe destination account for both transfers will be the same but on 2 different ledgers(ICP and PANDA) and is as follows:\nPrincipal: 5fq4w-lyaaa-aaaag-qjqta-cai\n\nSubaccount: [10,0,0,0,0,2,0,0,166,1,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0] (this is the Subaccount generated from the ICPanda DAO governance canister Id, dwv6s-6aaaa-aaaaq-aacta-cai)\n\nThe liquidity funds are located in the Subaccount generated by the ICPanda DAO governance.\n\nIf these proposals are approved, ICPSwap will add the funds to the liquidity pool. The transferred funds will provide liquidity in the range of 500 to 20000 PANDA per ICP.\n\nPlease note: There may be price fluctuations for PANDA between proposal initiation and successful transfer. When adding liquidity, the price range will be dynamically adjusted based on the current price of PANDA, but it will not deviate significantly from the set price range.\n\nSubsequent proposals can be made to add more liquidity or adjust the ranges if the price of PANDA experiences significant changes.\n";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 1 : int32;
                to_principal = opt principal "5fq4w-lyaaa-aaaag-qjqta-cai";
                to_subaccount = opt record { subaccount = vec {10;0;0;0;0;2;0;0;166;1;1;0;0;0;0;0;0;0;0;0;0;0;0;0;0;0;0;0;0;0;0;0}; };
                memo = null;
                amount_e8s = 400000000000 : nat64;
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json