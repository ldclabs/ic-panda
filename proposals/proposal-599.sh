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
        title = "Register set_admission_pause on membership";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.5/src/membership/membership.did";
        summary = "Registers generic function 3024 to call set_admission_pause on membership (wmu2p-maaaa-aaaaj-qsjxq-cai), with validate_set_admission_pause on the same canister validating and rendering future execution payloads. Topic: ApplicationBusinessLogic.\n\nPause or resume new PANDA membership applications while existing claims continue qualification refresh and reconciliation.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_024 : nat64;
            name = "set_admission_pause on membership";
            description = opt "Pause or resume new PANDA membership applications while existing claims continue qualification refresh and reconciliation.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { ApplicationBusinessLogic };
                validator_canister_id = opt principal "wmu2p-maaaa-aaaaj-qsjxq-cai";
                target_canister_id = opt principal "wmu2p-maaaa-aaaaj-qsjxq-cai";
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
