// Generates reviewable controller payloads. Never signs, submits, upgrades or freezes a service.
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { createHash } from 'node:crypto'
import { readFile, writeFile } from 'node:fs/promises'
import { pathToFileURL } from 'node:url'

const blob = IDL.Vec(IDL.Nat8)
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex')
export function freezePlan(inventory, baseline, cutover) {
  if (!/^[0-9a-f]{64}$/.test(cutover) || /^0+$/.test(cutover))
    throw new Error('A nonzero cutover ID is required')
  if (
    baseline.format !== 'dmsg-legacy-baseline-review/1' ||
    !Array.isArray(baseline.sources) ||
    !Array.isArray(baseline.unresolved) ||
    baseline.unresolved.length
  )
    throw new Error('A completed baseline review or explicit resolution is required')
  if (
    !Array.isArray(inventory.targets) ||
    !inventory.targets.length ||
    inventory.targets.length > 1000
  )
    throw new Error('Invalid target inventory')
  const evidence = digest(Buffer.from(JSON.stringify(baseline))),
    id = Buffer.from(cutover, 'hex'),
    proof = Buffer.from(evidence, 'hex')
  const before = [],
    draining = [],
    sealing = [],
    seen = new Set()
  function call(target, method, types, values) {
    return {
      canister: target.canister,
      expected_module_sha256: target.module_sha256,
      method,
      candid_hex: Buffer.from(IDL.encode(types, values)).toString('hex')
    }
  }
  for (const target of inventory.targets) {
    if (
      Principal.fromText(target.canister).toText() !== target.canister ||
      seen.has(target.canister) ||
      !/^[0-9a-f]{64}$/.test(target.module_sha256)
    )
      throw new Error('Invalid, duplicate or unpinned source')
    seen.add(target.canister)
    if (
      !baseline.sources.some(
        (source) =>
          source.canister === target.canister &&
          source.module_sha256 === target.module_sha256 &&
          source.evidence_ref
      )
    )
      throw new Error('Baseline review does not match the target module')
    if (['message', 'profile', 'channel', 'identity'].includes(target.kind)) {
      before.push(call(target, 'admin_legacy_acknowledge_baseline', [blob], [proof]))
      draining.push(call(target, 'admin_legacy_drain', [blob], [id]))
      sealing.push(call(target, 'admin_legacy_seal', [blob], [id]))
    } else if (target.kind === 'cose' || target.kind === 'oss') {
      if (
        !Array.isArray(target.scopes) ||
        !target.scopes.length ||
        target.scopes.length > 4096 ||
        new Set(target.scopes).size !== target.scopes.length
      )
        throw new Error('Explicit unique namespace/folder scopes are required')
      for (const scope of target.scopes) {
        const folder = target.kind === 'oss'
        if (
          folder
            ? !Number.isInteger(scope) || scope <= 0 || scope > 0xffffffff
            : typeof scope !== 'string' || !/^[a-z0-9_]{1,128}$/.test(scope)
        )
          throw new Error('Invalid legacy scope')
        const type = folder ? IDL.Nat32 : IDL.Text,
          suffix = folder ? 'folder' : 'namespace'
        draining.push(
          call(target, `admin_legacy_drain_${suffix}`, [type, blob, blob], [scope, id, proof])
        )
        sealing.push(call(target, `admin_legacy_seal_${suffix}`, [type, blob], [scope, id]))
      }
    } else throw new Error('Unsupported legacy component')
  }
  return {
    format: 'dmsg-legacy-controller-plan/1',
    executes: false,
    cutover_id: cutover,
    baseline_evidence: evidence,
    stages: [
      { name: 'baseline_review', calls: before },
      { name: 'drain_all_scopes', calls: draining },
      {
        name: 'inspect_and_reconcile_pending',
        calls: [],
        required:
          'Read current statuses, resolve unknown operations from original receipts, and retain source evidence before sealing.'
      },
      { name: 'seal_after_drain', calls: sealing },
      {
        name: 'capture_final_snapshots',
        calls: [],
        required:
          'Collect verified name/role/channel/message pages and OSS ciphertext manifests. Compare counts, bytes and the final delta before any product cutover.'
      }
    ]
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [inventory, baseline, cutover, output] = process.argv.slice(2)
  if (!inventory || !baseline || !cutover || !output)
    throw new Error(
      'Usage: node legacy-freeze-plan.mjs <inventory.json> <baseline-review.json> <cutover-hex> <output.json>'
    )
  const result = freezePlan(
    JSON.parse(await readFile(inventory, 'utf8')),
    JSON.parse(await readFile(baseline, 'utf8')),
    cutover
  )
  await writeFile(output, JSON.stringify(result, null, 2) + '\n', { flag: 'wx' })
  process.stdout.write(
    `Prepared ${result.stages.reduce((n, stage) => n + stage.calls.length, 0)} unsigned controller calls. No service was changed.\n`
  )
}
