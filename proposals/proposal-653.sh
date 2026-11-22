#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Generate the signed SNS proposal message locally for review.
payload=$(didc encode --format blob '(record {batch_id=102:nat; evidence=blob "\3d\f4\78\f3\90\19\50\7d\4e\ec\25\06\4a\8f\ca\ab\d9\8b\3a\c2\53\e2\95\0f\c8\de\3c\f7\ce\d9\b4\18"})')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Fix the return to dMsg after Internet Identity sign-in on Android\";
      url = \"https://dmsg.net/\";
      summary = \"Publish dmsg_frontend 3.5.0 asset batch 102 on 2fvu6-tqaaa-aaaap-akksa-cai. The new /_/redeem page previews the plan and duration for a membership code and lets the visitor open dMsg Desktop or copy the code for the CLI. Redemption requires confirmation in the client. File share and file request pages link to an offer when it is available. The membership comparison now lists the hosted Agent ID limit for each plan.

Batch evidence (SHA-256): 3df478f39019507d4eec25064a8fcaabd98b3ac253e2950fc8de3cf7ced9b418.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 1100 : nat64;
          payload = $payload;
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
