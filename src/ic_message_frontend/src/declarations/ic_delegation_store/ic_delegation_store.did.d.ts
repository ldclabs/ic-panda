import type { Principal } from '@icp-sdk/core/principal'
import type { ActorMethod } from '@icp-sdk/core/agent'
import type { IDL } from '@icp-sdk/core/candid'

export type CanisterArgs = { 'Upgrade': UpgradeArgs } | { 'Init': InitArgs }
export interface InitArgs {
  'allowed_origins': Array<string>
}
export interface UpgradeArgs {
  'allowed_origins': [] | [Array<string>]
}
export interface _SERVICE {}
export declare const idlFactory: IDL.InterfaceFactory
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[]
