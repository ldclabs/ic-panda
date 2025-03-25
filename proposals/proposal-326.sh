#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register ICPSwap TransferPositionValidator Canister";
        url = "https://github.com/ICPSwap-Labs/icpswap-validators";
        summary = "Assist ICPanda DAO team (Dev principal: xj4cn-sujbq-csa7q-lj5ru-bmavz-gk5ec-eg276-uyo6x-zmedq-rsaqn-nqe) in managing the liquidity positions in ICPSwap which owned by governance canister dwv6s-6aaaa-aaaaq-aacta-cai.";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "7i4z4-fyaaa-aaaap-anzsa-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
