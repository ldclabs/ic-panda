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
        title = "Register set_admission_pause on dmsg_commerce";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.5/src/dmsg_commerce/dmsg_commerce.did";
        summary = "Registers generic function 3034 to call set_admission_pause on dmsg_commerce (uwist-sqaaa-aaaaj-qsjyq-cai), with validate_set_admission_pause on the same canister validating and rendering future execution payloads. Topic: ApplicationBusinessLogic.\n\nPause or resume new cash checkouts. Reconciliation, refunds, existing entitlement maintenance and PANDA membership continue.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_034 : nat64;
            name = "set_admission_pause on dmsg_commerce";
            description = opt "Pause or resume new cash checkouts. Reconciliation, refunds, existing entitlement maintenance and PANDA membership continue.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { ApplicationBusinessLogic };
                validator_canister_id = opt principal "uwist-sqaaa-aaaaj-qsjyq-cai";
                target_canister_id = opt principal "uwist-sqaaa-aaaaj-qsjyq-cai";
                validator_method_name = opt "validate_set_admission_pause";
                target_method_name = opt "set_admission_pause";
              }
            };
          }
        };
      }
    };
  },
)'
