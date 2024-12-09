export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'preparers': IDL.Opt(IDL.Vec(IDL.Principal)),
    'committers': IDL.Opt(IDL.Vec(IDL.Principal)),
  })
  const InitArgs = IDL.Record({
    'preparers': IDL.Vec(IDL.Principal),
    'committers': IDL.Vec(IDL.Principal),
  })
  const MinterArgs = IDL.Variant({
    'Upgrade': UpgradeArgs,
    'Init': InitArgs,
  })
  const LinkLog = IDL.Record({
    'rewards': IDL.Nat64,
    'linker': IDL.Tuple(IDL.Principal, IDL.Principal),
    'minted_at': IDL.Nat64,
  })
  const PublicTokenOverview = IDL.Record({
    'address': IDL.Text,
    'feesUSD': IDL.Float64,
    'id': IDL.Nat,
    'name': IDL.Text,
    'priceUSD': IDL.Float64,
    'priceUSDChange': IDL.Float64,
    'standard': IDL.Text,
    'symbol': IDL.Text,
    'totalVolumeUSD': IDL.Float64,
    'txCount': IDL.Int,
    'volumeUSD': IDL.Float64,
    'volumeUSD1d': IDL.Float64,
    'volumeUSD7d': IDL.Float64,
  })
  return IDL.Service({
    'getToken': IDL.Func([IDL.Text], [PublicTokenOverview], ['query']),
  })
}
