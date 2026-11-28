#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

payload=$(didc encode --defs src/dmsg_commerce/dmsg_commerce.did \
  --method schedule_policy --format blob < proposals/dmsg-catalog-v3.did)

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Raise dMsg plan quotas and update the catalog terms digest\";
      url = \"https://dmsg.net/\";
      summary = \"Schedule catalog v3 for December 6, 2026 at 16:00 UTC on dmsg_commerce (uwist-sqaaa-aaaaj-qsjyq-cai). This raises total encrypted cloud storage for Free accounts from 100 MiB to 256 MiB (268,435,456 bytes). Active channels and monthly execution units each rise from 5 to 10 for Free, 20 to 100 for Plus, and 100 to 1,000 for Pro. Max provides 10,000 of each. New and existing accounts receive the new limits on refresh after activation. Execution allowances for the activation month are prorated. Catalog v3 records the SHA-256 of the published v1 service terms at https://dmsg.net/_/terms/v1.txt (53fc118e607cf750fbb271b43d6c14701c45628bec5413b34cee4966be1a448a). Each Free cloud backup can contain up to 100 MiB per file.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 3_033 : nat64;
          payload = $payload;
        };
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
