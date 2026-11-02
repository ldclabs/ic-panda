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
        title = "Register schedule_policy on dmsg_commerce";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.5/src/dmsg_commerce/dmsg_commerce.did";
        summary = "Registers generic function 3033 to call schedule_policy on dmsg_commerce (uwist-sqaaa-aaaaj-qsjyq-cai), with validate_schedule_policy on the same canister validating and rendering future execution payloads. Topic: ApplicationBusinessLogic.\n\nSchedule a versioned subscription catalog with plan prices, resource limits, execution weights and storage products. Limits-only increases may take effect immediately under the existing policy checks.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_033 : nat64;
            name = "schedule_policy on dmsg_commerce";
            description = opt "Schedule a versioned subscription catalog with plan prices, resource limits, execution weights and storage products. Limits-only increases may take effect immediately under the existing policy checks.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { ApplicationBusinessLogic };
                validator_canister_id = opt principal "uwist-sqaaa-aaaaj-qsjyq-cai";
                target_canister_id = opt principal "uwist-sqaaa-aaaaj-qsjyq-cai";
                validator_method_name = opt "validate_schedule_policy";
                target_method_name = opt "schedule_policy";
              }
            };
          }
        };
      }
    };
  },
)'
