#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Generate the signed SNS proposal message locally for review.
payload=$(didc encode --format blob '(record {batch_id=104:nat; evidence=blob "\ee\7d\69\54\bf\d9\29\5e\bf\79\21\e5\ae\8b\57\8b\f9\74\c6\e7\a5\79\52\d8\8f\73\72\22\37\ff\d9\6f"})')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Prerender public dMsg pages and improve keyboard navigation\";
      url = \"https://dmsg.net/\";
      summary = \"Publish dmsg_frontend 3.5.2 asset batch 104 on 2fvu6-tqaaa-aaaap-akksa-cai. The homepage, legal and support pages, signature verification page and membership code page now ship as prerendered HTML. File sharing, file request and profile links open without briefly showing the homepage. The update also improves keyboard navigation and screen reader feedback, and loads interactive demos and Markdown previews on demand.

Batch evidence (SHA-256): ee7d6954bfd9295ebf7921e5ae8b578bf974c6e7a57952d88f73722237ffd96f.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 1100 : nat64;
          payload = $payload;
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
