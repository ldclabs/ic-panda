/** The single native presentation adapter. Policy and persistence live in Rust. */
import { createHash } from 'node:crypto'
import { isNoticePlan, type NoticeCategory, type NoticePlan } from '../shared/notifications.ts'

export interface NativeNotice {
  id?: string
  show(): void
  close(): void
  on(event: 'click' | 'show' | 'failed', listener: (...args: any[]) => void): unknown
}

interface Host {
  create(options: { id: string; groupId: string; title: string; body: string; silent: boolean }): NativeNotice
  body(category: NoticeCategory, count: number | null): string
  foreground(): boolean
  inApp(plan: NoticePlan): void
  open(plan: NoticePlan): Promise<void>
  result(plan: NoticePlan, result: 'submitted' | 'shown' | 'failed' | 'suppressed'): Promise<unknown>
  remove?(id: string): void
  /** Windows supplies persistent activations separately from instance clicks. */
  activation?: boolean
}

/** Fixed-length opaque platform ID, also within Windows toast tag limits. */
export const nativeNoticeId = (systemId: string): string => createHash('sha256').update(systemId).digest('base64url').slice(0, 16)

export class Notifications {
  private shown = new Map<string, string>()
  private notes = new Map<string, NativeNotice>()
  private routes = new Map<string, NoticePlan>()
  private testNote: NativeNotice | null = null
  private host: Host

  constructor(host: Host) { this.host = host }

  private settle(plan: NoticePlan, outcome: 'submitted' | 'shown' | 'failed' | 'suppressed') {
    void this.host.result(plan, outcome).catch(() => { /* The persisted original ID reconciles on reconnect. */ })
  }

  private attach(note: NativeNotice, plan: NoticePlan) {
    const id = nativeNoticeId(plan.system_id)
    this.notes.set(plan.system_id, note)
    this.routes.set(id, plan)
    if (!this.host.activation) note.on('click', () => { void this.host.open(this.routes.get(id) ?? plan).catch(() => {}) })
    note.on('show', () => {
      if (this.notes.get(plan.system_id) === note) this.settle(plan, 'shown')
    })
    note.on('failed', () => {
      if (this.notes.get(plan.system_id) !== note) return
      this.shown.delete(plan.system_id)
      this.settle(plan, 'failed')
    })
  }

  present(value: unknown) {
    if (!isNoticePlan(value)) return
    const plan = value
    this.routes.set(nativeNoticeId(plan.system_id), plan)
    if (this.shown.get(plan.system_id) === plan.delivery_id) {
      this.settle(plan, 'submitted')
      return
    }
    // Focus can change between the daemon's decision and the native call.
    // Foreground delivery is always a lightweight application hint, without sound.
    if (this.host.foreground()) {
      this.shown.set(plan.system_id, plan.delivery_id)
      this.host.inApp(plan)
      this.settle(plan, plan.in_app ? 'submitted' : 'suppressed')
      return
    }
    if (plan.in_app || plan.update) {
      this.settle(plan, 'suppressed')
      return
    }
    try {
      const note = this.host.create({
        id: nativeNoticeId(plan.system_id), groupId: nativeNoticeId(`${plan.account}:${plan.category}`),
        title: 'dMsg', body: this.host.body(plan.category, plan.count), silent: !plan.sound
      })
      this.attach(note, plan)
      let failed = false
      note.on('failed', () => { failed = true })
      this.shown.set(plan.system_id, plan.delivery_id)
      note.show()
      if (!failed) this.settle(plan, 'submitted')
    } catch {
      this.shown.delete(plan.system_id)
      this.settle(plan, 'failed')
    }
  }

  remove(systemIds: string[]) {
    for (const id of systemIds) {
      try { this.notes.get(id)?.close(); this.host.remove?.(nativeNoticeId(id)) } catch { /* OS best effort. */ }
      this.notes.delete(id)
      this.routes.delete(nativeNoticeId(id))
    }
  }

  /** Reattach delivered history without showing it again. */
  restore(history: NativeNotice[], plans: unknown[]) {
    for (const plan of plans.filter(isNoticePlan)) this.routes.set(nativeNoticeId(plan.system_id), plan)
    for (const note of history) {
      const plan = note.id ? this.routes.get(note.id) : undefined
      if (!plan || this.notes.has(plan.system_id)) continue
      this.attach(note, plan)
      this.shown.set(plan.system_id, plan.delivery_id)
    }
  }

  async activate(nativeId: string): Promise<boolean> {
    const plan = this.routes.get(nativeId)
    if (!plan) return false
    await this.host.open(plan)
    return true
  }

  test(body: string, result: (outcome: 'submitted' | 'shown' | 'failed') => void) {
    let answered = false
    const note = this.host.create({ id: 'dmsg-test', groupId: 'dmsg-test', title: 'dMsg', body, silent: true })
    this.testNote = note
    note.on('show', () => { answered = true; if (this.testNote === note) result('shown') })
    note.on('failed', () => { answered = true; if (this.testNote === note) result('failed') })
    note.show()
    if (!answered) result('submitted')
  }
}
