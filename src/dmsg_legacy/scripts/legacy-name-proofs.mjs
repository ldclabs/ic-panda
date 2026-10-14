// Collects the frozen name and role snapshots that legacy-name-plan.mjs reads.
// Calls only legacy_snapshot, signed by a fresh key; changes no service.
import { writeFile } from 'node:fs/promises'
import { loadEntry } from './bundle.mjs'
const args = new Map([['--host', 'https://icp-api.io']])
for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i], process.argv[i + 1])
for (const flag of ['--source', '--identity', '--output'])
  if (!args.get(flag)) throw new Error(`Missing ${flag}`)
const { collectNameProofs } = await loadEntry('scripts/legacy-name-proofs.ts')
const { names, authorities, counts } = await collectNameProofs({
  host: args.get('--host'),
  source: args.get('--source'),
  identity: args.get('--identity')
})
await writeFile(args.get('--output'), JSON.stringify({ names, authorities }), {
  flag: 'wx',
  mode: 0o600
})
process.stdout.write(
  `Verified ${counts.names} frozen names (${counts.name_pages} pages) and ${counts.authorities} name accounts (${counts.authority_pages} pages). Only legacy_snapshot was called.\n`
)
