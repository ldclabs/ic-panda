// Drafts the inputs of legacy-freeze-plan.mjs from a legacy-inventory.mjs
// observation and a canisters release: the targets, pinned to the release's
// modules, and a baseline review whose open items a person must clear before
// the plan can be made. Reads local files only.
import { createHash } from 'node:crypto'
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

// The canisters that freeze through admin_legacy_*: inventory kind → plan kind and release module.
const SOURCES = {
  message: ['message', 'ic_message'],
  channel: ['channel', 'ic_message_channel'],
  profile: ['profile', 'ic_message_profile'],
  name_identity: ['identity', 'ic_name_identity']
}

export const MODULES = Object.values(SOURCES).map(([, module]) => module)

/** `modules` maps each release module (`ic_message`, …) to the SHA-256 of its `.wasm.gz`. */
export function freezeInputs(inventory, release, modules) {
  const targets = [],
    sources = [],
    unresolved = []
  for (const canister of inventory.canisters ?? []) {
    if (!SOURCES[canister.kind]) continue
    const [kind, module] = SOURCES[canister.kind]
    const sha = modules[module]
    if (!/^[0-9a-f]{64}$/.test(sha ?? ''))
      throw new Error(`${module}.wasm.gz is not in the release`)
    const observed = canister.metadata?.module_hash ?? null,
      name = `${canister.names?.[0] ?? module} ${canister.principal}`
    targets.push({ canister: canister.principal, kind, module_sha256: sha })
    sources.push({
      canister: canister.principal,
      module_sha256: sha,
      observed_module_sha256: observed,
      controllers: canister.metadata?.controllers ?? [],
      evidence_ref: ''
    })
    if (observed !== sha)
      unresolved.push(
        `${name} runs ${observed ?? 'an unknown module'}: upgrade it to ${module}.wasm.gz of ${release} (${sha}), observe again with legacy-inventory.mjs and draft these files again.`
      )
    unresolved.push(
      `${name}: confirm the upgrade to ${sha} stopped the canister first, so nothing the old module started was in flight; put the upgrade proposal in evidence_ref, then delete this line.`
    )
  }
  if (!targets.length) throw new Error('The inventory names no legacy canister that freezes')
  return {
    targets: { targets },
    baseline: {
      format: 'dmsg-legacy-baseline-review/1',
      release,
      inventory_started_at: inventory.started_at ?? null,
      sources,
      unresolved
    }
  }
}

/** Each module's `.wasm.gz` in `dir`, checked against the `<file>.<sha256>.txt` the release lists. */
export async function releaseModules(dir) {
  const modules = {}
  for (const module of MODULES) {
    const sha = createHash('sha256')
      .update(await readFile(join(dir, `${module}.wasm.gz`)))
      .digest('hex')
    await stat(join(dir, `${module}.wasm.gz.${sha}.txt`)).catch(() => {
      throw new Error(`${module}.wasm.gz does not match its release checksum`)
    })
    modules[module] = sha
  }
  return modules
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [inventory, dir, release, output] = process.argv.slice(2)
  if (!inventory || !dir || !release || !output)
    throw new Error(
      'Usage: node legacy-freeze-inputs.mjs <inventory.json> <release-dir> <release-tag> <output-dir>'
    )
  const result = freezeInputs(
    JSON.parse(await readFile(inventory, 'utf8')),
    release,
    await releaseModules(dir)
  )
  await mkdir(output, { recursive: true })
  for (const [file, value] of [
    ['targets.json', result.targets],
    ['baseline.json', result.baseline]
  ])
    await writeFile(join(output, file), JSON.stringify(value, null, 2) + '\n', {
      flag: 'wx',
      mode: 0o600
    })
  process.stdout.write(
    `Drafted ${result.targets.targets.length} targets; ${result.baseline.unresolved.length} review items are open in baseline.json.\n`
  )
}
