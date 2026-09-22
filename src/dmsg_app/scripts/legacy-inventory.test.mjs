import { test } from 'node:test'
import assert from 'node:assert/strict'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import {
  kindOf,
  projection,
  normalizeState,
  routingEdges,
  collect
} from './legacy-inventory.mjs'

test('only known deployment roles are queried; identity/user lists are excluded', () => {
  assert.equal(kindOf('ic_message_channel_03'), 'channel')
  assert.equal(kindOf('ic_oss_bucket_42'), 'bucket')
  assert.equal(kindOf('dmsg_user'), null)
  assert.equal(kindOf('arbitrary_canister'), null)
  assert.equal(projection('dependency'), null)
  for (const kind of [
    'message',
    'channel',
    'profile',
    'cose',
    'cluster',
    'bucket',
    'name_identity',
    'minter'
  ]) {
    const p = projection(kind)
    assert(p)
    for (const name of [
      'latest_usernames',
      'init_vector',
      'weak_ed25519_secret_key',
      'payload',
      'dek'
    ])
      assert(!p.fields.includes(name))
  }
})

test('Candid projection omits unnecessary user names and preserves large counters', () => {
  const p = projection('message')
  const record = p.result._fields.find(([key]) => key === 'Ok')[1]
  const fields = Object.fromEntries(record._fields)
  const full = IDL.Variant({
    Ok: IDL.Record({ ...fields, latest_usernames: IDL.Vec(IDL.Text) }),
    Err: IDL.Text
  })
  const value = {
    name: 'synthetic',
    managers: [],
    channel_canisters: [],
    matured_channel_canisters: [],
    profile_canisters: [],
    cose_canisters: [],
    names_total: 2n,
    users_total: 3n,
    incoming_total: 1n << 100n,
    transfer_out_total: 0n,
    next_block_height: 0n,
    next_block_phash: new Uint8Array(32),
    latest_usernames: ['must-not-be-recorded']
  }
  const encoded = IDL.encode([full], [{ Ok: value }])
  const decoded = IDL.decode([p.result], encoded)[0].Ok
  const result = normalizeState('message', decoded)
  assert(!('latest_usernames' in result))
  assert.equal(result.incoming_total, (1n << 100n).toString())
  assert.equal(result.next_block_phash, '00'.repeat(32))
})

test('routes all shards including matured ones without collecting member identities', () => {
  const a = Principal.fromUint8Array(Uint8Array.of(1, 1)).toText()
  const b = Principal.fromUint8Array(Uint8Array.of(2, 1)).toText()
  const edges = routingEdges('message', {
    profile_canisters: [],
    channel_canisters: [a],
    matured_channel_canisters: [b],
    cose_canisters: []
  })
  assert.deepEqual(
    edges.map((e) => e.principal),
    [a, b]
  )
  assert(edges.every((e) => e.kind === 'channel'))
  assert.deepEqual(routingEdges('bucket', { managers: [a] }), [])
  assert.throws(() =>
    routingEdges('channel', { ic_oss_cluster: 'invalid', ic_oss_buckets: [] })
  )
})

test('refuses insecure hosts and tracked public output before network access', async () => {
  await assert.rejects(
    collect({ host: 'http://localhost:4943', ids: 'unused', output: '/tmp/unused' }),
    /HTTPS/
  )
  const output = new URL('../../../docs/i0-should-not-be-created', import.meta.url).pathname
  await assert.rejects(
    collect({ host: 'https://icp-api.io', ids: 'unused', output }),
    /Git-ignored/
  )
})
