#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Transfer 10_000_000 PANDA to add liquidity to the PANDA/ICP pool on kongswap";
        url = "https://kongswap.io/pools/add?token0=druyg-tyaaa-aaaaq-aactq-cai&token1=ryjl3-tyaaa-aaaaa-aaaba-cai";
        summary = "Proposal to add 10,000,000 PANDA/3,290 ICP liquidity on KongSwap.\n\n1. Since KongSwap does not support adding liquidity directly from the DAO treasury, we will use the team account on KongSwap (3xwwj-v46zk-3ssuw-6wvhm-nle5f-cyx2d-somz2-vtnem-pgowp-7z6hp-lqe) to manage the liquidity.\n2. 10,000,000 PANDA will be transferred from the DAO treasury to the team account.\n3. 3,290 ICP will be transferred from the team account on ICPSwap (xj4cn-sujbq-csa7q-lj5ru-bmavz-gk5ec-eg276-uyo6x-zmedq-rsaqn-nqe) to the team account on KongSwap.\n\nLiquidity will be added to KongSwap pool immediately once funds are in place.\n";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 2 : int32;
                to_principal = opt principal "3xwwj-v46zk-3ssuw-6wvhm-nle5f-cyx2d-somz2-vtnem-pgowp-7z6hp-lqe";
                to_subaccount = null;
                memo = null;
                amount_e8s = 1000000000000000 : nat64;
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json