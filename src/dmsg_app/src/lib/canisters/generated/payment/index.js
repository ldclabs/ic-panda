// Generated from the public dmsg_payment.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const ReceiptSigner = IDL.Record({
    'revoked' : IDL.Bool,
    'public_key' : IDL.Vec(IDL.Nat8),
    'epoch' : IDL.Nat64,
    'valid_until' : IDL.Nat64,
    'valid_from' : IDL.Nat64,
  });
  const PaymentInit = IDL.Record({
    'service_fee' : IDL.Nat,
    'daily_orders' : IDL.Nat32,
    'platform' : Account,
    'enabled' : IDL.Bool,
    'home_user' : IDL.Principal,
    'ledger' : IDL.Principal,
    'ledger_fee' : IDL.Nat,
    'signer' : ReceiptSigner,
    'max_open_per_payer' : IDL.Nat32,
    'max_fee' : IDL.Nat,
  });
  const FundsDecision = IDL.Variant({
    'SettlementCommitted' : IDL.Null,
    'RefundCommitted' : IDL.Null,
    'Pending' : IDL.Null,
  });
  const Quote = IDL.Record({
    'quote_scope' : IDL.Vec(IDL.Nat8),
    'fee_reserve' : IDL.Nat,
    'service_fee' : IDL.Nat,
    'max_network_fee' : IDL.Nat,
    'envelope_digest' : IDL.Vec(IDL.Nat8),
    'accept_by' : IDL.Nat64,
    'offer_digest' : IDL.Vec(IDL.Nat8),
    'recipient' : Account,
    'platform' : Account,
    'created_at' : IDL.Nat64,
    'ledger' : IDL.Principal,
    'quote_id' : IDL.Vec(IDL.Nat8),
    'max_bytes' : IDL.Nat32,
    'payer' : Account,
    'amount' : IDL.Nat,
    'home_payment' : IDL.Principal,
    'retain_ns' : IDL.Nat64,
    'recipient_net' : IDL.Nat,
    'signer_epoch' : IDL.Nat64,
    'fund_by' : IDL.Nat64,
  });
  const Escrow = IDL.Record({
    'next_leg' : IDL.Nat64,
    'liabilities' : IDL.Nat,
    'decision' : FundsDecision,
    'pending_payouts' : IDL.Nat32,
    'op_id' : IDL.Vec(IDL.Nat8),
    'subaccount' : IDL.Vec(IDL.Nat8),
    'network_fees' : IDL.Nat,
    'quote' : Quote,
    'transferred' : IDL.Nat,
    'funded_at' : IDL.Opt(IDL.Nat64),
    'version' : IDL.Nat64,
    'quote_digest' : IDL.Vec(IDL.Nat8),
    'payer_principal' : IDL.Principal,
    'receipt_digest' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'primary_remaining' : IDL.Nat,
    'escrow_id' : IDL.Vec(IDL.Nat8),
    'funding_ref' : IDL.Opt(IDL.Nat64),
    'confirmed_in' : IDL.Nat,
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
  const Result = IDL.Variant({ 'Ok' : Escrow, 'Err' : Error });
  const LegStatus = IDL.Variant({
    'Superseded' : IDL.Null,
    'FeeBlocked' : IDL.Null,
    'Rejected' : IDL.Null,
    'Succeeded' : IDL.Null,
    'InFlight' : IDL.Null,
    'Unknown' : IDL.Null,
    'Pending' : IDL.Null,
  });
  const LegKind = IDL.Variant({
    'Refund' : IDL.Record({ 'funding_block' : IDL.Nat64 }),
    'Platform' : IDL.Null,
    'ReserveRefund' : IDL.Null,
    'Recipient' : IDL.Null,
  });
  const TransferLeg = IDL.Record({
    'to' : Account,
    'fee' : IDL.Nat,
    'status' : LegStatus,
    'replaces' : IDL.Opt(IDL.Nat64),
    'history_digest' : IDL.Vec(IDL.Nat8),
    'kind' : LegKind,
    'memo' : IDL.Vec(IDL.Nat8),
    'leg_id' : IDL.Nat64,
    'block' : IDL.Opt(IDL.Nat64),
    'created_at_time' : IDL.Nat64,
    'escrow_id' : IDL.Vec(IDL.Nat8),
    'revision' : IDL.Nat64,
    'amount' : IDL.Nat,
    'expected_fee' : IDL.Opt(IDL.Nat),
  });
  const Result_1 = IDL.Variant({ 'Ok' : TransferLeg, 'Err' : Error });
  const AdmissionReceipt = IDL.Record({
    'protocol' : IDL.Nat16,
    'retain_until' : IDL.Nat64,
    'relay_id' : IDL.Vec(IDL.Nat8),
    'envelope_digest' : IDL.Vec(IDL.Nat8),
    'size' : IDL.Nat32,
    'accept_by' : IDL.Nat64,
    'admission_seq' : IDL.Nat64,
    'quote_digest' : IDL.Vec(IDL.Nat8),
    'stored_at' : IDL.Nat64,
    'policy_version' : IDL.Nat64,
    'escrow_id' : IDL.Vec(IDL.Nat8),
    'home_payment' : IDL.Principal,
    'signer_epoch' : IDL.Nat64,
    'fund_by' : IDL.Nat64,
    'inbox_key_version' : IDL.Nat64,
  });
  const SignedReceipt = IDL.Record({
    'signature' : IDL.Vec(IDL.Nat8),
    'receipt' : AdmissionReceipt,
  });
  const Deposit = IDL.Record({
    'committed_at' : IDL.Nat64,
    'from' : Account,
    'refundable' : IDL.Nat,
    'block' : IDL.Nat64,
    'amount' : IDL.Nat,
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
  const Result_2 = IDL.Variant({ 'Ok' : CertifiedBatch, 'Err' : Error });
  const Result_3 = IDL.Variant({ 'Ok' : ReceiptSigner, 'Err' : Error });
  const Result_4 = IDL.Variant({ 'Ok' : IDL.Vec(Escrow), 'Err' : Error });
  const Result_5 = IDL.Variant({ 'Ok' : IDL.Vec(TransferLeg), 'Err' : Error });
  const PaymentOffer = IDL.Record({
    'quote_scope' : IDL.Vec(IDL.Nat8),
    'issued_at' : IDL.Nat64,
    'subject' : IDL.Vec(IDL.Nat8),
    'recipient' : Account,
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'version' : IDL.Nat64,
    'ledger' : IDL.Principal,
    'offer_id' : IDL.Vec(IDL.Nat8),
    'home_payment' : IDL.Principal,
    'expires_at' : IDL.Nat64,
    'recipient_net' : IDL.Nat,
  });
  const SignedOffer = IDL.Record({
    'signature' : IDL.Vec(IDL.Nat8),
    'offer' : PaymentOffer,
  });
  const OpenEscrow = IDL.Record({
    'quote_signature' : IDL.Vec(IDL.Nat8),
    'offer' : SignedOffer,
    'op_id' : IDL.Vec(IDL.Nat8),
    'quote' : Quote,
  });
  const Result_6 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  return IDL.Service({
    'check_funding' : IDL.Func([IDL.Vec(IDL.Nat8), IDL.Nat64], [Result], []),
    'claim_deposit_refund' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_1],
        [],
      ),
    'claim_fee_reserve' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_1], []),
    'expiry_refund' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'finalize_receipt' : IDL.Func([SignedReceipt], [Result], []),
    'get_deposit' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64],
        [IDL.Opt(Deposit)],
        ['query'],
      ),
    'get_escrow' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], ['query']),
    'get_escrow_by_operation' : IDL.Func(
        [IDL.Principal, IDL.Vec(IDL.Nat8)],
        [Result],
        ['query'],
      ),
    'get_escrow_certified' : IDL.Func(
        [IDL.Vec(IDL.Vec(IDL.Nat8))],
        [Result_2],
        ['query'],
      ),
    'get_receipt_signer' : IDL.Func([IDL.Nat64], [Result_3], ['query']),
    'get_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_1],
        ['query'],
      ),
    'list_my_escrows' : IDL.Func(
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_4],
        ['query'],
      ),
    'list_transfers' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Opt(IDL.Nat64)],
        [Result_5],
        ['query'],
      ),
    'open_escrow' : IDL.Func([OpenEscrow], [Result], []),
    'process_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_1],
        [],
      ),
    'reconcile_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64, IDL.Nat64],
        [Result_1],
        [],
      ),
    'revise_rejected_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64, IDL.Nat],
        [Result_1],
        [],
      ),
    'revoke_receipt_signer' : IDL.Func([IDL.Nat64], [Result_6], []),
    'rotate_receipt_signer' : IDL.Func([ReceiptSigner], [Result_6], []),
    'set_orders_enabled' : IDL.Func([IDL.Bool], [Result_6], []),
  });
};
export const init = ({ IDL }) => {
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const ReceiptSigner = IDL.Record({
    'revoked' : IDL.Bool,
    'public_key' : IDL.Vec(IDL.Nat8),
    'epoch' : IDL.Nat64,
    'valid_until' : IDL.Nat64,
    'valid_from' : IDL.Nat64,
  });
  const PaymentInit = IDL.Record({
    'service_fee' : IDL.Nat,
    'daily_orders' : IDL.Nat32,
    'platform' : Account,
    'enabled' : IDL.Bool,
    'home_user' : IDL.Principal,
    'ledger' : IDL.Principal,
    'ledger_fee' : IDL.Nat,
    'signer' : ReceiptSigner,
    'max_open_per_payer' : IDL.Nat32,
    'max_fee' : IDL.Nat,
  });
  return [PaymentInit];
};
