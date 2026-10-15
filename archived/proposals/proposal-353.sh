#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Transfer 905 ICP to cover the $4,800 USD cost of registering the anda.ai domain";
        url = "https://anda.ai";
        summary = "Details:\n\n- The domain was purchased on May 16, 2024, via Namecheap.\n  - Namecheap Invoice: https://ap.www.namecheap.com/profile/billing/order/details/170959095/Order\n  - The domain will serve as the official website for Anda AI, ICPanda’s next-gen AI agent platform.\n\nPurpose of Anda AI:\nAnda AI is developing AI agents with persistent memory, decentralized trust, and swarm intelligence. The website will host updates, documentation, and access to the product.\n\nDevelopment Status:\n  - Client app (Tauri-based, cross-platform): https://github.com/ldclabs/anda-app\n  - Beta launch targeted for end of June 2025.";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 1 : int32;
                to_principal = opt principal "ntihc-z566a-oifro-4tvua-vkbdv-ndi7q-tx2h6-yblw7-t6ofd-g7fwa-gqe";
                to_subaccount = null;
                memo = null;
                amount_e8s = 90_500_000_000 : nat64;
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
