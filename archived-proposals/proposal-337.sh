#!/usr/bin/env bash

GOV_CID="dwv6s-6aaaa-aaaaq-aacta-cai"
NEURON_ID="\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd"
dfx canister --ic call ${GOV_CID} manage_neuron '(
    record {
        subaccount = blob "'${NEURON_ID}'";
        command = opt variant {
            MakeProposal = record {
                url = "https://forum.dfinity.org/t/sns-topics-plan";
                title = "Set topics for custom SNS proposals";
                action = opt variant {
                    SetTopicsForCustomProposals = record {
                        custom_function_id_to_topic = vec {
                            record {
                                1000 : nat64;
                                variant { ApplicationBusinessLogic };
                            };
                            record {
                                1005 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1006 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1100 : nat64;
                                variant { ApplicationBusinessLogic };
                            };
                            record {
                                1101 : nat64;
                                variant { ApplicationBusinessLogic };
                            };
                            record {
                                1102 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1103 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1104 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1105 : nat64;
                                variant { ApplicationBusinessLogic };
                            };
                            record {
                                1106 : nat64;
                                variant { ApplicationBusinessLogic };
                            };
                            record {
                                1107 : nat64;
                                variant { ApplicationBusinessLogic };
                            };
                            record {
                                1108 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1109 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1110 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1111 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1112 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1113 : nat64;
                                variant { DaoCommunitySettings };
                            };
                            record {
                                1114 : nat64;
                                variant { ApplicationBusinessLogic };
                            };
                            record {
                                1115 : nat64;
                                variant { ApplicationBusinessLogic };
                            };
                            record {
                                1116 : nat64;
                                variant { DaoCommunitySettings };
                            };
                        };
                    }
                };
                summary = "Set topics ApplicationBusinessLogic and DaoCommunitySettings for SNS proposals.";
            }
        };
    },
)'