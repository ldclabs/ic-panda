export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'ledger_canister' : IDL.Opt(IDL.Principal),
    'chain_canister' : IDL.Opt(IDL.Principal),
  });
  const InitArgs = IDL.Record({
    'ecdsa_key_name' : IDL.Text,
    'chain' : IDL.Nat8,
    'ledger_canister' : IDL.Opt(IDL.Principal),
    'chain_canister' : IDL.Opt(IDL.Principal),
  });
  const MinterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const BurnInput = IDL.Record({
    'fee_rate' : IDL.Nat64,
    'address' : IDL.Text,
    'amount' : IDL.Nat64,
  });
  const BurnOutput = IDL.Record({
    'tip_height' : IDL.Nat64,
    'block_index' : IDL.Nat64,
    'txid' : IDL.Vec(IDL.Nat8),
    'instructions' : IDL.Nat64,
  });
  const Result_1 = IDL.Variant({ 'Ok' : BurnOutput, 'Err' : IDL.Text });
  const Result_2 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  const State = IDL.Record({
    'tokens_minted_count' : IDL.Nat64,
    'ecdsa_key_name' : IDL.Opt(IDL.Text),
    'managers' : IDL.Vec(IDL.Principal),
    'burning_utxos' : IDL.Vec(
      IDL.Tuple(
        IDL.Nat64,
        IDL.Tuple(
          IDL.Principal,
          IDL.Vec(IDL.Nat8),
          IDL.Nat64,
          IDL.Nat64,
          IDL.Text,
        ),
      )
    ),
    'chain' : IDL.Text,
    'tokens_burned_count' : IDL.Nat64,
    'collected_utxos' : IDL.Nat64,
    'tokens_burned' : IDL.Nat64,
    'accounts' : IDL.Nat64,
    'minter_subaddress' : IDL.Opt(IDL.Text),
    'ledger_canister' : IDL.Opt(IDL.Principal),
    'minter_address' : IDL.Opt(IDL.Text),
    'burned_utxos' : IDL.Nat64,
    'chain_canister' : IDL.Opt(IDL.Principal),
    'tokens_minted' : IDL.Nat64,
  });
  const Result_3 = IDL.Variant({ 'Ok' : State, 'Err' : IDL.Null });
  const Utxo = IDL.Record({
    'height' : IDL.Nat64,
    'value' : IDL.Nat64,
    'txid' : IDL.Vec(IDL.Nat8),
    'vout' : IDL.Nat32,
  });
  const BurnedUtxos = IDL.Record({
    'height' : IDL.Nat64,
    'block_index' : IDL.Nat64,
    'txid' : IDL.Vec(IDL.Nat8),
    'address' : IDL.Vec(IDL.Nat8),
    'utxos' : IDL.Vec(Utxo),
  });
  const CollectedUtxo = IDL.Record({
    'height' : IDL.Nat64,
    'principal' : IDL.Principal,
    'block_index' : IDL.Nat64,
    'utxo' : Utxo,
  });
  const MintedUtxo = IDL.Record({
    'block_index' : IDL.Nat64,
    'utxo' : Utxo,
    'minted_at' : IDL.Nat64,
  });
  const Result_4 = IDL.Variant({
    'Ok' : IDL.Vec(MintedUtxo),
    'Err' : IDL.Text,
  });
  const MintOutput = IDL.Record({
    'instructions' : IDL.Nat64,
    'amount' : IDL.Nat64,
  });
  const Result_5 = IDL.Variant({ 'Ok' : MintOutput, 'Err' : IDL.Text });
  return IDL.Service({
    'admin_set_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'api_version' : IDL.Func([], [IDL.Nat16], ['query']),
    'burn_ckdoge' : IDL.Func([BurnInput], [Result_1], []),
    'get_address' : IDL.Func([], [Result_2], ['query']),
    'get_state' : IDL.Func([], [Result_3], ['query']),
    'list_burned_utxos' : IDL.Func(
        [IDL.Nat64, IDL.Nat16],
        [IDL.Vec(BurnedUtxos)],
        ['query'],
      ),
    'list_collected_utxos' : IDL.Func(
        [IDL.Nat64, IDL.Nat16],
        [IDL.Vec(CollectedUtxo)],
        ['query'],
      ),
    'list_minted_utxos' : IDL.Func(
        [IDL.Opt(IDL.Principal)],
        [Result_4],
        ['query'],
      ),
    'mint_ckdoge' : IDL.Func([], [Result_5], []),
    'retry_burn_ckdoge' : IDL.Func(
        [IDL.Nat64, IDL.Opt(IDL.Nat64)],
        [Result_1],
        [],
      ),
    'validate_admin_set_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'ledger_canister' : IDL.Opt(IDL.Principal),
    'chain_canister' : IDL.Opt(IDL.Principal),
  });
  const InitArgs = IDL.Record({
    'ecdsa_key_name' : IDL.Text,
    'chain' : IDL.Nat8,
    'ledger_canister' : IDL.Opt(IDL.Principal),
    'chain_canister' : IDL.Opt(IDL.Principal),
  });
  const MinterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  return [IDL.Opt(MinterArgs)];
};
