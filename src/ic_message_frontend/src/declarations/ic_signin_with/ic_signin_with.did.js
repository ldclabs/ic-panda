export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'session_expires_in_ms' : IDL.Opt(IDL.Nat64),
    'governance_canister' : IDL.Opt(IDL.Principal),
  });
  const InitArgs = IDL.Record({
    'session_expires_in_ms' : IDL.Nat64,
    'governance_canister' : IDL.Opt(IDL.Principal),
  });
  const CanisterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const Delegation = IDL.Record({
    'pubkey' : IDL.Vec(IDL.Nat8),
    'targets' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'expiration' : IDL.Nat64,
  });
  const SignedDelegation = IDL.Record({
    'signature' : IDL.Vec(IDL.Nat8),
    'delegation' : Delegation,
  });
  const Result_1 = IDL.Variant({ 'Ok' : SignedDelegation, 'Err' : IDL.Text });
  const Result_2 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  const StateInfo = IDL.Record({
    'session_expires_in_ms' : IDL.Nat64,
    'governance_canister' : IDL.Opt(IDL.Principal),
    'statement' : IDL.Text,
    'domains' : IDL.Vec(IDL.Tuple(IDL.Text, IDL.Text)),
  });
  const Result_3 = IDL.Variant({ 'Ok' : StateInfo, 'Err' : IDL.Text });
  const Result_4 = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Nat8), 'Err' : IDL.Text });
  const SignInResponse = IDL.Record({
    'user_key' : IDL.Vec(IDL.Nat8),
    'seed' : IDL.Vec(IDL.Nat8),
    'expiration' : IDL.Nat64,
  });
  const Result_5 = IDL.Variant({ 'Ok' : SignInResponse, 'Err' : IDL.Text });
  const Result_6 = IDL.Variant({ 'Ok' : IDL.Principal, 'Err' : IDL.Text });
  return IDL.Service({
    'admin_remove_domain' : IDL.Func([IDL.Text], [Result], []),
    'admin_update_domain' : IDL.Func([IDL.Text, IDL.Text], [Result], []),
    'admin_update_statement' : IDL.Func([IDL.Text], [Result], []),
    'get_delegation' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_1],
        ['query'],
      ),
    'get_sign_in_with_ethereum_message' : IDL.Func(
        [IDL.Text, IDL.Text, IDL.Nat32, IDL.Nat64],
        [Result_2],
        ['query'],
      ),
    'get_sign_in_with_solana_message' : IDL.Func(
        [IDL.Text, IDL.Text, IDL.Nat64],
        [Result_2],
        ['query'],
      ),
    'info' : IDL.Func([], [Result_3], ['query']),
    'my_iv' : IDL.Func([], [Result_4], ['query']),
    'sign_in_with_ethereum' : IDL.Func(
        [
          IDL.Text,
          IDL.Text,
          IDL.Nat32,
          IDL.Nat64,
          IDL.Text,
          IDL.Vec(IDL.Nat8),
          IDL.Vec(IDL.Nat8),
          IDL.Vec(IDL.Nat8),
        ],
        [Result_5],
        [],
      ),
    'sign_in_with_solana' : IDL.Func(
        [
          IDL.Text,
          IDL.Text,
          IDL.Nat64,
          IDL.Text,
          IDL.Vec(IDL.Nat8),
          IDL.Vec(IDL.Nat8),
          IDL.Vec(IDL.Nat8),
        ],
        [Result_5],
        [],
      ),
    'validate_admin_remove_domain' : IDL.Func([IDL.Text], [Result_2], []),
    'validate_admin_update_domain' : IDL.Func(
        [IDL.Text, IDL.Text],
        [Result_2],
        [],
      ),
    'validate_admin_update_statement' : IDL.Func([IDL.Text], [Result_2], []),
    'verify_envelope' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Opt(IDL.Principal), IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_6],
        ['query'],
      ),
    'whoami' : IDL.Func([], [Result_6], ['query']),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'session_expires_in_ms' : IDL.Opt(IDL.Nat64),
    'governance_canister' : IDL.Opt(IDL.Principal),
  });
  const InitArgs = IDL.Record({
    'session_expires_in_ms' : IDL.Nat64,
    'governance_canister' : IDL.Opt(IDL.Principal),
  });
  const CanisterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  return [IDL.Opt(CanisterArgs)];
};
