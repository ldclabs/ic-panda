#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Generate the signed SNS proposal message locally for review.
payload=$(didc encode --format blob '(record {batch_id=101:nat; evidence=blob "\36\76\50\9f\a9\ad\87\6c\dc\db\ec\0c\4f\77\a8\3c\a7\61\cd\93\32\a6\e4\3c\8c\fe\01\d5\ef\b6\87\b1"})')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Fix the return to dMsg after Internet Identity sign-in on Android\";
      url = \"https://dmsg.net/\";
      summary = \"Publish dmsg_frontend asset batch 101 on 2fvu6-tqaaa-aaaap-akksa-cai. Chrome can keep the original login tab in the background while the Internet Identity popup returns its response. The login page now holds that response in memory until the user taps Return to the dMsg app. This opens the verified HTTPS callback from the foreground tab. Leaving the login page discards the response. A Pixel 7a running Android 17, Chrome 158 and the Google Play release 0.1.3 (4) completed a real II sign-in with the proposed login page. The build also includes the previously committed change that names newly created browser passkeys using the local creation time in dMsg-MM-DD-HHmm format. Source commit 4331f86fa6754a474ada307f136faa4e242048da. Batch evidence 3676509fa9ad876cdcdbec0c4f77a83ca761cd9332a6e43c8cfe01d5efb687b1.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 1100 : nat64;
          payload = $payload;
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
