/**
 * The Agents view's rules apart from the window: the Agent Protocols
 * services an agent may be granted, and the live grants as a matrix of
 * services and the origins they reach. Pure, so they are tested under node.
 */

/** The Agent Protocols services, with the capabilities that read and write each. */
export const services = [
  { name: 'Agent Discourse', read: 'discourse.read', write: 'discourse.write' },
  { name: 'Agent Mail', read: 'mail.read', write: 'mail.write' },
  { name: 'Agent Knowledge', read: 'knowledge.read', write: 'knowledge.write' }
] as const

/** Publishing the agent's own profile. */
export const profileWrite = 'agent.profile.write'

const reaching = new Set<string>([...services.flatMap((s) => [s.read, s.write]), profileWrite])

/** What else an agent may be granted; `<name>:*` covers every target. */
export const vocabulary = [
  'account.read',
  'vault.list',
  'vault.read:<tag>',
  'vault.write:<tag>',
  'vault.secret:<item>',
  'file.read:<tag>',
  'channel.read:<channel>',
  'channel.publish:<channel>',
  'inbox.read',
  'inbox.contact',
  'attest.request',
  'action.request',
  'agent.delegation.read',
  'events.endpoint'
]

export interface Reach {
  /** Each service capability with the origins it reaches, `*` for every allowed host. */
  services: Record<string, string[]>
  /** The capabilities that are not a service's. */
  other: string[]
}

/** The live grants (not revoked, not expired at `now`) by agent. */
export function matrix(grants: any[], now = Date.now()): Map<string, Reach> {
  const result = new Map<string, Reach>()
  for (const grant of grants) {
    if (grant.revoked_at || (grant.expires_at && grant.expires_at <= now)) continue
    const reach = result.get(grant.grantee) ?? { services: {}, other: [] }
    for (const capability of grant.capabilities as string[]) {
      const at = capability.indexOf(':')
      const name = at < 0 ? capability : capability.slice(0, at)
      if (reaching.has(name)) {
        const origins = (reach.services[name] ??= [])
        const origin = at < 0 ? '*' : capability.slice(at + 1)
        if (!origins.includes(origin)) origins.push(origin)
      } else if (!reach.other.includes(capability)) reach.other.push(capability)
    }
    result.set(grant.grantee, reach)
  }
  return result
}

/** An audit entry's outcome as a tone: done, refused, or something between. */
export const outcomeTone = (outcome: string): 'ok' | 'danger' | 'warn' =>
  outcome === 'ok' ? 'ok' : /denied|refused|forbidden|rejected|error/i.test(outcome) ? 'danger' : 'warn'
