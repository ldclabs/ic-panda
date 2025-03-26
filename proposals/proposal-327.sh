#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"transferPosition\" to transfer position in ICPSwap";
        url = "https://github.com/ICPSwap-Labs/icpswap-validators";
        summary = "The `transferPosition` function is used to transfer position in ICPSwap pool 5fq4w-lyaaa-aaaag-qjqta-cai.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 2_000 : nat64;
                name = "`transferPosition` function";
                description = opt "It is used to transfer position in ICPSwap.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        topic = opt variant { ApplicationBusinessLogic };
                        validator_canister_id = opt principal "7i4z4-fyaaa-aaaap-anzsa-cai";
                        target_canister_id = opt principal "5fq4w-lyaaa-aaaag-qjqta-cai";
                        validator_method_name = opt "validate_transfer_position";
                        target_method_name = opt "transferPosition";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json

## quill can not support topic field, we should use dfx canister call to send proposal:
# dfx canister --network ic call dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
#   record {
#     subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
#     command = opt variant {
#       MakeProposal = record {
#         title = "Add a generic function \"transferPosition\" to transfer position in ICPSwap";
#         url = "https://github.com/ICPSwap-Labs/icpswap-validators";
#         summary = "The `transferPosition` function is used to transfer position in ICPSwap pool 5fq4w-lyaaa-aaaag-qjqta-cai.";
#         action = opt variant {
#             AddGenericNervousSystemFunction = record {
#                 id = 2_000 : nat64;
#                 name = "`transferPosition` function";
#                 description = opt "It is used to transfer position in ICPSwap.";
#                 function_type = opt variant {
#                     GenericNervousSystemFunction = record {
#                         topic = opt variant { ApplicationBusinessLogic };
#                         validator_canister_id = opt principal "7i4z4-fyaaa-aaaap-anzsa-cai";
#                         target_canister_id = opt principal "5fq4w-lyaaa-aaaag-qjqta-cai";
#                         validator_method_name = opt "validate_transfer_position";
#                         target_method_name = opt "transferPosition";
#                     }
#                 };
#             }
#         };
#       }
#     };
#   },
# )'