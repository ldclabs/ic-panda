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
        title = "Register admin_register_version on dmsg_wallet_hub";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.5/src/dmsg_wallet_hub/dmsg_wallet_hub.did";
        summary = "Registers generic function 3037 to call admin_register_version on dmsg_wallet_hub (ghcje-dyaaa-aaaas-qg7va-cai), with validate_admin_register_version on the same canister validating and rendering future execution payloads. Topic: CriticalDappOperations.\n\nRegister the staged wallet Wasm by its hash and release notes as the next version for new wallets. Existing wallets upgrade only after their members accept that exact version and hash.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_037 : nat64;
            name = "admin_register_version on dmsg_wallet_hub";
            description = opt "Register the staged wallet Wasm by its hash and release notes as the next version for new wallets. Existing wallets upgrade only after their members accept that exact version and hash.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { CriticalDappOperations };
                validator_canister_id = opt principal "ghcje-dyaaa-aaaas-qg7va-cai";
                target_canister_id = opt principal "ghcje-dyaaa-aaaas-qg7va-cai";
                validator_method_name = opt "validate_admin_register_version";
                target_method_name = opt "admin_register_version";
              }
            };
          }
        };
      }
    };
  },
)'
