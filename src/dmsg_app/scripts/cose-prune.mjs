// Anonymous public maintenance for dmsg_cose: page through prune_executions
// until the pass ends. Each page inspects at most 64 accounts; pruning keeps
// replay guards, so a repeated or interrupted pass is harmless.
import { Actor, AnonymousIdentity, HttpAgent } from '@icp-sdk/core/agent'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { parseArgs } from 'node:util'
import { idlFactory } from '../src/lib/canisters/generated/cose/index.js'

const hex = (bytes) => Buffer.from(bytes).toString('hex')

/** Run one cleanup pass from `after` (an `opt blob` cursor) to its end. */
export async function prunePass(cose, after = [], log = () => {}) {
  let pages = 0
  let removed = 0
  do {
    let page
    try {
      page = await cose.prune_executions(after)
    } catch (error) {
      // Resume from the last finished page with --after.
      error.message += after.length ? ` (resume with --after ${hex(after[0])})` : ''
      throw error
    }
    pages += 1
    removed += page.results_removed
    log(page)
    after = page.next_after
  } while (after.length > 0)
  return { pages, removed }
}

async function main() {
  const { values } = parseArgs({
    options: {
      canister: { type: 'string' },
      host: { type: 'string', default: 'https://icp-api.io' },
      after: { type: 'string' }
    }
  })
  if (!values.canister)
    throw new Error('Usage: node cose-prune.mjs --canister <dmsg_cose ID> [--host URL] [--after HEX]')
  const url = new URL(values.host)
  const local = ['localhost', '127.0.0.1'].includes(url.hostname)
  if (url.protocol !== 'https:' && !local) throw new Error('HTTPS host required')
  const agent = await HttpAgent.create({
    host: url.origin,
    identity: new AnonymousIdentity(),
    // Only a local replica has a root key of its own.
    shouldFetchRootKey: local
  })
  const cose = Actor.createActor(idlFactory, { agent, canisterId: values.canister })
  const after = values.after ? [Uint8Array.from(Buffer.from(values.after, 'hex'))] : []
  const { pages, removed } = await prunePass(cose, after)
  console.log(`Scanned ${pages} pages and removed ${removed} expired results.`)
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href)
  main().catch((error) => {
    console.error(error.message)
    process.exitCode = 1
  })
