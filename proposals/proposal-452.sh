#!/usr/bin/env bash

# quill can not support topic field, we should use dfx canister call to send proposal:
dfx canister --network ic call dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Add a generic function to \"Commit proposed assets for upgrading the one_bridge_app canister\"";
        url = "https://internetcomputer.org/docs/current/developer-docs/daos/sns/managing/sns-asset-canister#sns-genericnervoussystemfunctions";
        summary = "Adding a new generic function that commit proposed assets for upgrading the one_bridge_app canister.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_300 : nat64;
                name = "Commit proposed assets for upgrading the one_bridge_app canister";
                description = opt "Commit proposed assets for upgrading the one_bridge_app canister.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        topic = opt variant { ApplicationBusinessLogic };
                        validator_canister_id = opt principal "ejwdq-iyaaa-aaaap-an47q-cai";
                        target_canister_id = opt principal "ejwdq-iyaaa-aaaap-an47q-cai";
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
