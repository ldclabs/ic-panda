#!/usr/bin/env bash
set -euo pipefail

# Submit after proposal 574 has registered the dMsg canisters with SNS.
# Like proposals 522-540, this uses icp-cli to preserve the topic field.
# Running this script submits one proposal immediately; use an identity
# authorized to make proposals for the neuron below.
# Before execution proposals, grant SNS governance Commit on dmsg_connect.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Register commit_proposed_batch on dmsg_connect";
        url = "https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_connect/README.md";
        summary = "Registers generic function 3000 to call commit_proposed_batch on dmsg_connect (acjs3-cyaaa-aaaas-qg7da-cai), with validate_commit_proposed_batch on the same canister validating and rendering future execution payloads. Topic: ApplicationBusinessLogic.\n\nCommit a proposed asset batch whose evidence hash was approved, publishing the reviewed dMsg Connect files and response headers.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_000 : nat64;
            name = "commit_proposed_batch on dmsg_connect";
            description = opt "Commit a proposed asset batch whose evidence hash was approved, publishing the reviewed dMsg Connect files and response headers.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { ApplicationBusinessLogic };
                validator_canister_id = opt principal "acjs3-cyaaa-aaaas-qg7da-cai";
                target_canister_id = opt principal "acjs3-cyaaa-aaaas-qg7da-cai";
                validator_method_name = opt "validate_commit_proposed_batch";
                target_method_name = opt "commit_proposed_batch";
              }
            };
          }
        };
      }
    };
  },
)'
