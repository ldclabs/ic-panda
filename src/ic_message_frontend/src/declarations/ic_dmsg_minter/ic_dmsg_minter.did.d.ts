import type { Principal } from '@icp-sdk/core/principal'
import type { ActorMethod } from '@icp-sdk/core/agent'
import type { IDL } from '@icp-sdk/core/candid'

export interface InitArgs {
  'preparers': Array<Principal>
  'committers': Array<Principal>
}
export interface LinkLog {
  'rewards': bigint
  'linker': [Principal, Principal]
  'minted_at': bigint
}
export type MinterArgs = { 'Upgrade': UpgradeArgs } | { 'Init': InitArgs }
export interface State {
  'preparers': Array<Principal>
  'next_block_height': bigint
  'minted_total': bigint
  'committers': Array<Principal>
}
export interface UpgradeArgs {
  'preparers': [] | [Array<Principal>]
  'committers': [] | [Array<Principal>]
}
export interface _SERVICE {
  'get_block': ActorMethod<[bigint], [] | [LinkLog]>
  'get_state': ActorMethod<[], State>
  'list_blocks': ActorMethod<[[] | [bigint], [] | [bigint]], Array<LinkLog>>
  'try_commit': ActorMethod<[Principal, Principal], [] | [bigint]>
  'try_prepare': ActorMethod<[Principal, Principal], boolean>
}
export declare const idlFactory: IDL.InterfaceFactory
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[]
