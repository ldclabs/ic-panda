// Generated from the public dmsg_handle.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const HandleInit = IDL.Record({
    'max_pending' : IDL.Nat32,
    'home_user' : IDL.Principal,
    'ledger' : IDL.Principal,
    'ledger_fee' : IDL.Nat,
  });
  const LegacySnapshot = IDL.Record({
    'event_tip' : IDL.Vec(IDL.Nat8),
    'source_canister' : IDL.Principal,
    'count' : IDL.Nat64,
    'entries_digest' : IDL.Vec(IDL.Nat8),
    'snapshot_id' : IDL.Vec(IDL.Nat8),
    'freeze_version' : IDL.Nat64,
  });
  const Error = IDL.Variant({
    'MigrationKeyUnavailable' : IDL.Null,
    'LegacyWriteDisabled' : IDL.Null,
    'InvalidInput' : IDL.Text,
    'RekeyRequired' : IDL.Null,
    'VersionConflict' : IDL.Null,
    'ExecutionUnknown' : IDL.Null,
    'IntegrityFailed' : IDL.Null,
    'NotFound' : IDL.Null,
    'FeeBlocked' : IDL.Null,
    'DeviceNotApproved' : IDL.Null,
    'Locked' : IDL.Null,
    'RecoveryIncomplete' : IDL.Null,
    'PolicyStale' : IDL.Null,
    'IdempotencyConflict' : IDL.Null,
    'UnsupportedProtocol' : IDL.Null,
    'Unavailable' : IDL.Text,
    'Forbidden' : IDL.Null,
    'ResultExpired' : IDL.Null,
    'Expired' : IDL.Null,
    'QuotaExceeded' : IDL.Null,
    'AuthRequired' : IDL.Null,
    'Pending' : IDL.Null,
  });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  const HandleAction = IDL.Variant({
    'AcceptTransfer' : IDL.Null,
    'Register' : IDL.Null,
    'Transfer' : IDL.Null,
    'ClaimLegacy' : IDL.Null,
  });
  const HandleIntent = IDL.Record({
    'handle_canister' : IDL.Principal,
    'action' : HandleAction,
    'subject' : IDL.Vec(IDL.Nat8),
    'op_id' : IDL.Vec(IDL.Nat8),
    'target_subject' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'handle' : IDL.Text,
    'expected_version' : IDL.Nat64,
    'terms_digest' : IDL.Vec(IDL.Nat8),
  });
  const HandleRecord = IDL.Record({
    'event_tip' : IDL.Vec(IDL.Nat8),
    'owner_subject' : IDL.Vec(IDL.Nat8),
    'version' : IDL.Nat64,
    'handle' : IDL.Text,
  });
  const Result_1 = IDL.Variant({ 'Ok' : HandleRecord, 'Err' : Error });
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Registration = IDL.Record({
    'fee' : IDL.Nat,
    'intent' : HandleIntent,
    'payer' : Account,
  });
  const HandlePhase = IDL.Variant({
    'Committed' : IDL.Null,
    'Reserved' : IDL.Null,
    'Paid' : IDL.Null,
    'RefundPending' : IDL.Null,
    'ChargeUnknown' : IDL.Null,
    'Charging' : IDL.Null,
    'Expired' : IDL.Null,
  });
  const HandleOperation = IDL.Record({
    'registration' : Registration,
    'memo' : IDL.Vec(IDL.Nat8),
    'ledger_block' : IDL.Opt(IDL.Nat64),
    'created_at' : IDL.Nat64,
    'digest' : IDL.Vec(IDL.Nat8),
    'phase' : HandlePhase,
    'amount' : IDL.Nat,
    'expires_at' : IDL.Nat64,
  });
  const Result_2 = IDL.Variant({ 'Ok' : HandleOperation, 'Err' : Error });
  const HandleEvent = IDL.Record({
    'at' : IDL.Nat64,
    'to' : IDL.Vec(IDL.Nat8),
    'previous' : IDL.Vec(IDL.Nat8),
    'from' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'version' : IDL.Nat64,
    'handle' : IDL.Text,
    'legacy_snapshot' : IDL.Vec(IDL.Nat8),
    'sequence' : IDL.Nat64,
  });
  const LegacyReservation = IDL.Record({
    'frozen_admins' : IDL.Vec(IDL.Principal),
    'handle' : IDL.Text,
    'legacy_name_principal' : IDL.Opt(IDL.Principal),
    'legacy_owner' : IDL.Principal,
    'quarantined' : IDL.Bool,
  });
  const Result_3 = IDL.Variant({
    'Ok' : IDL.Opt(LegacyReservation),
    'Err' : Error,
  });
  const SnapshotProgress = IDL.Record({
    'last_handle' : IDL.Opt(IDL.Text),
    'snapshot' : IDL.Opt(LegacySnapshot),
    'imported' : IDL.Nat64,
    'sealed' : IDL.Bool,
    'rolling_digest' : IDL.Vec(IDL.Nat8),
  });
  const Result_4 = IDL.Variant({ 'Ok' : SnapshotProgress, 'Err' : Error });
  const Result_5 = IDL.Variant({
    'Ok' : IDL.Vec(LegacyReservation),
    'Err' : Error,
  });
  const CertifiedEntry = IDL.Record({
    'key' : IDL.Vec(IDL.Nat8),
    'value' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'witness' : IDL.Vec(IDL.Nat8),
  });
  const CertifiedBatch = IDL.Record({
    'certificate' : IDL.Vec(IDL.Nat8),
    'schema' : IDL.Nat16,
    'entries' : IDL.Vec(CertifiedEntry),
    'canister' : IDL.Principal,
  });
  const Result_6 = IDL.Variant({ 'Ok' : CertifiedBatch, 'Err' : Error });
  return IDL.Service({
    'begin_legacy_snapshot' : IDL.Func([LegacySnapshot], [Result], []),
    'claim_legacy_handle' : IDL.Func(
        [HandleIntent, IDL.Vec(IDL.Nat8)],
        [Result_1],
        [],
      ),
    'commit_handle' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_2],
        [],
      ),
    'expire_handle_reservation' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result],
        [],
      ),
    'get_handle_event' : IDL.Func(
        [IDL.Nat64],
        [IDL.Opt(HandleEvent)],
        ['query'],
      ),
    'get_handle_operation' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_2],
        ['query'],
      ),
    'get_legacy_reservation' : IDL.Func([IDL.Text], [Result_3], ['query']),
    'import_legacy_handles' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(LegacyReservation)],
        [Result_4],
        [],
      ),
    'list_legacy_reservations' : IDL.Func(
        [IDL.Opt(IDL.Text)],
        [Result_5],
        ['query'],
      ),
    'reconcile_handle_charge' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_2],
        [],
      ),
    'reserve_handle' : IDL.Func([Registration], [Result_2], []),
    'resolve_handle_certified' : IDL.Func(
        [IDL.Vec(IDL.Text)],
        [Result_6],
        ['query'],
      ),
    'seal_legacy_snapshot' : IDL.Func([], [Result_4], []),
    'snapshot_certified' : IDL.Func([], [Result_6], ['query']),
    'snapshot_progress' : IDL.Func([], [SnapshotProgress], ['query']),
    'transfer_handle' : IDL.Func([HandleIntent, HandleIntent], [Result_1], []),
  });
};
export const init = ({ IDL }) => {
  const HandleInit = IDL.Record({
    'max_pending' : IDL.Nat32,
    'home_user' : IDL.Principal,
    'ledger' : IDL.Principal,
    'ledger_fee' : IDL.Nat,
  });
  return [HandleInit];
};
