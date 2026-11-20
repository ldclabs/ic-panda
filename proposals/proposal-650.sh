#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Write the signed proposal message to proposal-message.json for review and separate submission.
payload=$(didc encode --format blob '(record {batch_id=6:nat; evidence=blob "\a2\04\9f\13\59\2f\fd\9b\e2\9c\fc\f8\ee\e1\37\2a\cd\50\4a\35\d5\13\b5\64\9c\5c\42\fb\ad\c0\8e\a3"})')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Publish Android 17 app association on connect.dmsg.net\";
      url = \"https://connect.dmsg.net/\";
      summary = \"Publish dmsg_connect batch 6, evidence a2049f13592ffd9be29cfcf8eee1372acd504a35d513b5649c5c42fbadc08ea3. The sole asset operation updates /.well-known/assetlinks.json for net.dmsg.android. It adds the two current Play hybrid signing certificates and retains the earlier signing certificate for older devices. The association authorizes verified App Links for pairing and requests.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 3000 : nat64;
          payload = ${payload};
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
