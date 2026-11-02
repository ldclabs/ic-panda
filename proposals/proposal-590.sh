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
        title = "Register admin_set_limits on dmsg_payment";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.5/src/dmsg_payment/dmsg_payment.did";
        summary = "Registers generic function 3015 to call admin_set_limits on dmsg_payment (urjuh-7iaaa-aaaaj-qsjya-cai), with validate_admin_set_limits on the same canister validating and rendering future execution payloads. Topic: ApplicationBusinessLogic.\n\nReplace the paid-message admission, escrow capacity and ledger-call limits. Existing escrows and recovery operations remain available.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_015 : nat64;
            name = "admin_set_limits on dmsg_payment";
            description = opt "Replace the paid-message admission, escrow capacity and ledger-call limits. Existing escrows and recovery operations remain available.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { ApplicationBusinessLogic };
                validator_canister_id = opt principal "urjuh-7iaaa-aaaaj-qsjya-cai";
                target_canister_id = opt principal "urjuh-7iaaa-aaaaj-qsjya-cai";
                validator_method_name = opt "validate_admin_set_limits";
                target_method_name = opt "admin_set_limits";
              }
            };
          }
        };
      }
    };
  },
)'
