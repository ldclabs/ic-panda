export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({ 'min_confirmations' : IDL.Opt(IDL.Nat32) });
  const InitArgs = IDL.Record({
    'ecdsa_key_name' : IDL.Text,
    'chain' : IDL.Nat8,
    'prev_start_height' : IDL.Nat64,
    'prev_start_blockhash' : IDL.Text,
    'min_confirmations' : IDL.Nat32,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const RPCAgent = IDL.Record({
    'proxy_token' : IDL.Opt(IDL.Text),
    'api_token' : IDL.Opt(IDL.Text),
    'endpoint' : IDL.Text,
    'name' : IDL.Text,
    'max_cycles' : IDL.Nat64,
  });
  const Utxo = IDL.Record({
    'height' : IDL.Nat64,
    'value' : IDL.Nat64,
    'txid' : IDL.Vec(IDL.Nat8),
    'vout' : IDL.Nat32,
  });
  const CreateTxInput = IDL.Record({
    'from_subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'fee_rate' : IDL.Nat64,
    'address' : IDL.Text,
    'utxos' : IDL.Vec(Utxo),
    'amount' : IDL.Nat64,
  });
  const CreateTxOutput = IDL.Record({
    'tx' : IDL.Vec(IDL.Nat8),
    'fee' : IDL.Nat64,
    'tip_height' : IDL.Nat64,
    'instructions' : IDL.Nat64,
  });
  const Result_1 = IDL.Variant({ 'Ok' : CreateTxOutput, 'Err' : IDL.Text });
  const Result_2 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  const Result_3 = IDL.Variant({ 'Ok' : IDL.Nat64, 'Err' : IDL.Text });
  const State = IDL.Record({
    'start_height' : IDL.Nat64,
    'processed_height' : IDL.Nat64,
    'tip_height' : IDL.Nat64,
    'ecdsa_key_name' : IDL.Opt(IDL.Text),
    'managers' : IDL.Vec(IDL.Principal),
    'rpc_proxy_public_key' : IDL.Opt(IDL.Text),
    'chain' : IDL.Text,
    'confirmed_height' : IDL.Nat64,
    'unconfirmed_utxos' : IDL.Nat64,
    'unprocessed_blocks' : IDL.Nat64,
    'syncing_status' : IDL.Opt(IDL.Int8),
    'last_errors' : IDL.Vec(IDL.Text),
    'confirmed_utxs' : IDL.Nat64,
    'tip_blockhash' : IDL.Text,
    'unconfirmed_utxs' : IDL.Nat64,
    'min_confirmations' : IDL.Nat32,
    'processed_blockhash' : IDL.Text,
    'rpc_agents' : IDL.Vec(RPCAgent),
    'confirmed_utxos' : IDL.Nat64,
    'start_blockhash' : IDL.Text,
  });
  const Result_4 = IDL.Variant({ 'Ok' : State, 'Err' : IDL.Null });
  const BlockRef = IDL.Record({
    'height' : IDL.Nat64,
    'hash' : IDL.Vec(IDL.Nat8),
  });
  const Result_5 = IDL.Variant({ 'Ok' : BlockRef, 'Err' : IDL.Text });
  const TxStatus = IDL.Record({
    'height' : IDL.Nat64,
    'tip_height' : IDL.Nat64,
    'confirmed_height' : IDL.Nat64,
  });
  const UnspentTx = IDL.Record({
    'height' : IDL.Nat64,
    'output' : IDL.Vec(IDL.Vec(IDL.Nat8)),
    'spent' : IDL.Vec(IDL.Opt(IDL.Vec(IDL.Nat8))),
  });
  const Result_6 = IDL.Variant({ 'Ok' : UnspentTx, 'Err' : IDL.Text });
  const UtxosOutput = IDL.Record({
    'tip_height' : IDL.Nat64,
    'confirmed_height' : IDL.Nat64,
    'utxos' : IDL.Vec(Utxo),
    'tip_blockhash' : IDL.Vec(IDL.Nat8),
  });
  const Result_7 = IDL.Variant({ 'Ok' : UtxosOutput, 'Err' : IDL.Text });
  const SendTxInput = IDL.Record({
    'tx' : IDL.Vec(IDL.Nat8),
    'from_subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const SendTxOutput = IDL.Record({
    'tip_height' : IDL.Nat64,
    'txid' : IDL.Vec(IDL.Nat8),
    'instructions' : IDL.Nat64,
  });
  const Result_8 = IDL.Variant({ 'Ok' : SendTxOutput, 'Err' : IDL.Text });
  return IDL.Service({
    'admin_restart_syncing' : IDL.Func([IDL.Opt(IDL.Int8)], [Result], []),
    'admin_set_agent' : IDL.Func([IDL.Vec(RPCAgent)], [Result], []),
    'admin_set_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'api_version' : IDL.Func([], [IDL.Nat16], ['query']),
    'create_tx' : IDL.Func([CreateTxInput], [Result_1], []),
    'get_address' : IDL.Func([], [Result_2], ['query']),
    'get_balance' : IDL.Func([IDL.Text], [Result_3], ['query']),
    'get_balance_b' : IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Nat64], ['query']),
    'get_state' : IDL.Func([], [Result_4], ['query']),
    'get_tip' : IDL.Func([], [Result_5], ['query']),
    'get_tx_status' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [IDL.Opt(TxStatus)],
        ['query'],
      ),
    'get_utx' : IDL.Func([IDL.Text], [Result_6], ['query']),
    'get_utx_b' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [IDL.Opt(UnspentTx)],
        ['query'],
      ),
    'list_utxos' : IDL.Func(
        [IDL.Text, IDL.Nat16, IDL.Bool],
        [Result_7],
        ['query'],
      ),
    'list_utxos_b' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat16, IDL.Bool],
        [Result_7],
        ['query'],
      ),
    'send_tx' : IDL.Func([SendTxInput], [Result_8], []),
    'sign_and_send_tx' : IDL.Func([SendTxInput], [Result_8], []),
    'validate_admin_set_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({ 'min_confirmations' : IDL.Opt(IDL.Nat32) });
  const InitArgs = IDL.Record({
    'ecdsa_key_name' : IDL.Text,
    'chain' : IDL.Nat8,
    'prev_start_height' : IDL.Nat64,
    'prev_start_blockhash' : IDL.Text,
    'min_confirmations' : IDL.Nat32,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  return [IDL.Opt(ChainArgs)];
};
