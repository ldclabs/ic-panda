import { IDL } from '@icp-sdk/core/candid'
import { Actor, type ActorMethod } from '@icp-sdk/core/agent'
import {
  LegacyReader,
  LegacyCheckpointStore,
  type ReaderSource,
  type Service
} from '@dmsg/legacy'
import { idlFactory as message } from '$declarations/ic_message/ic_message.did.js'
import { idlFactory as channel } from '$declarations/ic_message_channel/ic_message_channel.did.js'
import { idlFactory as profile } from '$declarations/ic_message_profile/ic_message_profile.did.js'
import { idlFactory as cose } from '$declarations/ic_cose_canister/ic_cose_canister.did.js'
import { idlFactory as bucket } from '$declarations/ic_oss_bucket_02/ic_oss_bucket_02.did.js'
import { dynAgent } from '$lib/utils/auth'

// Deployment inventory observed 2026-09-22. A route change requires an explicit
// inventory review; an actor response cannot add new authorized targets.
const allowed: ReaderSource['allowed'] = {
  ledger: ['druyg-tyaaa-aaaaq-aactq-cai', 'ocqzv-tyaaa-aaaar-qal4a-cai'],
  minter: ['ql553-iqaaa-aaaap-anuyq-cai'],
  message: ['nscli-qiaaa-aaaaj-qa4pa-cai'],
  channel: [
    'nvdn4-5qaaa-aaaaj-qa4pq-cai',
    'zof5a-5yaaa-aaaai-acr2q-cai',
    '4jxyd-pqaaa-aaaah-qdqtq-cai'
  ],
  profile: ['ijyxz-wyaaa-aaaaj-qa4qa-cai'],
  cose: ['n3bau-gaaaa-aaaaj-qa4oq-cai'],
  bucket: ['532er-faaaa-aaaaj-qncpa-cai', 'sb6zj-3aaaa-aaaaj-qndla-cai']
}
export async function createLegacyReader(mode: ReaderSource['mode']) {
  const ledger = () =>
    IDL.Service({
      icrc1_balance_of: IDL.Func(
        [IDL.Record({ owner: IDL.Principal, subaccount: IDL.Opt(IDL.Vec(IDL.Nat8)) })],
        [IDL.Nat],
        ['query']
      ),
      icrc1_decimals: IDL.Func([], [IDL.Nat8], ['query']),
      icrc1_symbol: IDL.Func([], [IDL.Text], ['query'])
    })
  const minter = () =>
    IDL.Service({
      get_state: IDL.Func(
        [],
        [
          IDL.Record({
            next_block_height: IDL.Nat64,
            minted_total: IDL.Nat64,
            preparers: IDL.Vec(IDL.Principal),
            committers: IDL.Vec(IDL.Principal)
          })
        ],
        ['query']
      ),
      get_block: IDL.Func(
        [IDL.Nat64],
        [
          IDL.Opt(
            IDL.Record({
              linker: IDL.Tuple(IDL.Principal, IDL.Principal),
              rewards: IDL.Nat64,
              minted_at: IDL.Nat64
            })
          )
        ],
        ['query']
      )
    })
  const factories = { message, channel, profile, cose, bucket, ledger, minter }
  const actors = new Map<string, Record<string, ActorMethod>>()
  const source: ReaderSource = {
    principal: dynAgent.id.getPrincipal().toText(),
    message: allowed.message[0]!,
    mode,
    allowed,
    derivation: {
      contextVersion: 1,
      keyNames: { Local: 'test_key_1', ECDH: 'test_key_1', VetKey: 'key_1' }
    }
  }
  const checkpoint = await LegacyCheckpointStore.open(source)
  const restored = await checkpoint.load()
  const reader = new LegacyReader(
    source,
    {
      identity: () => dynAgent.id.getPrincipal().toText(),
      async call(service: Service, canister: string, method: string, args: unknown[]) {
        const key = `${service}:${canister}`
        let actor = actors.get(key)
        if (!actor) {
          actor = Actor.createActor(factories[service], {
            agent: dynAgent,
            canisterId: canister
          })
          actors.set(key, actor)
        }
        return actor[method]!(...args)
      }
    },
    { restored, save: (state) => checkpoint.save(state) }
  )
  return { reader, checkpoint }
}
