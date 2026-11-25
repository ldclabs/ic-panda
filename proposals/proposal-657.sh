#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Generate the signed SNS proposal message locally for review.
payload=$(didc encode --format blob '(record {batch_id=103:nat; evidence=blob "\23\86\f8\6d\d6\9c\a3\36\df\16\9c\99\55\ca\c2\b4\41\07\47\f4\45\88\44\3f\a7\dc\b5\c2\8d\4f\84\66"})')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Publish the dMsg account deletion guide\";
      url = \"https://dmsg.net/\";
      summary = \"Publish dmsg_frontend 3.5.1 asset batch 103 on 2fvu6-tqaaa-aaaap-akksa-cai. The new /_/delete-account guide explains how to request account deletion in the mobile app or by email after uninstalling it, how account ownership is verified, which records are retained and how to cancel store subscriptions separately. The guide is available in English and Simplified Chinese, can be read without JavaScript and is linked from the support and privacy pages.

Batch evidence (SHA-256): 2386f86dd69ca336df169c9955cac2b4410747f44588443fa7dcb5c28d4f8466.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 1100 : nat64;
          payload = $payload;
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
