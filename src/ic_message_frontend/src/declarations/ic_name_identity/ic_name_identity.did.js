export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'session_expires_in_ms' : IDL.Opt(IDL.Nat64),
    'name' : IDL.Opt(IDL.Text),
  });
  const InitArgs = IDL.Record({
    'session_expires_in_ms' : IDL.Nat64,
    'name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  const Delegator = IDL.Record({
    'owner' : IDL.Principal,
    'role' : IDL.Int8,
    'sign_in_at' : IDL.Nat64,
  });
  const Result = IDL.Variant({ 'Ok' : IDL.Vec(Delegator), 'Err' : IDL.Text });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const Delegation = IDL.Record({
    'pubkey' : IDL.Vec(IDL.Nat8),
    'targets' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'expiration' : IDL.Nat64,
  });
  const SignedDelegation = IDL.Record({
    'signature' : IDL.Vec(IDL.Nat8),
    'delegation' : Delegation,
  });
  const Result_2 = IDL.Variant({ 'Ok' : SignedDelegation, 'Err' : IDL.Text });
  const NameAccount = IDL.Record({
    'name' : IDL.Text,
    'account' : IDL.Principal,
  });
  const Result_3 = IDL.Variant({
    'Ok' : IDL.Vec(NameAccount),
    'Err' : IDL.Text,
  });
  const Result_4 = IDL.Variant({ 'Ok' : IDL.Principal, 'Err' : IDL.Text });
  const State = IDL.Record({
    'session_expires_in_ms' : IDL.Nat64,
    'name' : IDL.Text,
    'sign_in_count' : IDL.Nat64,
  });
  const Result_5 = IDL.Variant({ 'Ok' : State, 'Err' : IDL.Text });
  const SignInResponse = IDL.Record({
    'user_key' : IDL.Vec(IDL.Nat8),
    'seed' : IDL.Vec(IDL.Nat8),
    'expiration' : IDL.Nat64,
  });
  const Result_6 = IDL.Variant({ 'Ok' : SignInResponse, 'Err' : IDL.Text });
  const Result_7 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  return IDL.Service({
    'activate_name' : IDL.Func([IDL.Text], [Result], []),
    'add_delegator' : IDL.Func(
        [IDL.Text, IDL.Principal, IDL.Int8],
        [Result],
        [],
      ),
    'admin_reset_name' : IDL.Func(
        [IDL.Text, IDL.Vec(IDL.Principal)],
        [Result_1],
        [],
      ),
    'get_delegation' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_2],
        ['query'],
      ),
    'get_delegators' : IDL.Func([IDL.Text], [Result], ['query']),
    'get_my_accounts' : IDL.Func([], [Result_3], ['query']),
    'get_principal' : IDL.Func([IDL.Text], [Result_4], ['query']),
    'get_state' : IDL.Func([], [Result_5], ['query']),
    'leave_delegation' : IDL.Func([IDL.Text], [Result_1], []),
    'remove_delegator' : IDL.Func([IDL.Text, IDL.Principal], [Result_1], []),
    'sign_in' : IDL.Func(
        [IDL.Text, IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_6],
        [],
      ),
    'validate_admin_reset_name' : IDL.Func(
        [IDL.Text, IDL.Vec(IDL.Principal)],
        [Result_7],
        [],
      ),
    'whoami' : IDL.Func([], [Result_4], ['query']),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'session_expires_in_ms' : IDL.Opt(IDL.Nat64),
    'name' : IDL.Opt(IDL.Text),
  });
  const InitArgs = IDL.Record({
    'session_expires_in_ms' : IDL.Nat64,
    'name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  return [IDL.Opt(ChainArgs)];
};
