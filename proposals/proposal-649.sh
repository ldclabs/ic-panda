#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Write the signed proposal message to proposal-message.json for review and separate submission.
payload=$(didc encode --format blob '(record {batch_id=100:nat; evidence=blob "\b8\51\d7\aa\f4\5b\4f\b8\fd\f8\14\86\fc\b6\a3\2a\14\a5\d7\04\b6\2c\2d\71\80\5f\92\ce\10\f0\80\a6"})')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Publish Android 17 app association on dmsg.net\";
      url = \"https://dmsg.net/\";
      summary = \"Publish dmsg_frontend batch 100, evidence b851d7aaf45b4fb8fdf81486fcb6a32a14a5d704b62c2d71805f92ce10f080a6. The sole asset operation updates /.well-known/assetlinks.json for net.dmsg.android. It adds the two current Play hybrid signing certificates and retains the earlier signing certificate for older devices. The association authorizes App Links and passkey credential sharing.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 1100 : nat64;
          payload = ${payload};
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
