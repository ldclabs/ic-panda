#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Submit after the production commerce catalog shows the v3 quotas from proposal 659.
payload=$(didc encode --format blob '(record {batch_id=105:nat; evidence=blob "\1e\84\22\d9\2f\d6\5e\29\35\7d\9d\cb\fd\11\22\16\6e\bb\57\f4\8c\94\7f\6e\1d\ca\f7\01\da\47\20\b1"})')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Update the dMsg website with catalog v3 membership limits\";
      url = \"https://dmsg.net/\";
      summary = \"Publish dmsg_frontend 3.5.3 asset batch 105 on 2fvu6-tqaaa-aaaap-akksa-cai. The website updates its membership comparison to match the quotas in proposal 659, scheduled for December 6, 2026 at 16:00 UTC.

The homepage and the English and Chinese reading guides show 256 MiB of encrypted cloud storage for Free accounts and a 100 MiB limit per Free cloud backup file. Both the active channel limit and the monthly execution allowance are 10 for Free, 100 for Plus, 1,000 for Pro and 10,000 for Max.

Batch evidence (SHA-256): 1e8422d92fd65e29357d9dcbfd1122166ebb57f48c947f6e1dcaf701da4720b1.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 1100 : nat64;
          payload = $payload;
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
