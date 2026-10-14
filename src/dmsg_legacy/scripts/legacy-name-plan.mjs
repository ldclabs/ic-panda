import { readFile, writeFile, stat } from 'node:fs/promises'
import { loadEntry } from './bundle.mjs'
const args = new Map()
for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i], process.argv[i + 1])
for (const flag of [
  '--input',
  '--root-key-hex',
  '--source',
  '--identity',
  '--count',
  '--cutover-hex',
  '--output'
])
  if (!args.get(flag)) throw new Error(`Missing ${flag}`)
const fromHex = (value) => {
  if (!/^(?:[0-9a-f]{2})+$/.test(value)) throw new Error('Invalid lowercase hex')
  return Uint8Array.from(Buffer.from(value, 'hex'))
}
if ((await stat(args.get('--input'))).size > 16 * 1024 * 1024)
  throw new Error('Input exceeds 16 MiB')
// JSON proof transport encodes only binary proof fields as lowercase hex.
const input = JSON.parse(await readFile(args.get('--input'), 'utf8'))
const proofs = (values) =>
  values.map((value) => ({
    ...value,
    ...Object.fromEntries(
      ['args', 'nonce', 'requestId', 'certificate', 'reply'].map((key) => [
        key,
        fromHex(value[key])
      ])
    )
  }))
const { nameImportPlan } = await loadEntry('scripts/legacy-name-plan.ts')
const plan = await nameImportPlan({
  names: proofs(input.names),
  authorities: proofs(input.authorities),
  rootKey: fromHex(args.get('--root-key-hex')),
  source: args.get('--source'),
  identity: args.get('--identity'),
  expectedNames: Number(args.get('--count')),
  cutover: fromHex(args.get('--cutover-hex'))
})
await writeFile(args.get('--output'), JSON.stringify(plan, null, 2), {
  flag: 'wx',
  mode: 0o600
})
process.stdout.write(
  `Verified ${plan.count} frozen names; wrote ${plan.calls.length} unsigned calls. No network calls or signing performed.\n`
)
