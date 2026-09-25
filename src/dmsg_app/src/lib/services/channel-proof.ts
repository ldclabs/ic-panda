import { Principal } from '@icp-sdk/core/principal'
import type { HttpAgent } from '@icp-sdk/core/agent'
import { certifiedValue } from './certified'
import type { CloudSecurityEvidence } from './cloud-security'
import {
  readCloudCommand,
  verifyCloudCommand,
  type CloudSigned,
  type CloudAction
} from '../protocol/cloud'
import { decodeCanonical, digest, equal, hex, unb64 } from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import { ensure } from '../errors'

export interface StoredChannelEvent {
  signed: CloudSigned
  evidence: CloudSecurityEvidence
  acceptance_evidence?: CloudSecurityEvidence | null
  stored_at: number
  head: string
  control_seq: number
}
export interface ChannelTrust {
  home: string
  namespace: string
  agent: HttpAgent
}
/** Historical certificate time is checked against the retained relay acceptance
 * time. This proves the key/authority binding, not an external trusted timestamp. */
export async function channelAccountEvidence(
  evidence: CloudSecurityEvidence,
  account: string,
  at: number,
  trust: ChannelTrust
) {
  ensure(
    evidence?.schema === 1 &&
      evidence.canister === trust.home &&
      Number.isSafeInteger(at) &&
      at > 0 &&
      at <= Date.now() + 5000,
    'INTEGRITY_FAILED'
  )
  const entries = evidence.entries.filter((e) => e.account_id === account)
  ensure(entries.length === 1 && evidence.entries.length <= 64, 'INTEGRITY_FAILED')
  const entry = entries[0],
    raw = xidBytes(account)
  const proof = await certifiedValue(
    {
      schema: 1,
      canister: Principal.fromText(trust.home),
      certificate: unb64(evidence.certificate, 65536),
      entries: [
        { key: raw, value: [unb64(entry.value, 65536)], witness: unb64(entry.witness, 262144) }
      ]
    },
    trust.agent,
    trust.home,
    raw,
    at
  )
  const snapshot = decodeCanonical<Record<string, any>>(proof.value),
    devices = decodeCanonical<Map<Uint8Array, any>>(unb64(entry.devices, 65536))
  ensure(
    snapshot.schema === 2 &&
      snapshot.issuer === `${trust.namespace}${account}` &&
      snapshot.account_status === 'Active' &&
      equal(snapshot.account_id, raw) &&
      equal(snapshot.home_user, Principal.fromText(trust.home).toUint8Array()) &&
      devices instanceof Map &&
      devices.size <= 16 &&
      equal(snapshot.devices_root, digest('dmsg/devices/v1', devices)),
    'INTEGRITY_FAILED'
  )
  return { snapshot, devices, certifiedAt: proof.certifiedAt }
}
export async function channelDevice(
  evidence: CloudSecurityEvidence,
  account: string,
  device: string,
  at: number,
  trust: ChannelTrust
) {
  const { snapshot, devices, certifiedAt } = await channelAccountEvidence(
    evidence,
    account,
    at,
    trust
  )
  const found = [...devices].filter(
    ([key]) => key instanceof Uint8Array && hex(key) === device
  )
  ensure(found.length === 1, 'AUTH_REQUIRED')
  const value = found[0][1]
  ensure(
    value.revoked_at === null &&
      hex(value.input.device_id) === device &&
      value.input.signing_pub instanceof Uint8Array &&
      value.input.signing_pub.length === 32 &&
      value.input.capabilities.includes('ContentSign'),
    'AUTH_REQUIRED'
  )
  return { snapshot, device: value.input, certifiedAt }
}
export async function verifyChannelEvent(
  event: Pick<StoredChannelEvent, 'signed' | 'evidence' | 'stored_at'>,
  trust: ChannelTrust
) {
  const parsed = readCloudCommand(event.signed)
  ensure(parsed.issuer.startsWith(trust.namespace), 'AUTH_REQUIRED')
  const account = parsed.issuer.slice(trust.namespace.length),
    deviceId = hex(parsed.kid)
  xidBytes(account)
  const authority = await channelDevice(
    event.evidence,
    account,
    deviceId,
    event.stored_at,
    trust
  )
  ensure(
    BigInt(parsed.body.security_epoch) === BigInt(authority.snapshot.security_epoch) &&
      parsed.body.deadline > event.stored_at,
    'POLICY_STALE'
  )
  const body = verifyCloudCommand(
    event.signed,
    { issuer: parsed.issuer, deviceId },
    authority.device.signing_pub,
    parsed.body.action as CloudAction
  )
  return {
    body,
    account,
    deviceId,
    authority,
    unsigned: { ...body, issuer: parsed.issuer, account_id: account, device_id: deviceId }
  }
}
