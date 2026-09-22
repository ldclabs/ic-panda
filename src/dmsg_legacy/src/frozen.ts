import {
  Certificate,
  hashValue,
  lookupResultToBuffer,
  requestIdOf,
  type HttpAgent
} from '@icp-sdk/core/agent'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { sha256 } from '@noble/hashes/sha2.js'
import {
  binary,
  digest,
  pack,
  principal,
  requireLegacy,
  readObservation,
  observation
} from './base'
import { verifySnapshotSeries, type SnapshotProof } from './snapshot'
import type { LegacyArchive } from './archive'
const blob = IDL.Vec(IDL.Nat8)
const frozenFile = IDL.Record({
  id: IDL.Nat32,
  metadata: blob,
  chunks: IDL.Vec(IDL.Tuple(IDL.Nat32, IDL.Nat32, blob)),
  missing: IDL.Vec(IDL.Nat32),
  complete: IDL.Bool,
  ciphertext_digest: blob
})
const status = IDL.Record({
  folder: IDL.Nat32,
  cutover_id: blob,
  baseline_evidence: blob,
  mode: IDL.Variant({ Draining: IDL.Null, ReadOnly: IDL.Null }),
  frozen_at: IDL.Nat64,
  files: IDL.Nat64,
  digest: blob
})
const pageType = IDL.Record({
  status,
  entries: IDL.Vec(frozenFile),
  next: IDL.Opt(IDL.Nat32),
  complete: IDL.Bool
})
const resultType = IDL.Variant({ Ok: pageType, Err: IDL.Text })
export interface FolderProof {
  format: 'dmsg-legacy-folder-proof/1'
  canister: string
  caller: string
  argumentDigest: Uint8Array
  expiry: string
  nonce: Uint8Array
  requestId: Uint8Array
  certificate: Uint8Array
  reply: Uint8Array
}
function requestHash(proof: FolderProof) {
  const fields: Record<string, unknown> = {
    request_type: 'call',
    canister_id: Principal.fromText(proof.canister),
    method_name: 'legacy_folder_manifest',
    sender: Principal.fromText(proof.caller),
    ingress_expiry: BigInt(proof.expiry),
    ...(proof.nonce.length ? { nonce: proof.nonce } : {})
  }
  const rows = Object.entries(fields).map(
    ([key, value]) => [sha256(new TextEncoder().encode(key)), hashValue(value)] as const
  )
  rows.push([sha256(new TextEncoder().encode('arg')), proof.argumentDigest])
  rows.sort(([a], [b]) => {
    for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i]! - b[i]!
    return 0
  })
  const bytes = new Uint8Array(rows.length * 64)
  rows.forEach(([key, value], i) => {
    bytes.set(key, i * 64)
    bytes.set(value, i * 64 + 32)
  })
  return sha256(bytes)
}
export async function verifyFolderProof(
  proof: FolderProof,
  rootKey: Uint8Array,
  expected: { canister: string; caller: string; folder: number; cutover: Uint8Array }
) {
  requireLegacy(
    proof.format === 'dmsg-legacy-folder-proof/1' &&
      proof.canister === expected.canister &&
      proof.caller === expected.caller &&
      proof.argumentDigest.length === 32 &&
      proof.certificate.length <= 65536 &&
      proof.reply.length <= 196608 &&
      /^[0-9]{1,20}$/.test(proof.expiry) &&
      proof.nonce.length <= 32,
    'corrupt',
    'Invalid frozen folder proof'
  )
  const request = requestHash(proof)
  requireLegacy(
    digest(request) === digest(proof.requestId),
    'corrupt',
    'Folder request binding failed'
  )
  const certificate = await Certificate.create({
    certificate: binary(proof.certificate),
    rootKey: binary(rootKey),
    principal: { canisterId: Principal.fromText(expected.canister) },
    disableTimeVerification: true
  })
  const reply = lookupResultToBuffer(
      certificate.lookup_path(['request_status', request, 'reply'])
    ),
    state = lookupResultToBuffer(
      certificate.lookup_path(['request_status', request, 'status'])
    )
  requireLegacy(
    reply &&
      state &&
      new TextDecoder().decode(state) === 'replied' &&
      digest(reply) === digest(proof.reply),
    'corrupt',
    'Uncertified folder reply'
  )
  const result = IDL.decode([resultType], binary(reply))[0] as any,
    page = result.Ok
  requireLegacy(
    page &&
      'ReadOnly' in page.status.mode &&
      page.status.folder === expected.folder &&
      digest(page.status.cutover_id) === digest(expected.cutover) &&
      page.status.files <= 100000n &&
      page.entries.length <= 16,
    'corrupt',
    'Folder is not the expected frozen scope'
  )
  return page as {
    status: { files: bigint; digest: Uint8Array }
    entries: {
      id: number
      chunks: [number, number, Uint8Array][]
      missing: number[]
      complete: boolean
      ciphertext_digest: Uint8Array
    }[]
    next: number[]
    complete: boolean
  }
}
/** Only the argument digest is retained: bearer download tokens are never archived. */
export async function collectFolderProof(
  agent: HttpAgent,
  canister: string,
  folder: number,
  token: Uint8Array,
  allowed: string[]
) {
  requireLegacy(
    allowed.includes(principal(canister)) && agent.rootKey,
    'permission',
    'Unreviewed bucket'
  )
  const caller = (await agent.getPrincipal()).toText(),
    proofs: FolderProof[] = []
  let after: number[] = []
  for (;;) {
    const arg = new Uint8Array(
      IDL.encode([IDL.Nat32, IDL.Opt(IDL.Nat32), IDL.Opt(blob)], [folder, after, [token]])
    )
    const result = await agent.update(canister, { methodName: 'legacy_folder_manifest', arg })
    requireLegacy(result.requestDetails, 'version', 'Missing ingress request details')
    const request = result.requestDetails
    const proof: FolderProof = {
      format: 'dmsg-legacy-folder-proof/1',
      canister,
      caller,
      argumentDigest: sha256(arg),
      expiry: request.ingress_expiry.toBigInt().toString(),
      nonce: request.nonce ? binary(request.nonce) : new Uint8Array(),
      requestId: requestIdOf(request),
      certificate: binary(result.rawCertificate),
      reply: binary(result.reply)
    }
    const resultValue = IDL.decode([resultType], binary(result.reply))[0] as any
    requireLegacy(resultValue.Ok, 'version', 'Folder freeze is unavailable')
    const page = await verifyFolderProof(proof, agent.rootKey, {
      canister,
      caller,
      folder,
      cutover: resultValue.Ok.status.cutover_id
    })
    requireLegacy(
      page.complete ||
        (page.next.length === 1 && (!after.length || page.next[0]! > after[0]!)),
      'corrupt',
      'Folder cursor stalled'
    )
    proofs.push(proof)
    requireLegacy(proofs.length <= 10000, 'limit', 'Folder proof limit')
    if (page.complete) return proofs
    after = page.next
  }
}
export interface FrozenArchiveProof {
  channel: string
  authority: SnapshotProof
  messages: SnapshotProof[]
  folder: FolderProof[]
}
export async function compareFrozenArchive(
  archive: LegacyArchive,
  proofs: FrozenArchiveProof[],
  trust: { rootKey: Uint8Array; channels: string[]; buckets: string[]; cutover: Uint8Array }
) {
  const changes: { source: string; reason: string }[] = [],
    covered = new Set<string>()
  for (const proof of proofs) {
    const match = /^(.+)\/channel\/([0-9]+)$/.exec(proof.channel)
    requireLegacy(
      match && trust.channels.includes(match[1]!),
      'permission',
      'Unreviewed channel scope'
    )
    const source = match[1]!,
      channel = Number(match[2]),
      expected = { rootKey: trust.rootKey, canister: source, cutover: trust.cutover }
    const authority = await verifySnapshotSeries([proof.authority], {
      ...expected,
      scope: { ChannelAuthority: channel }
    })
    const rows = await verifySnapshotSeries(proof.messages, {
      ...expected,
      scope: { Messages: channel }
    })
    requireLegacy(
      proof.authority.caller === archive.inventory.principal &&
        proof.messages[0]?.caller === archive.inventory.principal,
      'permission',
      'Frozen archive identity mismatch'
    )
    const type = IDL.Record({
      id: IDL.Nat32,
      message_start: IDL.Nat32,
      latest_message_id: IDL.Nat32,
      dek_digest: blob,
      storage: IDL.Opt(IDL.Tuple(IDL.Principal, IDL.Nat32))
    })
    const current = IDL.decode([type], authority.rows[0]![1])[0] as any
    const old = archive.inventory.objects.find(
      (o) => o.key === proof.channel && o.kind === 'channel'
    )
    if (
      !old ||
      digest(binary(readObservation(old.bytes).dek)) !==
        Array.from(current.dek_digest as Uint8Array, (b) =>
          b.toString(16).padStart(2, '0')
        ).join('')
    )
      changes.push({ source: proof.channel, reason: 'channel-key-changed-or-missing' })
    const messageType = IDL.Record({
      id: IDL.Nat32,
      kind: IDL.Nat8,
      reply_to: IDL.Nat32,
      created_by: IDL.Principal,
      created_at: IDL.Nat64,
      payload: blob
    })
    for (const [, bytes] of rows.rows) {
      const [id, values] = IDL.decode([IDL.Nat32, IDL.Opt(messageType)], bytes) as unknown as [
          number,
          any[]
        ],
        key = `${proof.channel}/message/${id}`
      const object = archive.inventory.objects.find(
        (o) => o.key === key && o.kind === 'message'
      )
      if (!values.length) {
        if (object && readObservation(object.bytes).payload.length)
          changes.push({ source: key, reason: 'source-slot-absent-cache-retained' })
        continue
      }
      if (!object || object.digest !== digest(pack(observation(values[0]))))
        changes.push({ source: key, reason: object ? 'message-changed' : 'message-added' })
    }
    if (current.storage.length) {
      const [bucket, folder] = current.storage[0] as [Principal, number]
      requireLegacy(
        trust.buckets.includes(bucket.toText()) && proof.folder.length > 0,
        'missing',
        'Frozen attachment manifest required'
      )
      let count = 0,
        last = -1,
        digestValue = ''
      for (const [index, frozen] of proof.folder.entries()) {
        const page = await verifyFolderProof(frozen, trust.rootKey, {
          canister: bucket.toText(),
          caller: archive.inventory.principal,
          folder,
          cutover: trust.cutover
        })
        const nextDigest = digest(page.status.digest)
        requireLegacy(
          !digestValue || nextDigest === digestValue,
          'corrupt',
          'Folder manifest changed'
        )
        digestValue = nextDigest
        requireLegacy(
          page.complete === (index === proof.folder.length - 1),
          'missing',
          'Frozen folder pages incomplete'
        )
        for (const file of page.entries) {
          requireLegacy(file.id > last, 'corrupt', 'Duplicated frozen file')
          last = file.id
          count++
          const key = `${bucket.toText()}/file/${file.id}`,
            object = archive.inventory.objects.find((o) => o.key === key && o.kind === 'file')
          if (
            !object ||
            !file.complete ||
            digest(binary(readObservation(object.bytes).ciphertext)) !==
              Array.from(file.ciphertext_digest, (b) => b.toString(16).padStart(2, '0')).join(
                ''
              )
          )
            changes.push({ source: key, reason: 'file-changed-missing-or-incomplete' })
        }
        if (page.complete)
          requireLegacy(
            BigInt(count) === page.status.files,
            'missing',
            'Frozen file count mismatch'
          )
      }
    }
    covered.add(proof.channel)
  }
  for (const object of archive.inventory.objects.filter((o) => o.kind === 'channel'))
    if (!covered.has(object.key))
      changes.push({ source: object.key, reason: 'frozen-proof-missing' })
  return {
    format: 'dmsg-legacy-freeze-comparison/1',
    cutover: Array.from(trust.cutover, (b) => b.toString(16).padStart(2, '0')).join(''),
    checkedChannels: covered.size,
    changes,
    matched: changes.length === 0 && covered.size > 0
  }
}
