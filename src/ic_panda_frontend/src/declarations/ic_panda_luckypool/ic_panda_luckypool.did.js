export const idlFactory = ({ IDL }) => {
  const AddPrizeInputV2 = IDL.Record({
    'total_amount' : IDL.Nat32,
    'kind' : IDL.Opt(IDL.Nat8),
    'memo' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'recipient' : IDL.Opt(IDL.Principal),
    'quantity' : IDL.Nat16,
    'expire' : IDL.Nat16,
  });
  const PrizeOutput = IDL.Record({
    'id' : IDL.Vec(IDL.Nat8),
    'fee' : IDL.Nat64,
    'issued_at' : IDL.Nat64,
    'code' : IDL.Opt(IDL.Text),
    'kind' : IDL.Nat8,
    'memo' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Opt(IDL.Text),
    'refund_amount' : IDL.Nat64,
    'issuer' : IDL.Text,
    'filled' : IDL.Nat16,
    'quantity' : IDL.Nat16,
    'expire' : IDL.Nat64,
    'ended_at' : IDL.Nat64,
    'amount' : IDL.Nat64,
    'sys_subsidy' : IDL.Nat64,
  });
  const Result = IDL.Variant({ 'Ok' : PrizeOutput, 'Err' : IDL.Text });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const AirdropClaimInput = IDL.Record({
    'recaptcha' : IDL.Opt(IDL.Text),
    'challenge' : IDL.Text,
    'code' : IDL.Text,
    'lucky_code' : IDL.Opt(IDL.Text),
  });
  const AirdropStateOutput = IDL.Record({
    'lucky_code' : IDL.Opt(IDL.Text),
    'claimed' : IDL.Nat,
    'claimable' : IDL.Nat,
  });
  const Result_2 = IDL.Variant({ 'Ok' : AirdropStateOutput, 'Err' : IDL.Text });
  const AirdropCodeOutput = IDL.Record({
    'issued_at' : IDL.Nat64,
    'code' : IDL.Opt(IDL.Text),
    'name' : IDL.Opt(IDL.Text),
    'issuer' : IDL.Text,
    'filled' : IDL.Nat16,
    'quantity' : IDL.Nat16,
    'expire' : IDL.Nat64,
    'amount' : IDL.Nat64,
  });
  const AirdropLog = IDL.Record({
    'id' : IDL.Nat,
    'ts' : IDL.Nat64,
    'lucky_code' : IDL.Text,
    'caller' : IDL.Principal,
    'amount' : IDL.Nat,
  });
  const Result_3 = IDL.Variant({ 'Ok' : AirdropStateOutput, 'Err' : IDL.Null });
  const CaptchaOutput = IDL.Record({
    'challenge' : IDL.Text,
    'img_base64' : IDL.Text,
  });
  const Result_4 = IDL.Variant({ 'Ok' : CaptchaOutput, 'Err' : IDL.Text });
  const ClaimPrizeInput = IDL.Record({
    'challenge' : IDL.Vec(IDL.Nat8),
    'code' : IDL.Text,
  });
  const ClaimPrizeOutput = IDL.Record({
    'average' : IDL.Nat,
    'claimed' : IDL.Nat,
    'state' : AirdropStateOutput,
  });
  const Result_5 = IDL.Variant({ 'Ok' : ClaimPrizeOutput, 'Err' : IDL.Text });
  const AirdropHarvestInput = IDL.Record({
    'recaptcha' : IDL.Opt(IDL.Text),
    'amount' : IDL.Nat,
  });
  const LuckyDrawInput = IDL.Record({
    'icp' : IDL.Nat8,
    'amount' : IDL.Opt(IDL.Nat),
  });
  const LuckyDrawOutput = IDL.Record({
    'airdrop_cryptogram' : IDL.Opt(IDL.Text),
    'prize_cryptogram' : IDL.Opt(IDL.Text),
    'luckypool_empty' : IDL.Bool,
    'random' : IDL.Nat64,
    'amount' : IDL.Nat,
  });
  const Result_6 = IDL.Variant({ 'Ok' : LuckyDrawOutput, 'Err' : IDL.Text });
  const LuckyDrawLog = IDL.Record({
    'id' : IDL.Nat,
    'ts' : IDL.Nat64,
    'caller' : IDL.Principal,
    'random' : IDL.Nat64,
    'icp_amount' : IDL.Nat,
    'amount' : IDL.Nat,
  });
  const Notification = IDL.Record({
    'id' : IDL.Nat8,
    'level' : IDL.Nat8,
    'message' : IDL.Text,
    'dismiss' : IDL.Bool,
    'timeout' : IDL.Nat16,
  });
  const AddPrizeInput = IDL.Record({
    'claimable' : IDL.Nat32,
    'quantity' : IDL.Nat16,
    'expire' : IDL.Nat16,
  });
  const Result_7 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  const NameOutput = IDL.Record({
    'code' : IDL.Text,
    'name' : IDL.Text,
    'annual_fee' : IDL.Nat,
    'deposit' : IDL.Nat,
    'created_at' : IDL.Nat64,
  });
  const Result_8 = IDL.Variant({
    'Ok' : IDL.Opt(NameOutput),
    'Err' : IDL.Null,
  });
  const Result_9 = IDL.Variant({ 'Ok' : IDL.Principal, 'Err' : IDL.Text });
  const PrizeClaimLog = IDL.Record({
    'claimed_at' : IDL.Nat64,
    'prize' : PrizeOutput,
    'amount' : IDL.Nat,
  });
  const NameInput = IDL.Record({
    'name' : IDL.Text,
    'old_name' : IDL.Opt(IDL.Text),
  });
  const Result_10 = IDL.Variant({ 'Ok' : NameOutput, 'Err' : IDL.Text });
  const State = IDL.Record({
    'latest_luckydraw_logs' : IDL.Vec(LuckyDrawLog),
    'total_luckydraw' : IDL.Nat64,
    'latest_airdrop_logs' : IDL.Vec(AirdropLog),
    'managers' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'total_airdrop' : IDL.Nat64,
    'total_prize_count' : IDL.Opt(IDL.Nat64),
    'total_airdrop_count' : IDL.Nat64,
    'total_prize' : IDL.Opt(IDL.Nat64),
    'lucky_code' : IDL.Opt(IDL.Nat32),
    'airdrop_amount' : IDL.Opt(IDL.Nat64),
    'luckiest_luckydraw_logs' : IDL.Vec(LuckyDrawLog),
    'airdrop_balance' : IDL.Nat64,
    'total_luckydraw_count' : IDL.Nat64,
    'total_prizes_count' : IDL.Opt(IDL.Nat64),
    'prize_subsidy' : IDL.Opt(
      IDL.Tuple(IDL.Nat64, IDL.Nat16, IDL.Nat32, IDL.Nat8, IDL.Nat32, IDL.Nat16)
    ),
    'total_luckydraw_icp' : IDL.Nat64,
  });
  const Result_11 = IDL.Variant({ 'Ok' : State, 'Err' : IDL.Null });
  const Result_12 = IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text });
  const Result_13 = IDL.Variant({ 'Ok' : IDL.Principal, 'Err' : IDL.Null });
  return IDL.Service({
    'add_prize' : IDL.Func([AddPrizeInputV2], [Result], []),
    'admin_collect_icp' : IDL.Func([IDL.Nat], [Result_1], []),
    'admin_set_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result_1], []),
    'airdrop' : IDL.Func([AirdropClaimInput], [Result_2], []),
    'airdrop_codes_of' : IDL.Func(
        [IDL.Principal],
        [IDL.Vec(AirdropCodeOutput)],
        ['query'],
      ),
    'airdrop_logs' : IDL.Func(
        [IDL.Opt(IDL.Nat), IDL.Opt(IDL.Nat)],
        [IDL.Vec(AirdropLog)],
        ['query'],
      ),
    'airdrop_state_of' : IDL.Func(
        [IDL.Opt(IDL.Principal)],
        [Result_3],
        ['query'],
      ),
    'api_version' : IDL.Func([], [IDL.Nat16], ['query']),
    'captcha' : IDL.Func([], [Result_4], []),
    'claim_prize' : IDL.Func([ClaimPrizeInput], [Result_5], []),
    'harvest' : IDL.Func([AirdropHarvestInput], [Result_2], []),
    'luckydraw' : IDL.Func([LuckyDrawInput], [Result_6], []),
    'luckydraw_logs' : IDL.Func(
        [IDL.Opt(IDL.Nat), IDL.Opt(IDL.Nat)],
        [IDL.Vec(LuckyDrawLog)],
        ['query'],
      ),
    'manager_add_notification' : IDL.Func([Notification], [Result_1], []),
    'manager_add_prize' : IDL.Func([AddPrizeInput], [Result_7], []),
    'manager_add_prize_v2' : IDL.Func([AddPrizeInputV2], [Result_7], []),
    'manager_ban_users' : IDL.Func([IDL.Vec(IDL.Principal)], [Result_1], []),
    'manager_get_airdrop_key' : IDL.Func([], [Result_7], ['query']),
    'manager_remove_notifications' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_1],
        [],
      ),
    'manager_set_challenge_pub_key' : IDL.Func([IDL.Text], [Result_1], []),
    'manager_update_airdrop_amount' : IDL.Func([IDL.Nat64], [Result_1], []),
    'manager_update_airdrop_balance' : IDL.Func([IDL.Nat64], [Result_1], []),
    'manager_update_prize_subsidy' : IDL.Func(
        [
          IDL.Opt(
            IDL.Tuple(
              IDL.Nat64,
              IDL.Nat16,
              IDL.Nat32,
              IDL.Nat8,
              IDL.Nat32,
              IDL.Nat16,
            )
          ),
        ],
        [Result_1],
        [],
      ),
    'my_luckydraw_logs' : IDL.Func(
        [IDL.Opt(IDL.Nat), IDL.Opt(IDL.Nat)],
        [IDL.Vec(LuckyDrawLog)],
        ['query'],
      ),
    'name_lookup' : IDL.Func([IDL.Text], [Result_8], ['query']),
    'name_of' : IDL.Func([IDL.Opt(IDL.Principal)], [Result_8], ['query']),
    'notifications' : IDL.Func([], [IDL.Vec(Notification)], ['query']),
    'principal_by_luckycode' : IDL.Func([IDL.Text], [Result_9], ['query']),
    'prize' : IDL.Func([IDL.Text], [Result_2], []),
    'prize_claim_logs' : IDL.Func(
        [IDL.Principal, IDL.Opt(IDL.Nat), IDL.Opt(IDL.Nat)],
        [IDL.Vec(PrizeClaimLog)],
        ['query'],
      ),
    'prize_info' : IDL.Func(
        [IDL.Text, IDL.Opt(IDL.Principal)],
        [Result],
        ['query'],
      ),
    'prize_issue_logs' : IDL.Func(
        [IDL.Principal, IDL.Opt(IDL.Nat)],
        [IDL.Vec(PrizeOutput)],
        ['query'],
      ),
    'prize_ongoing' : IDL.Func([], [IDL.Vec(PrizeOutput)], ['query']),
    'prizes_of' : IDL.Func(
        [IDL.Opt(IDL.Principal)],
        [
          IDL.Vec(
            IDL.Tuple(
              IDL.Nat32,
              IDL.Nat32,
              IDL.Nat16,
              IDL.Nat32,
              IDL.Nat16,
              IDL.Nat16,
            )
          ),
        ],
        ['query'],
      ),
    'register_name' : IDL.Func([NameInput], [Result_10], []),
    'state' : IDL.Func([], [Result_11], ['query']),
    'unregister_name' : IDL.Func([NameInput], [Result_12], []),
    'update_name' : IDL.Func([NameInput], [Result_10], []),
    'validate_admin_collect_icp' : IDL.Func([IDL.Nat], [Result_1], []),
    'validate_admin_set_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_1],
        [],
      ),
    'whoami' : IDL.Func([], [Result_13], ['query']),
  });
};
export const init = ({ IDL }) => { return []; };
