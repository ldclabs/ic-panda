#!/usr/bin/env bash
set -euo pipefail

# Submit after proposal 574 has registered the dMsg canisters with SNS.
# Like proposals 522-540, this uses icp-cli to preserve the topic field.
# Running this script submits one proposal immediately; use an identity
# authorized to make proposals for the neuron below.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Register admin_set_pricing on dmsg_wallet_hub";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.5/src/dmsg_wallet_hub/dmsg_wallet_hub.did";
        summary = "Registers generic function 3040 to call admin_set_pricing on dmsg_wallet_hub (ghcje-dyaaa-aaaas-qg7va-cai), with validate_admin_set_pricing on the same canister validating and rendering future execution payloads. Topic: ApplicationBusinessLogic.\n\nSet wallet activation pricing, cycles allocation, per-plan wallet quotas and the new-activation pause switch.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_040 : nat64;
            name = "admin_set_pricing on dmsg_wallet_hub";
            description = opt "Set wallet activation pricing, cycles allocation, per-plan wallet quotas and the new-activation pause switch.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { ApplicationBusinessLogic };
                validator_canister_id = opt principal "ghcje-dyaaa-aaaas-qg7va-cai";
                target_canister_id = opt principal "ghcje-dyaaa-aaaas-qg7va-cai";
                validator_method_name = opt "validate_admin_set_pricing";
                target_method_name = opt "admin_set_pricing";
              }
            };
          }
        };
      }
    };
  },
)'
