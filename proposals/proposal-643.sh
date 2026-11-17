#!/usr/bin/env bash
set -euo pipefail

# Submit after the dmsg_commerce upgrade in proposal-641.sh has executed.
# Function ID 3046 was unused and not reserved on 2026-11-17;
# recheck the registry and pending registration proposals before submitting.
# Like proposals 522-540, this uses icp-cli to preserve the topic field.
# Running this script submits one proposal immediately; use an identity
# authorized to make proposals for the neuron below.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Register set_google_play_config on dmsg_commerce";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.11/src/dmsg_commerce/dmsg_commerce.did";
        summary = "Registers generic function 3046 to call set_google_play_config on dmsg_commerce (uwist-sqaaa-aaaaj-qsjyq-cai), with validate_set_google_play_config on the same canister validating and rendering future execution payloads. Topic: CriticalDappOperations.\n\nConfigure the Google Play Android package name, environment, explicit test-purchase acceptance, trusted verifier principals and product/option/offer mappings to membership plans or storage products.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload. It does not configure Google Play purchases or grant verifier authority by itself.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_046 : nat64;
            name = "set_google_play_config on dmsg_commerce";
            description = opt "Configure the Google Play application identity, environment, test-purchase acceptance, trusted verifier principals and product-to-plan or storage mappings.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { CriticalDappOperations };
                validator_canister_id = opt principal "uwist-sqaaa-aaaaj-qsjyq-cai";
                target_canister_id = opt principal "uwist-sqaaa-aaaaj-qsjyq-cai";
                validator_method_name = opt "validate_set_google_play_config";
                target_method_name = opt "set_google_play_config";
              }
            };
          }
        };
      }
    };
  },
)'
