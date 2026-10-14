import { HttpAgent } from '@icp-sdk/core/agent'
import { Ed25519KeyIdentity } from '@icp-sdk/core/identity'
import { hex } from 'dmsg-sdk/codec'
import { collectSnapshot, type SnapshotProof } from '../src'

const binary = ['args', 'nonce', 'requestId', 'certificate', 'reply'] as const

/** Proofs as legacy-name-plan.mjs reads them: the binary fields as lowercase hex. */
export function encodeProofs(proofs: SnapshotProof[]) {
  return proofs.map((proof) => ({
    ...proof,
    ...Object.fromEntries(binary.map((key) => [key, hex(proof[key])]))
  }))
}

/**
 * The frozen name and role snapshots of `source` (ic_message) and `identity`
 * (ic_name_identity), every page verified against the IC root key as it
 * arrives. The pages are `legacy_snapshot` updates signed by a fresh key that
 * holds nothing; a loopback host is a local replica whose root key is fetched.
 */
export async function collectNameProofs(input: { host: string; source: string; identity: string }) {
  const url = new URL(input.host)
  const local = url.protocol === 'http:' && ['127.0.0.1', 'localhost'].includes(url.hostname)
  if (!local && url.protocol !== 'https:') throw new Error('An HTTPS host is required')
  const agent = await HttpAgent.create({
    host: input.host,
    identity: Ed25519KeyIdentity.generate(),
    shouldFetchRootKey: false,
    verifyQuerySignatures: true
  })
  if (local) await agent.fetchRootKey()
  const names = await collectSnapshot(agent, input.source, { Names: null }, [input.source])
  const authorities = await collectSnapshot(agent, input.identity, { Authorities: null }, [
    input.identity
  ])
  return {
    names: encodeProofs(names.proofs),
    authorities: encodeProofs(authorities.proofs),
    counts: {
      names: names.entries.length,
      name_pages: names.proofs.length,
      authorities: authorities.entries.length,
      authority_pages: authorities.proofs.length
    }
  }
}
