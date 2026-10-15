#!/usr/bin/env bash

# quill can not support the topic field, so this calls manage_neuron with icp-cli and submits the
# proposal at once. Run it as an identity that controls the proposal neuron.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Add a generic function to drain the legacy ic_name_identity canister";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.0.0/src/ic_message_types/src/migration_canister.rs";
        summary = "Adds generic function 1129, which calls admin_legacy_drain on the legacy ic_name_identity canister (2rgax-kyaaa-aaaap-anvba-cai); validate_admin_legacy_drain on the same canister validates and renders each proposal. Topic: CriticalDappOperations.\n\nIt stops new business writes on the legacy ic_name_identity canister for the dMsg cutover; writes already in flight finish first and reads stay open. The payload is the 32-byte cutover ID.\n\nThe legacy dMsg canisters ic_message, ic_message_channel, ic_message_channel_02, ic_message_channel_03, ic_message_profile and ic_name_identity freeze in three steps for the migration to the new dMsg services: record the baseline review, drain, and seal once no write is in flight.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_129 : nat64;
                name = "`admin_legacy_drain` on ic_name_identity";
                description = opt "Stops new business writes on the legacy ic_name_identity canister for the dMsg cutover; writes already in flight finish first and reads stay open. The payload is the 32-byte cutover ID.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        topic = opt variant { CriticalDappOperations };
                        validator_canister_id = opt principal "2rgax-kyaaa-aaaap-anvba-cai";
                        target_canister_id = opt principal "2rgax-kyaaa-aaaap-anvba-cai";
                        validator_method_name = opt "validate_admin_legacy_drain";
                        target_method_name = opt "admin_legacy_drain";
                    }
                };
            }
        };
      }
    };
  },
)'
