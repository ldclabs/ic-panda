/** Prepared daemon plans for UI tests. Product policy is tested in Rust vectors. */
import { createHash } from 'node:crypto'

const hash = (value) => createHash('sha256').update(value).digest('hex')
const clone = (value) => JSON.parse(JSON.stringify(value))

export function notificationsFixture(account, changed) {
  const categories = Object.fromEntries(['security', 'action', 'message', 'service'].map((kind) => [kind, { enabled: true, sound: kind === 'security' }]))
  let global = { revision: 1, enabled: true, paused_until: null, quiet_hours: null, security_exception: false, badge: 'all' }
  const settings = new Map([[account, { revision: 1, enabled: true, categories, merge_devices: true, agent_mail: {}, remote_revision: 1 }]])
  const items = new Map(), history = new Map(), pending = [], removed = []
  let context = { foreground: false, visible: [] }, recoveryDigest = null
  const category = (target) => target.kind === 'recovery' ? 'security'
    : ['approval', 'task', 'wallet'].includes(target.kind) ? 'action'
    : ['plan', 'service'].includes(target.kind) ? 'service'
    : target.kind === 'summary' ? target.id : 'message'
  const identity = (target, owner = account) => hash(JSON.stringify([owner, target]))
  const row = (target, values = {}, owner = account) => {
    const item = { id: identity(target, owner), account: owner, target, group: identity(target, owner), version: '1', open: true,
      actionable: ['action', 'security'].includes(category(target)), unread: category(target) === 'message' ? 1 : 0,
      first_at: Date.now(), updated_at: Date.now(), deadline: null, observed: '0', notified: '0', stage: 0,
      attempted_at: null, pending_since: Date.now(), snoozed_until: null, muted: false, muted_until: null, quiet: false, ...values }
    items.set(item.id, item)
    return item
  }
  const enqueue = (item, values = {}) => {
    const plan = { system_id: hash(`group:${item.id}`), delivery_id: hash(`delivery:${item.id}:${item.version}:${item.snoozed_until}`), account: item.account,
      category: category(item.target), target: item.target, covered: [{ id: item.id, version: item.version, stage: 1 }], count: item.unread,
      sound: category(item.target) === 'security', in_app: context.foreground, update: false, attempted_at: Date.now(), ...values }
    pending.push(plan)
    changed('notifications/changed')
    return plan
  }
  const ref = (target) => {
    const item = items.get(identity(target))
    return item ? { id: item.id, version: item.version } : null
  }
  const methods = {
    'notifications/migrate': () => ({imported:true,awaiting_account:0}),
    'notifications/settings': (p = {}) => ({ global: clone(global), account: clone(settings.get(p.account ?? account) ?? null), account_id: p.account ?? account, remote_supported: false }),
    'notifications/settings_set': (p) => {
      const old = p.scope === 'installation' ? global : settings.get(p.account ?? account)
      if (!old || old.revision !== p.expected_revision) throw { code: 'VERSION_CONFLICT', message: 'Settings changed. Review and save again.' }
      const value = { ...clone(p.settings), revision: old.revision + 1 }
      if (p.scope === 'installation') global = value
      else { value.remote_revision = old.remote_revision; settings.set(p.account ?? account, value) }
      changed('notifications/changed')
      return { settings: clone(value), pending_sync: true }
    },
    'notifications/context': (p) => { context = clone(p.context); return null },
    'notifications/list': (p = {}) => {
      let list = [...items.values()].filter((item) => item.account === (p.account ?? account))
      if (p.id) list = list.filter((item) => item.id === p.id)
      else if ((p.scope ?? 'open') === 'open') list = list.filter((item) => item.open && item.actionable)
      return { items: clone(list), next_cursor: null }
    },
    'notifications/status': () => {
      const values = [...items.values()].filter((item) => item.open)
      const counts = { actionable: values.filter((item) => item.actionable).length, unread: values.filter((item) => category(item.target) === 'message' && !item.muted && item.target.kind !== 'agent_mail').reduce((sum, item) => sum + (item.unread ?? 0), 0), unverified: values.some((item) => item.unread === null) }
      return { counts, accounts: [...settings].map(([account, value]) => ({ account, enabled: value.enabled, counts, pending_sync: value.revision > value.remote_revision })), badge: global.badge === 'off' ? 0 : counts.actionable + (global.badge === 'actions' ? 0 : counts.unread), enabled: global.enabled }
    },
    'notifications/next': (p = {}) => {
      const plans = pending.splice(0).filter((plan) => global.enabled && settings.get(plan.account)?.enabled && settings.get(plan.account).categories[plan.category].enabled
        && !plan.covered.every((covered) => items.get(covered.id)?.observed === covered.version))
      for (const plan of plans) plan.in_app = context.foreground
      for (const plan of plans) history.set(plan.system_id, plan)
      return { plans: clone(plans), remove: removed.splice(0), history: p.restore ? clone([...history.values()]) : [], next_due: null }
    },
    'notifications/result': (p) => { const plan = history.get(p.system_id); if (plan?.delivery_id === p.delivery_id) plan.result = p.result; return null },
    'notifications/observed': (p) => {
      const item = items.get(p.id ?? identity(p.target, p.account ?? account))
      if (!item) throw { code: 'NOT_FOUND', message: 'No reminder' }
      if (p.version && p.version !== item.version) throw { code: 'VERSION_CONFLICT', message: 'Item changed' }
      item.observed = item.version
      return { id: item.id, version: item.version, stage: item.stage }
    },
    'notifications/snooze': (p) => {
      const item = items.get(p.id)
      if (!item || !item.open || !item.actionable || item.version !== p.version || (item.deadline && p.until >= item.deadline)) throw { code: 'INVALID_INPUT', message: 'Reminder deadline' }
      item.snoozed_until = p.until
      changed('notifications/changed')
      return null
    },
    'notifications/clear_recent': () => { for (const [id, item] of items) if (!item.open) items.delete(id); changed('notifications/changed'); return null }
  }
  return {
    methods, items, settings, row, enqueue, ref,
    recovery(warning) {
      if (!warning) return
      if (!warning.pending) {
        for (const item of items.values()) if (item.target.kind === 'recovery' && item.open) {
          item.open = false; item.actionable = false; removed.push(hash(`group:${item.id}`))
        }
        recoveryDigest = null
        changed('notifications/changed')
      } else if (recoveryDigest !== warning.digest) {
        recoveryDigest = warning.digest
        enqueue(row({ kind: 'recovery', id: warning.digest }))
      }
    }
  }
}
