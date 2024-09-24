# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Proposal for Airdropping 32% (320 Million) PANDA Tokens to the Community\";
        url = \"https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai\";
        summary = \"We're proposing an airdrop of 320 million PANDA tokens to our loyal holders!.\";
        action = opt variant {
            Motion = record {
                motion_text = \"In the initial token distribution plan for ICPanda DAO's SNS, 50% of the tokens (500 million) were allocated to a Lucky Pool, with 100 million designated for free airdrops and 400 million for lucky draws. Since the lucky draw feature has not met expectations, 399 million tokens remain unused. The team proposes reclaiming 20% of these tokens to the treasury (for future CEX listings) and airdropping 80% to PANDA token holders. The proposal is as follows:\n\n1. Eligible airdrop recipients include two types:\n   - Neurons staked with PANDA tokens.\n   - Regular wallet addresses with a balance of over 10,000 tokens (canister addresses are excluded).\n\n2. Airdrop amounts will be distributed based on airdrop weight:\n   - For regular addresses, the weight is equal to the token balance (e.g., if an address holds 1 million PANDA tokens, its airdrop weight is 1 million).\n   - For neurons, the weight is based on voting power (e.g., if a neuron has 1 million PANDA tokens staked with a 2-year dissolve delay, its voting power would be approximately 2 million, so the airdrop weight would be 2 million).\n\n3. A snapshot of eligible airdrop addresses will be taken on October 31, 2024, at 24:00 UTC.\n\n4. After the snapshot, there will be a public review period where anyone can check the airdrop amounts for any address. The airdrop will be executed starting November 11, 2024, at 00:00 UTC, with tokens distributed to eligible addresses via smart contract.\";
            }
        };
    }
)" > proposal-message.json

quill send proposal-message.json
