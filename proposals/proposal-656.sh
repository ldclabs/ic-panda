#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Submit only after registration of function 3047 has executed.
# set_promo_config replaces the whole verifier list. Recheck get_promo_config before submission.
verifier="jn5em-edog7-dgoec-sfrqi-mqn7f-3kuf7-y2dvc-e5qbt-ahi7q-i54ah-2qe"
config="(record { enabled = true; verifiers = vec { principal \"$verifier\" } })"
payload=$(didc encode --defs misc/canisters/canisters-v3.1.12/dmsg_commerce.did \
  --method set_promo_config --format blob "$config")

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Enable dMsg membership grants and authorize the production verifier\";
      url = \"https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.12/src/dmsg_commerce/dmsg_commerce.did\";
      summary = \"Set enabled to true and replace the empty verifier list on dmsg_commerce (uwist-sqaaa-aaaaj-qsjyq-cai) with one principal: $verifier. This authorizes the verifier to submit promotional and compensation grants for 30 or 365 days of Plus, Pro or Max membership, check guest-pass issuer eligibility, retrieve grant receipts and revoke issued grants. Each membership grant is submitted separately.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 3_047 : nat64;
          payload = ${payload};
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
