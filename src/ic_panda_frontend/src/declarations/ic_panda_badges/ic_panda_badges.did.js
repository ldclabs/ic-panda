export const idlFactory = ({ IDL }) => {
  return IDL.Service({ 'api_version' : IDL.Func([], [IDL.Nat16], ['query']) });
};
export const init = ({ IDL }) => { return []; };
