// Anonymous, read-only deployment inventory. No identity files, user records,
// token issuance, key derivation, management updates or business writes.
import {
  Actor,
  AnonymousIdentity,
  Certificate,
  Cbor,
  HttpAgent,
  lookupResultToBuffer
} from '@icp-sdk/core/agent'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { createHash } from 'node:crypto'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { execFileSync } from 'node:child_process'
import { resolve, relative, dirname } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { parseArgs } from 'node:util'

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex')
const Blob = IDL.Vec(IDL.Nat8),
  Principals = IDL.Vec(IDL.Principal)
const PublicKey = IDL.Record({ public_key: Blob, chain_code: Blob })
const opt = IDL.Opt
const states = {
  message: {
    name: IDL.Text,
    managers: Principals,
    profile_canisters: Principals,
    channel_canisters: Principals,
    matured_channel_canisters: Principals,
    cose_canisters: Principals,
    names_total: IDL.Nat64,
    users_total: IDL.Nat64,
    incoming_total: IDL.Nat,
    transfer_out_total: IDL.Nat,
    next_block_height: IDL.Nat64,
    next_block_phash: Blob
  },
  channel: {
    name: IDL.Text,
    managers: Principals,
    ic_oss_cluster: opt(IDL.Principal),
    ic_oss_buckets: Principals,
    channel_id: IDL.Nat32,
    channels_total: IDL.Nat64,
    messages_total: IDL.Nat64,
    incoming_gas: IDL.Nat,
    burned_gas: IDL.Nat
  },
  profile: {
    name: IDL.Text,
    managers: Principals,
    ic_oss_cluster: opt(IDL.Principal),
    ic_oss_buckets: Principals,
    profiles_total: IDL.Nat64
  },
  cose: {
    name: IDL.Text,
    managers: Principals,
    auditors: opt(Principals),
    governance_canister: opt(IDL.Principal),
    ecdsa_key_name: IDL.Text,
    schnorr_key_name: IDL.Text,
    vetkd_key_name: opt(IDL.Text),
    vetkd_context_version: opt(IDL.Nat8),
    ecdsa_public_key: opt(PublicKey),
    schnorr_ed25519_public_key: opt(PublicKey),
    schnorr_secp256k1_public_key: opt(PublicKey),
    allowed_apis: opt(IDL.Vec(IDL.Text)),
    namespace_total: IDL.Nat64,
    subnet_size: opt(IDL.Nat64)
  },
  cluster: {
    name: IDL.Text,
    managers: Principals,
    governance_canister: opt(IDL.Principal),
    ecdsa_key_name: IDL.Text,
    schnorr_key_name: IDL.Text,
    token_expiration: IDL.Nat64,
    ecdsa_token_public_key: IDL.Text,
    schnorr_ed25519_token_public_key: IDL.Text,
    weak_ed25519_token_public_key: IDL.Text,
    bucket_latest_version: Blob,
    bucket_wasm_total: IDL.Nat64,
    bucket_deployed_total: IDL.Nat64,
    bucket_deployment_logs: IDL.Nat64,
    subject_authz_total: IDL.Nat64,
    committers: opt(Principals)
  },
  bucket: {
    name: IDL.Text,
    managers: Principals,
    auditors: opt(Principals),
    governance_canister: opt(IDL.Principal),
    status: IDL.Int8,
    visibility: IDL.Nat8,
    total_files: IDL.Nat64,
    total_chunks: IDL.Nat64,
    total_folders: IDL.Nat64,
    file_id: IDL.Nat32,
    folder_id: IDL.Nat32,
    max_file_size: IDL.Nat64,
    trusted_ecdsa_pub_keys: IDL.Vec(Blob),
    trusted_eddsa_pub_keys: IDL.Vec(Blob)
  },
  name_identity: {
    name: IDL.Text,
    session_expires_in_ms: IDL.Nat64,
    sign_in_count: IDL.Nat64
  },
  minter: {
    preparers: Principals,
    committers: Principals,
    next_block_height: IDL.Nat64,
    minted_total: IDL.Nat64
  }
}
export function kindOf(name) {
  if (name === 'ic_message') return 'message'
  if (/^ic_message_channel(?:_\d+)?$/.test(name)) return 'channel'
  if (/^ic_message_profile(?:_\d+)?$/.test(name)) return 'profile'
  if (/^ic_cose_canister(?:_\d+)?$/.test(name)) return 'cose'
  if (/^ic_oss_cluster(?:_\d+)?$/.test(name)) return 'cluster'
  if (/^ic_oss_bucket(?:_\d+)?$/.test(name)) return 'bucket'
  if (name === 'ic_name_identity') return 'name_identity'
  if (name === 'ic_dmsg_minter') return 'minter'
  if (
    [
      'dmsg_frontend',
      'ic_panda_frontend',
      'ic_signin_with',
      'ic_delegation_store',
      'dmsg_ledger_canister',
      'dmsg_index_canister'
    ].includes(name)
  )
    return 'dependency'
  return null
}
export function projection(kind) {
  if (!states[kind]) return null
  const method =
    { cose: 'state_get_info', cluster: 'get_cluster_info', bucket: 'get_bucket_info' }[kind] ??
    'get_state'
  const record = IDL.Record(states[kind])
  const result = kind === 'minter' ? record : IDL.Variant({ Ok: record, Err: IDL.Text })
  return {
    method,
    result,
    args: kind === 'bucket' ? [[]] : [],
    input: kind === 'bucket' ? [opt(Blob)] : [],
    fields: Object.keys(states[kind])
  }
}
function jsonValue(value) {
  if (typeof value === 'bigint') return value.toString()
  if (value instanceof Uint8Array) return Buffer.from(value).toString('hex')
  if (value && typeof value.toText === 'function') return value.toText()
  if (Array.isArray(value)) return value.map(jsonValue)
  if (value && typeof value === 'object')
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, jsonValue(v)]))
  return value
}
export function normalizeState(kind, value) {
  const fields = states[kind]
  const result = {}
  for (const [name, type] of Object.entries(fields)) {
    result[name] = jsonValue(
      type instanceof IDL.OptClass ? (value[name]?.[0] ?? null) : value[name]
    )
  }
  for (const name of [
    'ecdsa_public_key',
    'schnorr_ed25519_public_key',
    'schnorr_secp256k1_public_key'
  ]) {
    if (result[name]?.public_key)
      result[name].sha256_public_key = sha256(Buffer.from(result[name].public_key, 'hex'))
  }
  return result
}
export function routingEdges(kind, state) {
  const edges = []
  const add = (key, targetKind, values) => {
    for (const value of values ?? []) {
      const principal = Principal.fromText(value).toText()
      if (principal !== value) throw new Error('Noncanonical routed Principal')
      edges.push({ field: key, principal, kind: targetKind })
    }
  }
  if (kind === 'message') {
    add('profile_canisters', 'profile', state.profile_canisters)
    add('channel_canisters', 'channel', state.channel_canisters)
    add('matured_channel_canisters', 'channel', state.matured_channel_canisters)
    add('cose_canisters', 'cose', state.cose_canisters)
  }
  if (['channel', 'profile'].includes(kind)) {
    add('ic_oss_cluster', 'cluster', state.ic_oss_cluster ? [state.ic_oss_cluster] : [])
    add('ic_oss_buckets', 'bucket', state.ic_oss_buckets)
  }
  if (kind === 'cluster') add('get_buckets', 'bucket', state.buckets)
  return edges
}
function certificateMillis(value) {
  if (!value?.length || value.length > 10 || value.at(-1) & 128)
    throw new Error('Invalid certified time')
  let result = 0n
  for (let i = 0; i < value.length; i++) result |= BigInt(value[i] & 127) << BigInt(i * 7)
  return Number(result / 1000000n)
}
async function metadata(agent, principal, output) {
  const canister = principal.toUint8Array(),
    text = (s) => new TextEncoder().encode(s)
  const paths = [
    ['time'].map(text),
    ...['controllers', 'module_hash'].map((key) => [text('canister'), canister, text(key)]),
    [text('canister'), canister, text('metadata'), text('candid:service')]
  ]
  const response = await agent.readState(principal, { paths })
  const cert = await Certificate.create({
    certificate: response.certificate,
    rootKey: agent.rootKey,
    principal: { canisterId: principal },
    maxAgeInMinutes: 5
  })
  const lookup = (suffix) =>
    lookupResultToBuffer(cert.lookup_path(['canister', canister, ...suffix]))
  const controllers = lookup(['controllers']),
    moduleHash = lookup(['module_hash'])
  const candid = lookup(['metadata', 'candid:service'])
  const certificatePath = `canisters/${principal.toText()}.certificate.cbor`
  await writeFile(resolve(output, certificatePath), response.certificate, {
    flag: 'wx',
    mode: 0o600
  })
  let candidPath = null
  if (candid) {
    candidPath = `canisters/${principal.toText()}.did`
    await writeFile(resolve(output, candidPath), candid, { flag: 'wx', mode: 0o600 })
  }
  return {
    evidence: 'certified_read_state',
    certified_at: new Date(
      certificateMillis(lookupResultToBuffer(cert.lookup_path(['time'])))
    ).toISOString(),
    controllers: controllers
      ? Cbor.decode(controllers).map((v) => Principal.fromUint8Array(v).toText())
      : null,
    module_hash: moduleHash ? Buffer.from(moduleHash).toString('hex') : null,
    candid_sha256: candid ? sha256(candid) : null,
    candid_path: candidPath,
    certificate_path: certificatePath,
    certificate_sha256: sha256(response.certificate)
  }
}
async function queryState(agent, principal, kind) {
  const p = projection(kind)
  if (!p) return null
  const factory = () => IDL.Service({ [p.method]: IDL.Func(p.input, [p.result], ['query']) })
  const actor = Actor.createActor(factory, { agent, canisterId: principal })
  const response = await actor[p.method](...p.args)
  if (kind !== 'minter' && !('Ok' in response)) throw new Error(String(response.Err))
  const value = normalizeState(kind, kind === 'minter' ? response : response.Ok)
  if (kind === 'cose') {
    const input = opt(IDL.Record({ ns: IDL.Text, derivation_path: IDL.Vec(Blob) }))
    const result = IDL.Variant({ Ok: PublicKey, Err: IDL.Text })
    const actor = Actor.createActor(
      () =>
        IDL.Service({
          ecdsa_public_key: IDL.Func([input], [result], ['query']),
          schnorr_public_key: IDL.Func(
            [IDL.Variant({ ed25519: IDL.Null, bip340secp256k1: IDL.Null }), input],
            [result],
            ['query']
          )
        }),
      { agent, canisterId: principal }
    )
    value.cached_public_keys = {}
    for (const algorithm of [
      'ecdsa_secp256k1',
      'schnorr_ed25519',
      'schnorr_bip340secp256k1'
    ]) {
      try {
        const response =
          algorithm === 'ecdsa_secp256k1'
            ? await actor.ecdsa_public_key([])
            : await actor.schnorr_public_key(
                { [algorithm === 'schnorr_ed25519' ? 'ed25519' : 'bip340secp256k1']: null },
                []
              )
        if (!('Ok' in response)) throw new Error(response.Err)
        value.cached_public_keys[algorithm] = {
          ...jsonValue(response.Ok),
          sha256_public_key: sha256(Uint8Array.from(response.Ok.public_key)),
          source: 'cached root public key query, null derivation input'
        }
      } catch (error) {
        value.cached_public_keys[algorithm] = { error: String(error.message).slice(0, 2000) }
      }
    }
  }
  if (kind === 'cluster') {
    try {
      const actor = Actor.createActor(
        () =>
          IDL.Service({
            get_buckets: IDL.Func(
              [],
              [IDL.Variant({ Ok: Principals, Err: IDL.Text })],
              ['query']
            )
          }),
        { agent, canisterId: principal }
      )
      const result = await actor.get_buckets()
      if (!('Ok' in result)) throw new Error(result.Err)
      value.buckets = result.Ok.map((v) => v.toText())
    } catch (error) {
      value.buckets_error = String(error.message).slice(0, 2000)
    }
    try {
      const deployment = IDL.Record({
        canister: IDL.Principal,
        deploy_at: IDL.Nat64,
        wasm_hash: Blob,
        prev_hash: Blob,
        error: opt(IDL.Text)
      })
      const actor = Actor.createActor(
        () =>
          IDL.Service({
            get_deployed_buckets: IDL.Func(
              [],
              [IDL.Variant({ Ok: IDL.Vec(deployment), Err: IDL.Text })],
              ['query']
            )
          }),
        { agent, canisterId: principal }
      )
      const response = await actor.get_deployed_buckets()
      if (!('Ok' in response)) throw new Error(response.Err)
      value.deployments = jsonValue(response.Ok)
    } catch (error) {
      value.deployments_error = String(error.message).slice(0, 2000)
    }
  }
  return {
    method: p.method,
    evidence: 'node_signed_query_not_certified_snapshot',
    observed_at: new Date().toISOString(),
    value
  }
}
export async function collect({ ids, host, output, maxCanisters = 64 }) {
  const url = new URL(host)
  if (url.protocol !== 'https:' || url.username || url.password)
    throw new Error('HTTPS host required; local/custom roots are not accepted')
  const gitRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../../..')
  const within = relative(gitRoot, output)
  if (within === '' || (!within.startsWith('..') && !within.startsWith('/'))) {
    try {
      execFileSync('git', ['check-ignore', '-q', '--', output], { cwd: gitRoot })
    } catch {
      throw new Error('Inventory output inside the public checkout must be Git-ignored')
    }
  }
  await mkdir(resolve(output, 'canisters'), { recursive: true, mode: 0o700 })
  const input = JSON.parse(await readFile(ids, 'utf8'))
  const items = new Map(),
    edges = []
  function add(principal, kind, name, source) {
    if (Principal.fromText(principal).toText() !== principal)
      throw new Error('Noncanonical Principal')
    let item = items.get(principal)
    if (!item) {
      if (items.size >= maxCanisters)
        throw new Error('Discovery bound reached; review before raising max-canisters')
      item = { principal, kind, names: [], sources: [], errors: [] }
      items.set(principal, item)
    }
    if (item.kind !== kind) throw new Error(`Conflicting canister role: ${principal}`)
    if (name && !item.names.includes(name)) item.names.push(name)
    if (!item.sources.includes(source)) item.sources.push(source)
  }
  for (const [name, value] of Object.entries(input)) {
    const kind = kindOf(name)
    if (kind && value.ic) add(value.ic, kind, name, 'input canister_ids.json')
  }
  if (!items.size) throw new Error('No known mainnet legacy canisters in the input')
  const agent = await HttpAgent.create({
    host: url.origin,
    identity: new AnonymousIdentity(),
    shouldFetchRootKey: false,
    verifyQuerySignatures: true,
    fetch: (input, init) => fetch(input, { ...init, signal: AbortSignal.timeout(30000) })
  })
  const started = new Date().toISOString()
  const processed = new Set()
  while (processed.size < items.size) {
    const batch = [...items.values()]
      .filter((item) => !processed.has(item.principal))
      .slice(0, 3)
    const completed = await Promise.all(
      batch.map(async (item) => {
        const principal = Principal.fromText(item.principal)
        try {
          item.metadata = await metadata(agent, principal, output)
        } catch (error) {
          item.errors.push({
            operation: 'read_state',
            message: String(error.message).slice(0, 2000)
          })
        }
        try {
          item.state = await queryState(agent, principal, item.kind)
        } catch (error) {
          item.errors.push({
            operation: 'state query',
            message: String(error.message).slice(0, 2000)
          })
        }
        processed.add(item.principal)
        return item
      })
    )
    for (const item of completed) {
      if (item.state)
        for (const edge of routingEdges(item.kind, item.state.value)) {
          edges.push({ from: item.principal, ...edge })
          add(edge.principal, edge.kind, null, `${item.principal}.${edge.field}`)
        }
      process.stdout.write(
        `${item.names[0] ?? item.kind}: metadata=${!!item.metadata} state=${!!item.state} errors=${item.errors.length}\n`
      )
    }
  }
  const result = {
    schema: 1,
    started_at: started,
    completed_at: new Date().toISOString(),
    host: url.origin,
    identity: 'anonymous',
    query_signature_verification: true,
    fetched_custom_root: false,
    updates_sent: 0,
    input_sha256: sha256(await readFile(ids)),
    collector_sha256: sha256(await readFile(fileURLToPath(import.meta.url))),
    canisters: [...items.values()],
    edges,
    limitations: [
      'State queries are node-signed observations, not a frozen full-data snapshot.',
      'Controllers/module hashes and published Candid are certified; runtime memory, cycles, stable schema and inaccessible roots remain unverified.',
      'The per-canister observations are not atomic across canisters or upgrades.',
      'No per-user content, setting, token, private-key or recovery endpoints are called; incidental get_state fields such as latest_usernames are omitted by the projection.'
    ]
  }
  await writeFile(resolve(output, 'inventory.json'), JSON.stringify(result, null, 2) + '\n', {
    flag: 'wx',
    mode: 0o600
  })
  return result
}
async function main() {
  const { values } = parseArgs({
    options: {
      ids: { type: 'string' },
      output: { type: 'string' },
      host: { type: 'string', default: 'https://icp-api.io' },
      'max-canisters': { type: 'string', default: '64' }
    }
  })
  if (!values.ids || !values.output)
    throw new Error(
      'Usage: node legacy-inventory.mjs --ids canister_ids.json --output /private/ignored/new-directory'
    )
  const maximum = Number(values['max-canisters'])
  if (!Number.isSafeInteger(maximum) || maximum < 1 || maximum > 256)
    throw new Error('max-canisters must be 1..256')
  const result = await collect({
    ids: resolve(values.ids),
    output: resolve(values.output),
    host: values.host,
    maxCanisters: maximum
  })
  console.log(
    `Recorded ${result.canisters.length} canisters and ${result.edges.length} routing edges; no update calls.`
  )
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href)
  main().catch((error) => {
    console.error(error.message)
    process.exitCode = 1
  })
