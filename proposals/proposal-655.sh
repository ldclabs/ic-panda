#!/usr/bin/env bash
set -euo pipefail

# Like proposal 643, use icp-cli to preserve the governance topic.
# Running this script submits the registration proposal immediately.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Register set_promo_config on dmsg_commerce";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.12/src/dmsg_commerce/dmsg_commerce.did";
        summary = "Register SNS function 3047 under CriticalDappOperations. Future execution proposals can enable or pause new promotional and compensation membership grants and replace the list of authorized verifier principals. The target and validator are set_promo_config and validate_set_promo_config on dmsg_commerce (uwist-sqaaa-aaaaj-qsjyq-cai). Activation and the choice of verifier require a separate vote.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_047 : nat64;
            name = "set_promo_config on dmsg_commerce";
            description = opt "Set the issuance switch and verifier principals for promotional and compensation membership grants.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { CriticalDappOperations };
                validator_canister_id = opt principal "uwist-sqaaa-aaaaj-qsjyq-cai";
                target_canister_id = opt principal "uwist-sqaaa-aaaaj-qsjyq-cai";
                validator_method_name = opt "validate_set_promo_config";
                target_method_name = opt "set_promo_config";
              }
            };
          }
        };
      }
    };
  }
)'
