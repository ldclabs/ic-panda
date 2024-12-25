#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Welcoming Luis to ICPanda DAO as a Strategic Partner";
        url = "https://x.com/ssskkk911";
        summary = "We are honored to announce that Luis (https://x.com/ssskkk911) will be joining ICPanda DAO as a strategic partner. In this role, Luis will lead efforts in strategy, operations, and marketing collaborations for ICPanda DAO.\n\n## About Luis:\n- Education: Peking University\n- Professional Experience:\n  - Former employee at Ping An Fund\n  - Early Bitcoin (BTC) adopter\n  - Co-founder of the Ethereum Fog Foundation\n  - Pioneer in blockchain innovation, leading the implementation of the world’s first blockchain mobile phone, the Candy Blockchain Mobile Phone\n\n## Token Allocation Plan:\nTo incentivize and support Luis in his work with ICPanda DAO, we propose a total allocation of **20M PANDA tokens** from the DAO treasury. The distribution will follow a milestone-based payment structure:\n1. **2M PANDA tokens**: Paid upon the approval of this proposal to confirm the partnership.\n2. **4M PANDA tokens**: Paid when the PANDA token price remains above **0.002 USD** for three consecutive days.\n3. **6M PANDA tokens**: Paid when the PANDA token price remains above **0.005 USD** for three consecutive days.\n4. **8M PANDA tokens**: Paid when the PANDA token price remains above **0.01 USD** for three consecutive days.\n\n## Current Proposal Scope:\nThis proposal seeks approval for the initial payment of **2M PANDA tokens** to confirm Luis as a strategic partner.\n\n## Token Recipient Address:\nfiecn-b6ky4-h2amr-nmcrl-ra36a-ogffr-oaf4x-aqjsq-kp7bo-nfbka-yae";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 2 : int32;
                to_principal = opt principal "fiecn-b6ky4-h2amr-nmcrl-ra36a-ogffr-oaf4x-aqjsq-kp7bo-nfbka-yae";
                to_subaccount = null;
                memo = null;
                amount_e8s = 200_000_000_000_000 : nat64;
            }
        };
    }
)' > proposal-message.json

quill send proposal-message.json
