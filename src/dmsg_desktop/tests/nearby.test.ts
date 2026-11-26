import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  attachmentAvailable,
  buildSendStartParams,
  buildVaultSendSelection,
  formatSas,
  identityStatusLabelKey,
  isReceiveActive,
  isSendActive,
  isValidNearbyOffer,
  mergeReceiveView,
  progressFraction,
  receiveStateBadge,
  reconcilePeerSelections,
  sendStateBadge,
  transportLabelKey,
  vaultEntryBlocked,
  type NearbyPendingPeer,
  type NearbyReceiveView,
  type NearbySendView
} from '../src/renderer/views/nearby.ts'

test('SAS codes format 6 digits into two groups and preserve non-6-digit text', () => {
  assert.equal(formatSas('123456'), '123 456')
  assert.equal(formatSas(' 004912 '), '004 912')
  assert.equal(formatSas('12345'), '12345')
  assert.equal(formatSas(''), '')
  assert.equal(formatSas(null), '')
})

test('offer validation checks the dmsg-nearby/1: prefix and base64url payload', () => {
  assert.equal(isValidNearbyOffer('dmsg-nearby/1:oWZ2AQJ2ZXJzaW9uMTIzNDU2'), true)
  assert.equal(isValidNearbyOffer('  dmsg-nearby/1:abcDEF0123456789-_  '), true)
  assert.equal(isValidNearbyOffer('dmsg-nearby/1:short'), false)
  assert.equal(isValidNearbyOffer('dmsg-nearby/2:oWZ2AQJ2ZXJzaW9uMTIzNDU2'), false)
  assert.equal(isValidNearbyOffer('https://example.com'), false)
  assert.equal(isValidNearbyOffer(''), false)
})

test('receive and send active state checks and state badges cover every daemon state', () => {
  const recv = (state: NearbyReceiveView['state']) => ({ state }) as NearbyReceiveView
  assert.equal(isReceiveActive(recv('waiting')), true)
  assert.equal(isReceiveActive(recv('running')), true)
  assert.equal(isReceiveActive(recv('done')), false)
  assert.equal(isReceiveActive(recv('failed')), false)
  assert.equal(isReceiveActive(recv('cancelled')), false)
  assert.equal(isReceiveActive(null), false)

  const send = (state: NearbySendView['state']) => ({ state }) as NearbySendView
  for (const s of ['connecting', 'awaiting_sas', 'awaiting_approval', 'awaiting_accept', 'running'] as const) {
    assert.equal(isSendActive(send(s)), true)
    assert.ok(sendStateBadge(s).label)
  }
  for (const s of ['done', 'failed', 'cancelled'] as const) {
    assert.equal(isSendActive(send(s)), false)
    assert.ok(sendStateBadge(s).label)
  }
  assert.equal(receiveStateBadge('done').tone, 'ok')
  assert.equal(receiveStateBadge('failed').tone, 'danger')
  assert.equal(sendStateBadge('awaiting_sas').tone, 'warn')
  assert.equal(sendStateBadge('awaiting_approval').tone, 'gem')
  assert.equal(transportLabelKey('lan'), 'nearbyTransportLan')
  assert.equal(transportLabelKey('awdl'), 'nearbyTransportAwdl')
  assert.equal(transportLabelKey('relay'), 'nearbyTransportRelay')
  assert.equal(transportLabelKey('other'), null)
  assert.equal(identityStatusLabelKey('same_account'), 'nearbyIdentitySameAccount')
  assert.equal(identityStatusLabelKey('verified'), 'nearbyIdentityVerified')
  assert.equal(identityStatusLabelKey('signature_only'), 'nearbyIdentitySignatureOnly')
})

test('vault entry blocking and snapshot selection reject managed, deleted, conflicted and non-local items', () => {
  const entry = {
    id: 'item-1',
    revision: 'rev-1',
    deleted: false,
    item: {
      title: 'Prod API',
      type: 'api',
      tags: ['work'],
      notes: 'secret note',
      fields: [
        { id: 'username', label: '', type: 'text', value: 'alice' },
        { id: 'credential', label: '', type: 'concealed', value: 'tok_123' },
        { id: 'empty', label: '', type: 'text', value: '' }
      ],
      files: ['f-local', 'f-cloud']
    }
  }
  const details = {
    'f-local': { file: { id: 'f-local', current: 'v-1', local: true, trashed_at: null } },
    'f-cloud': { file: { id: 'f-cloud', current: 'v-2', local: false, trashed_at: null } }
  }

  assert.equal(vaultEntryBlocked(entry, false), false)
  assert.equal(vaultEntryBlocked(entry, true), true)
  assert.equal(vaultEntryBlocked({ ...entry, deleted: true }, false), true)
  assert.equal(vaultEntryBlocked({ ...entry, item: { ...entry.item, tags: ['dmsg-agent'] } }, false), true)

  assert.equal(attachmentAvailable(details['f-local']), true)
  assert.equal(attachmentAvailable(details['f-cloud']), false)
  assert.equal(attachmentAvailable({ file: { current: 'v-1', local: true, trashed_at: 123 } }), false)

  // Empty selection yields null.
  assert.equal(buildVaultSendSelection(entry, false, [], false, [], details), null)
  // Empty-valued field alone yields null.
  assert.equal(buildVaultSendSelection(entry, false, ['empty'], false, [], details), null)
  // Cloud-only attachment selection yields null.
  assert.equal(buildVaultSendSelection(entry, false, ['username'], false, ['f-cloud'], details), null)
  // Valid selection preserves item field order and current attachment version.
  assert.deepEqual(
    buildVaultSendSelection(entry, false, ['credential', 'username', 'empty'], true, ['f-local'], details),
    {
      item_id: 'item-1',
      revision: 'rev-1',
      field_ids: ['username', 'credential'],
      include_notes: true,
      attachments: [{ file_id: 'f-local', version_id: 'v-1' }]
    }
  )
})

test('peer file selections default new peers to all offered files and preserve user toggles', () => {
  const peers: NearbyPendingPeer[] = [
    {
      peer_id: 'p1',
      sas: '123456',
      qr_bound: true,
      transport: 'lan',
      kind: 'files',
      files: [
        { index: 0, name: 'a.txt', mime: 'text/plain', size: 10 },
        { index: 1, name: 'b.txt', mime: 'text/plain', size: 20 }
      ]
    },
    {
      peer_id: 'p2',
      sas: '654321',
      qr_bound: false,
      transport: 'awdl',
      kind: 'secret_package',
      files: [],
      package_size: 512
    }
  ]

  const first = reconcilePeerSelections({}, peers)
  assert.deepEqual(first, { p1: [0, 1] })

  const toggled = reconcilePeerSelections({ p1: [1], stale: [0] }, peers)
  assert.deepEqual(toggled, { p1: [1] })
})

test('buildSendStartParams builds exact RPC payloads for QR offers and discovered peers', () => {
  const files = [{ file_id: 'f1', version_id: 'v1', name: 'doc.pdf', size: 100 }]
  const offer = 'dmsg-nearby/1:oWZ2AQJ2ZXJzaW9uMTIzNDU2'
  const peer = {
    advert: '0123456789abcdef',
    alias: 'Bob Mac',
    pk_hint: 'abcd12',
    host: '192.168.1.20',
    port: 54321,
    transport: 'lan'
  }

  assert.equal(
    buildSendStartParams({
      mode: 'send-files',
      offerText: '',
      selectedPeer: null,
      files,
      vaultSelection: null,
      shareIdentity: false
    }),
    null
  )

  assert.deepEqual(
    buildSendStartParams({
      mode: 'send-files',
      offerText: `  ${offer}  `,
      selectedPeer: peer,
      files,
      vaultSelection: null,
      shareIdentity: true
    }),
    {
      share_identity: true,
      source: 'gui',
      offer,
      files: [{ file_id: 'f1', version_id: 'v1' }]
    }
  )

  const vaultSelection = {
    item_id: 'v-item',
    revision: 'rev-2',
    field_ids: ['password'],
    include_notes: false,
    attachments: []
  }
  assert.deepEqual(
    buildSendStartParams({
      mode: 'send-vault',
      offerText: '',
      selectedPeer: peer,
      files: [],
      vaultSelection,
      shareIdentity: false
    }),
    {
      share_identity: false,
      source: 'gui',
      host: '192.168.1.20',
      port: 54321,
      peer_alias: 'Bob Mac',
      advert: '0123456789abcdef',
      transport: 'lan',
      vault_item: vaultSelection
    }
  )
})

test('mergeReceiveView preserves mdns_error across nearby/changed events for the same session', () => {
  const base = { id: 7, state: 'waiting' } as NearbyReceiveView
  assert.equal(mergeReceiveView(base, { id: 7, error: 'mDNS blocked' })?.mdns_error, 'mDNS blocked')
  assert.equal(mergeReceiveView({ ...base, id: 8 }, { id: 7, error: 'mDNS blocked' })?.mdns_error, undefined)
  assert.equal(mergeReceiveView(null, { id: 7, error: 'mDNS blocked' }), null)
})

test('progressFraction returns indeterminate when total_bytes is 0 and clamped bytes otherwise', () => {
  assert.deepEqual(progressFraction(null), { value: null, max: 100 })
  assert.deepEqual(
    progressFraction({ total_items: 1, completed_items: 0, total_bytes: 0, bytes: 0 }),
    { value: null, max: 100 }
  )
  assert.deepEqual(
    progressFraction({ total_items: 1, completed_items: 0, total_bytes: 1000, bytes: 450 }),
    { value: 450, max: 1000 }
  )
  assert.deepEqual(
    progressFraction({ total_items: 1, completed_items: 1, total_bytes: 1000, bytes: 1200 }),
    { value: 1000, max: 1000 }
  )
})
