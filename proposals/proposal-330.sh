#!/usr/bin/env bash

GOV_CID="dwv6s-6aaaa-aaaaq-aacta-cai"        # The governance canister ID.
NEURON_ID="\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd"  # The subaccount of the neuron you want to use to make the proposal.
FUNC_ID="2000"    # The generic function ID defined above.
POOL="5fq4w-lyaaa-aaaag-qjqta-cai"    # The ICPSwap pool you want to transfer the position.
TO_PRINCIPAL="xj4cn-sujbq-csa7q-lj5ru-bmavz-gk5ec-eg276-uyo6x-zmedq-rsaqn-nqe"   # The principal you want to transfer to.
POSITION_ID="12"    # The position ID you want to transfer. https://app.icpswap.com/info-tools/positions?pair=5fq4w-lyaaa-aaaag-qjqta-cai&principal=dwv6s-6aaaa-aaaaq-aacta-cai
URL="https://app.icpswap.com/liquidity/position/$POSITION_ID/$POOL"

TITLE="transfer the position $POSITION_ID to ICPanda DAO team account for maintaining the liquidity of ICPSwap pool $POOL"
SUMMARY="Team account $TO_PRINCIPAL will maintain the liquidity of ICPSwap pool $POOL. We will:\n1. Transfer the position $POSITION_ID to team account.\n2. Collect fees—temporarily held in the team account for team expenses and creating new liquidity pools.\n3. This position will be removed and recreated as it is out of the valid range.\n4. Transfer the position back to the governance account $GOV_CID."

BLOB="$(didc encode --format blob "(principal \"$GOV_CID\", principal \"$TO_PRINCIPAL\", $POSITION_ID:nat)")"

PROPOSAL="record { title=\"$TITLE\"; url=\"$URL\"; summary=\"$SUMMARY\"; action=opt variant {ExecuteGenericNervousSystemFunction = record {function_id=$FUNC_ID:nat64; payload=$BLOB;}}}"
DFX_ARGS="(record { subaccount=blob \"$NEURON_ID\"; command=opt variant { MakeProposal=$PROPOSAL};})"

echo $DFX_ARGS
# dfx canister --network ic call "$GOV_CID" manage_neuron "$DFX_ARGS"
