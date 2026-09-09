// Generated from the public dmsg_payment.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export interface Account {
  'owner' : Principal,
  'subaccount' : [] | [Uint8Array | number[]],
}
export interface AdmissionReceipt {
  'protocol' : number,
  'retain_until' : bigint,
  'relay_id' : Uint8Array | number[],
  'envelope_digest' : Uint8Array | number[],
  'size' : number,
  'accept_by' : bigint,
  'admission_seq' : bigint,
  'quote_digest' : Uint8Array | number[],
  'stored_at' : bigint,
  'policy_version' : bigint,
  'escrow_id' : Uint8Array | number[],
  'home_payment' : Principal,
  'signer_epoch' : bigint,
  'fund_by' : bigint,
  'inbox_key_version' : bigint,
}
export interface CertifiedBatch {
  'certificate' : Uint8Array | number[],
  'schema' : number,
  'entries' : Array<CertifiedEntry>,
  'canister' : Principal,
}
export interface CertifiedEntry {
  'key' : Uint8Array | number[],
  'value' : [] | [Uint8Array | number[]],
  'witness' : Uint8Array | number[],
}
export interface Deposit {
  'committed_at' : bigint,
  'from' : Account,
  'refundable' : bigint,
  'block' : bigint,
  'amount' : bigint,
}
export type Error = { 'MigrationKeyUnavailable' : null } |
  { 'LegacyWriteDisabled' : null } |
  { 'InvalidInput' : string } |
  { 'RekeyRequired' : null } |
  { 'VersionConflict' : null } |
  { 'ExecutionUnknown' : null } |
  { 'IntegrityFailed' : null } |
  { 'IdTimestampOutOfRange' : null } |
  { 'NotFound' : null } |
  { 'FeeBlocked' : null } |
  { 'DeviceNotApproved' : null } |
  { 'Locked' : null } |
  { 'RecoveryIncomplete' : null } |
  { 'IdCapacityExceeded' : null } |
  { 'PolicyStale' : null } |
  { 'IdGeneratorStateConflict' : null } |
  { 'IdempotencyConflict' : null } |
  { 'UnsupportedProtocol' : null } |
  { 'Unavailable' : string } |
  { 'Forbidden' : null } |
  { 'ResultExpired' : null } |
  { 'Expired' : null } |
  { 'QuotaExceeded' : null } |
  { 'AuthRequired' : null } |
  { 'Pending' : null };
export interface EscrowInfo {
  'liabilities' : bigint,
  'decision' : FundsDecision,
  'op_id' : Uint8Array | number[],
  'subaccount' : Uint8Array | number[],
  'network_fees' : bigint,
  'quote' : Quote,
  'transferred' : bigint,
  'funded_at' : [] | [bigint],
  'version' : bigint,
  'quote_digest' : Uint8Array | number[],
  'payer_principal' : Principal,
  'receipt_digest' : [] | [Uint8Array | number[]],
  'escrow_id' : Uint8Array | number[],
  'funding_ref' : [] | [bigint],
  'confirmed_in' : bigint,
}
export type FundsDecision = { 'SettlementCommitted' : null } |
  { 'RefundCommitted' : null } |
  { 'Pending' : null };
export type LegKind = { 'Refund' : { 'funding_block' : bigint } } |
  { 'Platform' : null } |
  { 'ReserveRefund' : null } |
  { 'Recipient' : null };
export type LegStatus = { 'Superseded' : null } |
  { 'FeeBlocked' : null } |
  { 'Rejected' : null } |
  { 'Succeeded' : null } |
  { 'InFlight' : null } |
  { 'Unknown' : null } |
  { 'Pending' : null };
export interface OpenEscrow {
  'quote_signature' : Uint8Array | number[],
  'offer' : SignedOffer,
  'op_id' : Uint8Array | number[],
  'quote' : Quote,
}
export interface PaymentInit {
  'service_fee' : bigint,
  'daily_orders' : number,
  'platform' : Account,
  'enabled' : boolean,
  'home_user' : Principal,
  'ledger' : Principal,
  'ledger_fee' : bigint,
  'signer' : ReceiptSigner,
  'max_open_per_payer' : number,
  'max_fee' : bigint,
}
export interface PaymentOffer {
  'account_id' : Uint8Array | number[],
  'quote_scope' : Uint8Array | number[],
  'issued_at' : bigint,
  'recipient' : Account,
  'device_id' : Uint8Array | number[],
  'security_epoch' : bigint,
  'version' : bigint,
  'ledger' : Principal,
  'offer_id' : Uint8Array | number[],
  'home_payment' : Principal,
  'expires_at' : bigint,
  'recipient_net' : bigint,
}
export interface Quote {
  'quote_scope' : Uint8Array | number[],
  'fee_reserve' : bigint,
  'service_fee' : bigint,
  'max_network_fee' : bigint,
  'envelope_digest' : Uint8Array | number[],
  'accept_by' : bigint,
  'offer_digest' : Uint8Array | number[],
  'recipient' : Account,
  'platform' : Account,
  'created_at' : bigint,
  'ledger' : Principal,
  'quote_id' : Uint8Array | number[],
  'max_bytes' : number,
  'payer' : Account,
  'amount' : bigint,
  'home_payment' : Principal,
  'retain_ms' : bigint,
  'recipient_net' : bigint,
  'signer_epoch' : bigint,
  'fund_by' : bigint,
}
export interface ReceiptSigner {
  'revoked' : boolean,
  'public_key' : Uint8Array | number[],
  'epoch' : bigint,
  'valid_until' : bigint,
  'valid_from' : bigint,
}
export type Result = { 'Ok' : EscrowInfo } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : TransferLeg } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : CertifiedBatch } |
  { 'Err' : Error };
export type Result_3 = { 'Ok' : ReceiptSigner } |
  { 'Err' : Error };
export type Result_4 = { 'Ok' : Array<EscrowInfo> } |
  { 'Err' : Error };
export type Result_5 = { 'Ok' : Array<TransferLeg> } |
  { 'Err' : Error };
export type Result_6 = { 'Ok' : null } |
  { 'Err' : Error };
export interface SignedOffer {
  'signature' : Uint8Array | number[],
  'offer' : PaymentOffer,
}
export interface SignedReceipt {
  'signature' : Uint8Array | number[],
  'receipt' : AdmissionReceipt,
}
export interface TransferLeg {
  'to' : Account,
  'fee' : bigint,
  'status' : LegStatus,
  'replaces' : [] | [bigint],
  'history_digest' : Uint8Array | number[],
  'kind' : LegKind,
  'memo' : Uint8Array | number[],
  'leg_id' : bigint,
  'block' : [] | [bigint],
  'created_at_time' : bigint,
  'escrow_id' : Uint8Array | number[],
  'revision' : bigint,
  'amount' : bigint,
  'expected_fee' : [] | [bigint],
}
export interface _SERVICE {
  'check_funding' : ActorMethod<[Uint8Array | number[], bigint], Result>,
  'claim_deposit_refund' : ActorMethod<
    [Uint8Array | number[], bigint],
    Result_1
  >,
  'claim_fee_reserve' : ActorMethod<[Uint8Array | number[]], Result_1>,
  'expiry_refund' : ActorMethod<[Uint8Array | number[]], Result>,
  'finalize_receipt' : ActorMethod<[SignedReceipt], Result>,
  'get_deposit' : ActorMethod<[Uint8Array | number[], bigint], [] | [Deposit]>,
  'get_escrow' : ActorMethod<[Uint8Array | number[]], Result>,
  'get_escrow_by_operation' : ActorMethod<
    [Principal, Uint8Array | number[]],
    Result
  >,
  'get_escrow_certified' : ActorMethod<
    [Array<Uint8Array | number[]>],
    Result_2
  >,
  'get_receipt_signer' : ActorMethod<[bigint], Result_3>,
  'get_transfer' : ActorMethod<[Uint8Array | number[], bigint], Result_1>,
  'list_my_escrows' : ActorMethod<[[] | [Uint8Array | number[]]], Result_4>,
  'list_transfers' : ActorMethod<
    [Uint8Array | number[], [] | [bigint]],
    Result_5
  >,
  'open_escrow' : ActorMethod<[OpenEscrow], Result>,
  'process_transfer' : ActorMethod<[Uint8Array | number[], bigint], Result_1>,
  'reconcile_transfer' : ActorMethod<
    [Uint8Array | number[], bigint, bigint],
    Result_1
  >,
  'revise_rejected_transfer' : ActorMethod<
    [Uint8Array | number[], bigint, bigint],
    Result_1
  >,
  'revoke_receipt_signer' : ActorMethod<[bigint], Result_6>,
  'rotate_receipt_signer' : ActorMethod<[ReceiptSigner], Result_6>,
  'set_orders_enabled' : ActorMethod<[boolean], Result_6>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
