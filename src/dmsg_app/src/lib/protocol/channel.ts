import { z } from 'zod'
import { ensure } from '../errors'
import { digest, hex } from './codec'

const hash = z.string().regex(/^[0-9a-f]{64}$/),
  account = z.string().regex(/^[0-9a-v]{19}[0g]$/),
  uint = z.number().int().nonnegative().safe()
export const channelGenesisSchema = z.strictObject({
  channel_id: hash,
  nonce: hash,
  type: z.enum(['direct', 'collaboration', 'distribution'])
})
export type ChannelRole = 'owner' | 'admin' | 'publisher' | 'member'
export interface ChannelMember {
  account: string
  role: ChannelRole
  joined_epoch: number
  can_rotate: boolean
  history: [number, number][]
}
export interface ChannelLedger {
  channel_id: string
  type: 'direct' | 'collaboration' | 'distribution'
  owner: string
  head: string
  control_seq: number
  acl_version: number
  epoch: number
  status: 'rotation_required' | 'active' | 'archived'
  manifest: string | null
  members: Record<string, ChannelMember>
  invitations: Record<
    string,
    { account: string; role: 'member' | 'publisher'; expires_at: number }
  >
  grants: Record<string, any>
}
export const channelControlSchema = z.strictObject({
  expected_head: hash,
  expected_acl_version: uint,
  action: z.discriminatedUnion('type', [
    z.strictObject({
      type: z.literal('invite'),
      invitation_id: hash,
      account,
      role: z.enum(['member', 'publisher']),
      expires_at: uint
    }),
    z.strictObject({ type: z.literal('accept'), invitation_id: hash }),
    z.strictObject({
      type: z.literal('role'),
      account,
      role: z.enum(['admin', 'member', 'publisher']),
      can_rotate: z.boolean()
    }),
    z.strictObject({ type: z.literal('remove'), account }),
    z.strictObject({ type: z.literal('leave') }),
    z.strictObject({ type: z.literal('archive') }),
    z.strictObject({
      type: z.literal('transfer'),
      account,
      device: hash,
      signature: z.string().max(90)
    }),
    z.strictObject({
      type: z.literal('history'),
      account,
      from_epoch: uint.min(1),
      to_epoch: uint.min(1),
      envelope_upload: hash
    }),
    z.strictObject({
      type: z.literal('grant'),
      grant: z.strictObject({
        grant_id: hash,
        issuer: account,
        origin: z.string().url().max(256),
        grantee_key: hash,
        resource: hash,
        actions: z
          .array(z.enum(['channel.read', 'channel.publish', 'file.read']))
          .min(1)
          .max(3),
        epoch_min: uint.min(1),
        epoch_max: uint.min(1),
        expires_at: uint,
        revoked: z.literal(false)
      })
    }),
    z.strictObject({ type: z.literal('revoke_grant'), grant_id: hash })
  ])
})
export type ChannelControl = z.infer<typeof channelControlSchema>
export interface EpochRecipient {
  recipient: string
  hpke_pub: string
}
export interface RotationLease {
  id: string
  account: string
  device: string
  epoch: number
  head: string
  recipients_digest: string
  security_versions: Record<string, number>
  fencing: number
  expires_at: number
}
export const activationSchema = z.strictObject({
  lease_id: hash,
  fencing: uint.min(1),
  expected_head: hash,
  epoch: uint.min(1),
  recipients_digest: hash,
  manifest_upload: hash,
  key_confirmation: hash,
  envelopes: z
    .array(
      z.strictObject({
        recipient: z.string().min(1).max(220),
        digest: hash,
        size: uint.min(1).max(2048),
        upload_id: hash,
        chunk: uint.max(127)
      })
    )
    .min(1)
    .max(600)
})
export type Activation = z.infer<typeof activationSchema>
export const channelMessageSchema = z.strictObject({
  channel_id: hash,
  epoch: uint.min(1),
  control_head: hash,
  acl_version: uint,
  message_id: hash,
  client_created_at: uint,
  ciphertext: z.string().max(44000)
})
export const recipientDigest = (recipients: EpochRecipient[]) =>
  hex(digest('dmsg/channel/recipients/v1', recipients))
export function readable(state: ChannelLedger, account: string, epoch: number) {
  const member = state.members[account]
  return (
    !!member &&
    epoch > 0 &&
    epoch <= state.epoch &&
    (epoch >= member.joined_epoch || member.history.some(([a, b]) => epoch >= a && epoch <= b))
  )
}
export function startChannel(payload: unknown, creator: string, head: string): ChannelLedger {
  const genesis = channelGenesisSchema.parse(payload)
  return {
    channel_id: genesis.channel_id,
    type: genesis.type,
    owner: creator,
    head,
    control_seq: 0,
    acl_version: 0,
    epoch: 0,
    status: 'rotation_required',
    manifest: null,
    members: {
      [creator]: {
        account: creator,
        role: 'owner',
        joined_epoch: 1,
        can_rotate: true,
        history: []
      }
    },
    invitations: {},
    grants: {}
  }
}
/** Independent client validation of the public control contract. The caller
 * verifies the event signature and any target-owner acceptance first. */
export function advanceChannel(
  previous: ChannelLedger,
  input: unknown,
  actor: string,
  head: string,
  acceptedAt: number
): ChannelLedger {
  const control = channelControlSchema.parse(input),
    action = control.action
  ensure(
    previous.status !== 'archived' &&
      control.expected_head === previous.head &&
      control.expected_acl_version === previous.acl_version,
    'UNVERIFIED_HEAD'
  )
  const state = structuredClone(previous),
    me = state.members[actor]
  state.invitations = Object.fromEntries(
    Object.entries(state.invitations).filter(([, v]) => v.expires_at > acceptedAt)
  )
  state.grants = Object.fromEntries(
    Object.entries(state.grants).filter(([, v]) => !v.revoked && v.expires_at > acceptedAt)
  )
  ensure(action.type === 'accept' || me, 'FORBIDDEN')
  const manager = me?.role === 'owner' || me?.role === 'admin'
  const target = 'account' in action ? state.members[action.account] : undefined
  const rotation = () => {
    state.status = 'rotation_required'
  }
  const removeInvites = (account: string) => {
    for (const [id, value] of Object.entries(state.invitations))
      if (value.account === account) delete state.invitations[id]
  }
  switch (action.type) {
    case 'invite':
      ensure(
        manager &&
          action.account !== actor &&
          !target &&
          !state.invitations[action.invitation_id] &&
          Object.keys(state.invitations).length < 100 &&
          action.expires_at > acceptedAt &&
          action.expires_at <= acceptedAt + 7 * 86400000,
        'FORBIDDEN'
      )
      state.invitations[action.invitation_id] = {
        account: action.account,
        role: action.role,
        expires_at: action.expires_at
      }
      break
    case 'accept': {
      const invitation = state.invitations[action.invitation_id]
      ensure(
        invitation?.account === actor &&
          !me &&
          Object.keys(state.members).length < (state.type === 'direct' ? 2 : 100),
        'FORBIDDEN'
      )
      state.members[actor] = {
        account: actor,
        role: invitation.role,
        joined_epoch: state.epoch + 1,
        can_rotate: false,
        history: []
      }
      removeInvites(actor)
      rotation()
      break
    }
    case 'role':
      ensure(me?.role === 'owner' && target && target.role !== 'owner', 'FORBIDDEN')
      target.role = action.role
      target.can_rotate = action.can_rotate
      break
    case 'remove':
      ensure(
        manager &&
          target &&
          actor !== action.account &&
          target.role !== 'owner' &&
          (me.role === 'owner' || ['member', 'publisher'].includes(target.role)),
        'FORBIDDEN'
      )
      delete state.members[action.account]
      removeInvites(action.account)
      rotation()
      break
    case 'leave':
      ensure(me.role !== 'owner', 'FORBIDDEN')
      delete state.members[actor]
      removeInvites(actor)
      rotation()
      break
    case 'transfer':
      ensure(me.role === 'owner' && target && action.account !== actor, 'FORBIDDEN')
      target.role = 'owner'
      target.can_rotate = true
      me.role = 'admin'
      state.owner = action.account
      break
    case 'archive':
      ensure(me.role === 'owner', 'FORBIDDEN')
      state.status = 'archived'
      break
    case 'history':
      ensure(
        manager &&
          target &&
          target.history.length < 64 &&
          action.from_epoch <= action.to_epoch &&
          action.to_epoch <= state.epoch &&
          action.to_epoch - action.from_epoch < 1000,
        'FORBIDDEN'
      )
      for (let e = action.from_epoch; e <= action.to_epoch; e++)
        ensure(readable(state, actor, e), 'FORBIDDEN')
      target.history.push([action.from_epoch, action.to_epoch])
      break
    case 'grant':
      ensure(
        manager &&
          action.grant.issuer === actor &&
          !state.grants[action.grant.grant_id] &&
          Object.keys(state.grants).length < 100 &&
          action.grant.epoch_min <= action.grant.epoch_max &&
          action.grant.epoch_max - action.grant.epoch_min < 1000 &&
          action.grant.expires_at > acceptedAt &&
          action.grant.expires_at <= acceptedAt + 30 * 86400000,
        'FORBIDDEN'
      )
      for (let e = action.grant.epoch_min; e <= action.grant.epoch_max; e++)
        ensure(readable(state, actor, e), 'FORBIDDEN')
      state.grants[action.grant.grant_id] = action.grant
      break
    case 'revoke_grant':
      ensure(manager && state.grants[action.grant_id], 'FORBIDDEN')
      state.grants[action.grant_id].revoked = true
      break
  }
  state.control_seq++
  state.acl_version++
  state.head = head
  return state
}
