# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "BUIDL & HODL - PANDA is great!";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposals";
        summary = "This is a motion proposal to see if people agree on the fact that the ICPanda is great.";
        action = opt variant {
            Motion = record {
                motion_text = "We are building ICPanda";
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json