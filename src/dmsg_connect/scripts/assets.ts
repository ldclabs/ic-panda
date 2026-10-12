// The release record of connect.dmsg.net: the SHA-256 of every file the asset
// canister serves, the headers file, the build config and the commit, and a
// check of a deployed canister against such a record.
//
//   node scripts/assets.ts manifest [dist]                    > manifest.json
//   node scripts/assets.ts verify manifest.json <canister> [host]
//
// `manifest` reads DMSG_CONNECT_CONFIG like the build. Two builds of one
// commit and config give the same `sha256`. `verify` reads the canister's
// asset list anonymously (default host https://icp-api.io) and fails on any
// missing, extra or different file.
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { readdir, readFile } from 'node:fs/promises'
import { join, relative, resolve, sep } from 'node:path'
import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { IDL } from '@icp-sdk/core/candid'

interface Manifest {
  format: 'dmsg-connect-assets/1'
  commit: string
  dirty: boolean
  config: unknown
  /** `.ic-assets.json5`: the response headers the canister applies. */
  headers: string
  /** Served path to SHA-256, sorted by path. */
  assets: Record<string, string>
  /** SHA-256 of the `sha256sum`-style listing of `headers` and `assets`. */
  sha256: string
}

const HEADERS = '.ic-assets.json5'
const sha256 = (bytes: Uint8Array | string) => createHash('sha256').update(bytes).digest('hex')

async function files(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { recursive: true, withFileTypes: true })
  return entries.filter((e) => e.isFile()).map((e) => join(e.parentPath, e.name))
}

async function manifest(dist: string): Promise<Manifest> {
  const root = resolve(dist)
  const assets: Record<string, string> = {}
  let headers = ''
  for (const file of (await files(root)).sort()) {
    const path = '/' + relative(root, file).split(sep).join('/')
    const digest = sha256(await readFile(file))
    if (path === `/${HEADERS}`) headers = digest
    else assets[path] = digest
  }
  if (!headers) throw new Error(`${root} has no ${HEADERS}; build it first`)
  const sorted = Object.fromEntries(Object.entries(assets).sort(([a], [b]) => (a < b ? -1 : 1)))
  const listing = [`${headers}  /${HEADERS}`, ...Object.entries(sorted).map(([p, d]) => `${d}  ${p}`)].join('\n') + '\n'
  const git = (...args: string[]) => execFileSync('git', args, { encoding: 'utf8' }).trim()
  return {
    format: 'dmsg-connect-assets/1',
    commit: git('rev-parse', 'HEAD'),
    dirty: git('status', '--porcelain') !== '',
    config: JSON.parse(await readFile(resolve(process.env.DMSG_CONNECT_CONFIG ?? 'connect.config.json'), 'utf8')),
    headers,
    assets: sorted,
    sha256: sha256(listing)
  }
}

const Encoding = IDL.Record({
  content_encoding: IDL.Text,
  sha256: IDL.Opt(IDL.Vec(IDL.Nat8)),
  length: IDL.Nat
})
const Asset = IDL.Record({ key: IDL.Text, content_type: IDL.Text, encodings: IDL.Vec(Encoding) })
const assetCanister = () => IDL.Service({ list: IDL.Func([IDL.Record({})], [IDL.Vec(Asset)], ['query']) })

type Listed = { key: string; encodings: { content_encoding: string; sha256: [] | [Uint8Array] }[] }

async function verify(path: string, canister: string, host: string): Promise<string[]> {
  const expected = JSON.parse(await readFile(path, 'utf8')) as Manifest
  const local = /^http:\/\/(127\.0\.0\.1|localhost|\[::1\])(:|\/|$)/.test(host)
  const agent = await HttpAgent.create({ host, shouldFetchRootKey: local })
  const actor = Actor.createActor<{ list(arg: Record<string, never>): Promise<Listed[]> }>(assetCanister, {
    agent,
    canisterId: canister
  })
  const served = new Map<string, string>()
  for (const asset of await actor.list({})) {
    const identity = asset.encodings.find((e) => e.content_encoding === 'identity')?.sha256[0]
    served.set(asset.key, identity ? Buffer.from(identity).toString('hex') : '')
  }
  const problems: string[] = []
  for (const [key, digest] of Object.entries(expected.assets)) {
    if (!served.has(key)) problems.push(`missing ${key}`)
    else if (served.get(key) !== digest) problems.push(`different ${key}`)
  }
  for (const key of served.keys()) if (!(key in expected.assets)) problems.push(`extra ${key}`)
  return problems
}

const [command, ...args] = process.argv.slice(2)
if (command === 'manifest') {
  console.log(JSON.stringify(await manifest(args[0] ?? process.env.DMSG_CONNECT_OUT ?? 'dist'), null, 2))
} else if (command === 'verify' && args.length >= 2) {
  const problems = await verify(args[0], args[1], args[2] ?? 'https://icp-api.io').catch((error: Error) => [
    `cannot list ${args[1]}: ${error.message.split('\n').find((l) => l.includes('Reject text')) ?? error.message}`
  ])
  for (const problem of problems) console.error(problem)
  if (problems.length) process.exit(1)
  console.log(`${args[1]} serves exactly the assets of ${args[0]}`)
} else {
  console.error('usage: node scripts/assets.ts manifest [dist] | verify <manifest.json> <canister> [host]')
  process.exit(2)
}
