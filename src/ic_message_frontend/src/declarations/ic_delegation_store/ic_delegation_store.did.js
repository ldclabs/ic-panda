export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'allowed_origins' : IDL.Opt(IDL.Vec(IDL.Text)),
  });
  const InitArgs = IDL.Record({ 'allowed_origins' : IDL.Vec(IDL.Text) });
  const CanisterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  return IDL.Service({});
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'allowed_origins' : IDL.Opt(IDL.Vec(IDL.Text)),
  });
  const InitArgs = IDL.Record({ 'allowed_origins' : IDL.Vec(IDL.Text) });
  const CanisterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  return [IDL.Opt(CanisterArgs)];
};
