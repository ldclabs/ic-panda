export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'preparers' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'committers' : IDL.Opt(IDL.Vec(IDL.Principal)),
  });
  const InitArgs = IDL.Record({
    'preparers' : IDL.Vec(IDL.Principal),
    'committers' : IDL.Vec(IDL.Principal),
  });
  const MinterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  const LinkLog = IDL.Record({
    'rewards' : IDL.Nat64,
    'linker' : IDL.Tuple(IDL.Principal, IDL.Principal),
    'minted_at' : IDL.Nat64,
  });
  const State = IDL.Record({
    'preparers' : IDL.Vec(IDL.Principal),
    'next_block_height' : IDL.Nat64,
    'minted_total' : IDL.Nat64,
    'committers' : IDL.Vec(IDL.Principal),
  });
  return IDL.Service({
    'get_block' : IDL.Func([IDL.Nat64], [IDL.Opt(LinkLog)], ['query']),
    'get_state' : IDL.Func([], [State], ['query']),
    'list_blocks' : IDL.Func(
        [IDL.Opt(IDL.Nat64), IDL.Opt(IDL.Nat64)],
        [IDL.Vec(LinkLog)],
        ['query'],
      ),
    'try_commit' : IDL.Func(
        [IDL.Principal, IDL.Principal],
        [IDL.Opt(IDL.Nat64)],
        [],
      ),
    'try_prepare' : IDL.Func([IDL.Principal, IDL.Principal], [IDL.Bool], []),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'preparers' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'committers' : IDL.Opt(IDL.Vec(IDL.Principal)),
  });
  const InitArgs = IDL.Record({
    'preparers' : IDL.Vec(IDL.Principal),
    'committers' : IDL.Vec(IDL.Principal),
  });
  const MinterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  return [IDL.Opt(MinterArgs)];
};
