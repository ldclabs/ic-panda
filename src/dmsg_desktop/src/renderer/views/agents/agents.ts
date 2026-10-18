/**
 * The Agents view's rules apart from the window: the Agent Protocols
 * services an agent may be granted, the live grants as a matrix of services
 * and the origins they reach, what a typed capability may name, the MCP
 * client setup for an Agent ID and the profile hint of a credential. Pure,
 * so they are tested under node.
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

export interface Reach {
  /** Each service capability with the origins it reaches, `*` for every allowed host. */
  services: Record<string, string[]>
  /** The capabilities that are not a service's. */
  other: string[]
}

/** Whether a grant is still in force at `now`: not revoked, not expired. */
export const live = (grant: any, now = Date.now()) => !grant.revoked_at && !(grant.expires_at && grant.expires_at <= now)

/** A grant's state at `now`. */
export const grantState = (grant: any, now = Date.now()): 'revoked' | 'expired' | 'live' =>
  grant.revoked_at ? 'revoked' : grant.expires_at && grant.expires_at <= now ? 'expired' : 'live'

/** The live grants (not revoked, not expired at `now`) by agent. */
export function matrix(grants: any[], now = Date.now()): Map<string, Reach> {
  const result = new Map<string, Reach>()
  for (const grant of grants) {
    if (!live(grant, now)) continue
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

/**
 * The typed capabilities the daemon's vocabulary does not name, such as
 * `vocabulary` entries `vault.read:<tag>` or `mail.read:<origin>`: a target
 * is required where one is named and is `*` or, for an origin, a bare
 * `https://` or `http://` origin. The daemon checks again.
 */
export function unknownCapabilities(capabilities: string[], vocabulary: string[]): string[] {
  return capabilities.filter(
    (capability) =>
      !vocabulary.some((entry) => {
        const at = entry.indexOf(':<')
        if (at < 0) return capability === entry
        const prefix = entry.slice(0, at + 1)
        const target = capability.startsWith(prefix) ? capability.slice(prefix.length) : ''
        if (!target || target.length > 128 || /\s/.test(target)) return false
        if (target === '*') return true
        if (entry.endsWith(':<origin>')) return /^https?:\/\/[^/?#\s]+$/.test(target)
        return !target.startsWith('dmsg-')
      })
  )
}

/** Whether granting a capability deserves a second look, as the daemon decides it. */
export const sensitive = (capability: string) =>
  capability.endsWith(':*') || capability.startsWith('vault.secret:') || capability.startsWith('wallet.execute:')

/** A shell word for a POSIX shell, quoted only when it needs it. */
export const shellWord = (word: string) => (/^[\w./:@%+=,-]+$/.test(word) ? word : `'${word.replaceAll("'", `'\\''`)}'`)

/**
 * How an MCP client runs `dmsg mcp` as the Agent ID `label` with its
 * credential: the Claude Code command, and the `mcpServers` entry Claude
 * Desktop, Cursor and others read.
 */
export function mcpSetup(runtime: { binary: string; home: string }, label: string, credential: string) {
  const name = `dmsg-${label}`
  const args = ['--home', runtime.home, 'mcp']
  const command = ['claude', 'mcp', 'add', name, '--env', `DMSG_AGENT_TOKEN=${credential}`, '--', runtime.binary, ...args].map(shellWord).join(' ')
  const config = JSON.stringify({ mcpServers: { [name]: { command: runtime.binary, args, env: { DMSG_AGENT_TOKEN: credential } } } }, null, 2)
  return { command, config }
}

/** A credential's Agent Profile delegation hint (Agent Profile §4), for the agent to publish with its own key. */
export function profileHint(principal: { principal_id: string; type?: string }, credential: any) {
  return {
    id: credential.id,
    principal: { id: principal.principal_id, type: String(principal.type ?? 'person').toLowerCase() },
    ...(credential.relationship ? { relationship: credential.relationship } : {}),
    scopes: credential.scopes
  }
}

/** Credentials by subject, newest first within each, subjects in the order first met. */
export function bySubject(credentials: any[]): [string, any[]][] {
  const groups = new Map<string, any[]>()
  for (const credential of [...credentials].sort((a, b) => (b.accepted_at ?? 0) - (a.accepted_at ?? 0))) {
    const list = groups.get(credential.subject) ?? []
    list.push(credential)
    groups.set(credential.subject, list)
  }
  return [...groups]
}

/** Whether a controller's ceiling covers `scopes` for `audiences`. */
export function covers(controller: any, scopes: string[], audiences: string[]): boolean {
  if (controller.delegation === '*') return true
  const { scopes: ceiling, audiences: parties } = controller.delegation
  return scopes.every((s) => ceiling.includes(s)) && audiences.every((a) => parties.includes(a))
}

/** An audit entry's outcome as a tone: done, refused, or something between. */
export const outcomeTone = (outcome: string): 'ok' | 'danger' | 'warn' =>
  outcome === 'ok' ? 'ok' : /denied|refused|forbidden|rejected|error/i.test(outcome) ? 'danger' : 'warn'

/** A credential's status as a tone. */
export const statusTone = (status: string): 'ok' | 'warn' | 'neutral' => (status === 'active' ? 'ok' : status === 'suspended' ? 'warn' : 'neutral')

/** Comma-separated values, trimmed, empty ones dropped. */
export const list = (text: string) =>
  text
    .split(',')
    .map((v) => v.trim())
    .filter(Boolean)
