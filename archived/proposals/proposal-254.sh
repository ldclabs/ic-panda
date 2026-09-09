#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Transfer 1250 ICP to Solidstate Team for Auditing of dMsg (ICPanda Message) App";
        url = "https://github.com/ldclabs/dmsg-auditing/blob/main/templates/scope.md";
        summary = "We (ICPanda DAO team) have developed https://dMsg.net, a decentralized end-to-end encrypted messaging application fully running on the Internet Computer blockchain.\n\nWe are seeking an audit for the application to ensure its security and reliability.\n\nGiven Solidstate’s expertise and experience in auditing Rust applications within the ICP ecosystem, we have engaged them to provide audit services. The agreed audit fee is 15,000 USDT * 110%, payable in ICP (approximately 1250 ICP at the current rate).\n\nPrevious audits by the Solidstate team can be referenced here: [ICLighthouse DAO Proposal 378](https://dashboard.internetcomputer.org/sns/hjcnr-bqaaa-aaaaq-aacka-cai/proposal/378).\n";
        action = opt variant {
            TransferSnsTreasuryFunds = record {
                from_treasury = 1 : int32;
                to_principal = opt principal "giw2o-uo43s-c4fef-txagm-3rbbq-pdimf-elsia-iar2z-ieznx-rwlsf-uqe";
                to_subaccount = null;
                memo = null;
                amount_e8s = 125_000_000_000 : nat64;
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json