// Creates the canisters on the local icp network at their mainnet IDs and links
// them, since `icp canister create` cannot take an ID. A managed network starts
// empty, so run this after every `icp network start`.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { HttpAgent, pollForResponse } from '@icp-sdk/core/agent'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'

const root = resolve(import.meta.dirname, '..')
const icp = (...args) => execFileSync('icp', args, { cwd: root, encoding: 'utf8' }).trim()

const ids = {
  ...JSON.parse(readFileSync(`${root}/.icp/data/mappings/ic.ids.json`, 'utf8')),
  // Local stand-in for the PANDA SNS ledger, which this project does not deploy.
  icrc1_ledger_canister: 'druyg-tyaaa-aaaaq-aactq-cai'
}
const { api_url, root_key } = JSON.parse(icp('network', 'status', '--json'))
const controller = Principal.fromText(icp('identity', 'principal'))
const agent = await HttpAgent.create({
  host: api_url,
  rootKey: Uint8Array.from(Buffer.from(root_key, 'hex'))
})

const CreateArgs = IDL.Record({
  amount: IDL.Opt(IDL.Nat),
  settings: IDL.Opt(IDL.Record({ controllers: IDL.Opt(IDL.Vec(IDL.Principal)) })),
  specified_id: IDL.Opt(IDL.Principal)
})

for (const [name, id] of Object.entries(ids)) {
  const canisterId = Principal.fromText(id)
  const arg = IDL.encode(
    [CreateArgs],
    [
      {
        amount: [100_000_000_000_000n],
        settings: [{ controllers: [[controller]] }],
        specified_id: [canisterId]
      }
    ]
  )
  try {
    // The management canister routes by the effective canister ID, which must
    // be the requested ID.
    const { requestId } = await agent.call(Principal.managementCanister(), {
      methodName: 'provisional_create_canister_with_cycles',
      arg,
      effectiveCanisterId: canisterId
    })
    await pollForResponse(agent, canisterId, requestId)
  } catch (err) {
    // IC0538: the canister already exists, as on a second run.
    if (err.cause?.code?.rejectErrorCode !== 'IC0538') throw err
  }
  icp('canister', 'link', name, id, '--force')
}
