// Generated from the public dmsg_handle.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export interface Account {
  'owner' : Principal,
  'subaccount' : [] | [Uint8Array | number[]],
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
export type HandleAction = { 'AcceptTransfer' : null } |
  { 'Register' : null } |
  { 'Transfer' : null } |
  { 'ClaimLegacy' : null };
export interface HandleEvent {
  'at' : bigint,
  'to' : Uint8Array | number[],
  'previous' : Uint8Array | number[],
  'from' : [] | [Uint8Array | number[]],
  'version' : bigint,
  'handle' : string,
  'legacy_snapshot' : Uint8Array | number[],
  'sequence' : bigint,
}
export interface HandleInit {
  'max_pending' : number,
  'home_user' : Principal,
  'ledger' : Principal,
  'ledger_fee' : bigint,
}
export interface HandleIntent {
  'account_id' : Uint8Array | number[],
  'handle_canister' : Principal,
  'target_account' : [] | [Uint8Array | number[]],
  'action' : HandleAction,
  'op_id' : Uint8Array | number[],
  'handle' : string,
  'expected_version' : bigint,
  'terms_digest' : Uint8Array | number[],
}
export interface HandleOperation {
  'registration' : Registration,
  'memo' : Uint8Array | number[],
  'ledger_block' : [] | [bigint],
  'created_at' : bigint,
  'digest' : Uint8Array | number[],
  'phase' : HandlePhase,
  'amount' : bigint,
  'expires_at' : bigint,
}
export type HandlePhase = { 'Committed' : null } |
  { 'Reserved' : null } |
  { 'Paid' : null } |
  { 'RefundPending' : null } |
  { 'ChargeUnknown' : null } |
  { 'Charging' : null } |
  { 'Expired' : null };
export interface HandleRecord {
  'event_tip' : Uint8Array | number[],
  'version' : bigint,
  'handle' : string,
  'owner_account' : Uint8Array | number[],
}
export interface LegacyReservation {
  'frozen_admins' : Array<Principal>,
  'handle' : string,
  'legacy_name_principal' : [] | [Principal],
  'legacy_owner' : Principal,
  'quarantined' : boolean,
}
export interface LegacySnapshot {
  'event_tip' : Uint8Array | number[],
  'source_canister' : Principal,
  'count' : bigint,
  'entries_digest' : Uint8Array | number[],
  'snapshot_id' : Uint8Array | number[],
  'freeze_version' : bigint,
}
export interface Registration {
  'fee' : bigint,
  'intent' : HandleIntent,
  'payer' : Account,
}
export type Result = { 'Ok' : null } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : HandleRecord } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : HandleOperation } |
  { 'Err' : Error };
export type Result_3 = { 'Ok' : [] | [LegacyReservation] } |
  { 'Err' : Error };
export type Result_4 = { 'Ok' : SnapshotProgress } |
  { 'Err' : Error };
export type Result_5 = { 'Ok' : Array<LegacyReservation> } |
  { 'Err' : Error };
export type Result_6 = { 'Ok' : CertifiedBatch } |
  { 'Err' : Error };
export interface SnapshotProgress {
  'last_handle' : [] | [string],
  'snapshot' : [] | [LegacySnapshot],
  'imported' : bigint,
  'sealed' : boolean,
  'rolling_digest' : Uint8Array | number[],
}
export interface _SERVICE {
  'begin_legacy_snapshot' : ActorMethod<[LegacySnapshot], Result>,
  'claim_legacy_handle' : ActorMethod<
    [HandleIntent, Uint8Array | number[]],
    Result_1
  >,
  'commit_handle' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_2
  >,
  'expire_handle_reservation' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result
  >,
  'get_handle_event' : ActorMethod<[bigint], [] | [HandleEvent]>,
  'get_handle_operation' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_2
  >,
  'get_legacy_reservation' : ActorMethod<[string], Result_3>,
  'import_legacy_handles' : ActorMethod<
    [Uint8Array | number[], Array<LegacyReservation>],
    Result_4
  >,
  'list_legacy_reservations' : ActorMethod<[[] | [string]], Result_5>,
  'reconcile_handle_charge' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[], bigint],
    Result_2
  >,
  'reserve_handle' : ActorMethod<[Registration], Result_2>,
  'resolve_handle_certified' : ActorMethod<[Array<string>], Result_6>,
  'seal_legacy_snapshot' : ActorMethod<[], Result_4>,
  'snapshot_certified' : ActorMethod<[], Result_6>,
  'snapshot_progress' : ActorMethod<[], SnapshotProgress>,
  'transfer_handle' : ActorMethod<[HandleIntent, HandleIntent], Result_1>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
